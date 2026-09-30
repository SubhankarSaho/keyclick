//! macOS keyboard detection via a listen-only Quartz event tap.
//!
//! `kCGEventTapOptionListenOnly` means the tap can observe but never modify,
//! delay or drop events. It requires the "Input Monitoring" privacy permission
//! (kTCCServiceListenEvent). When Secure Event Input is active (e.g. password
//! fields) macOS does not deliver key events to taps at all.

use std::ffi::c_void;
use std::sync::mpsc;
use std::thread::JoinHandle;

use super::key_classifier::{classify_macos_keycode, RepeatFilter};
use super::{CategorySink, KeyCategory, MonitorError, PermissionStatus};

type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFStringRef = *const c_void;
type CFAllocatorRef = *const c_void;
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;

type CGEventTapCallBack = extern "C" fn(
    proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

const K_CG_SESSION_EVENT_TAP: u32 = 1;
const K_CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const K_CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;

const K_CG_EVENT_KEY_DOWN: u32 = 10;
const K_CG_EVENT_KEY_UP: u32 = 11;
const K_CG_EVENT_FLAGS_CHANGED: u32 = 12;
const K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;

const K_CG_KEYBOARD_EVENT_AUTOREPEAT: u32 = 8;
const K_CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;

// Shift, Control, Option, Command, CapsLock, Fn.
const MODIFIER_FLAGS_MASK: u64 = 0x0002_0000 | 0x0004_0000 | 0x0008_0000 | 0x0010_0000 | 0x0001_0000 | 0x0080_0000;

const K_CF_RUN_LOOP_RUN_STOPPED: i32 = 2;
const K_CF_RUN_LOOP_RUN_FINISHED: i32 = 1;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
    fn CGEventGetFlags(event: CGEventRef) -> u64;
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    static kCFRunLoopDefaultMode: CFStringRef;
    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: isize,
    ) -> CFRunLoopSourceRef;
    fn CFMachPortInvalidate(port: CFMachPortRef);
    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRunInMode(mode: CFStringRef, seconds: f64, return_after_source_handled: bool) -> i32;
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRetain(cf: *const c_void) -> *const c_void;
    fn CFRelease(cf: *const c_void);
}

struct TapContext {
    sink: CategorySink,
    tap: CFMachPortRef,
    prev_flags: u64,
    repeat: RepeatFilter,
}

struct RunLoopHandle(CFRunLoopRef);
// CFRunLoopStop is documented as thread-safe; the handle is retained.
unsafe impl Send for RunLoopHandle {}

pub struct Tap {
    run_loop: RunLoopHandle,
    thread: Option<JoinHandle<()>>,
}

impl Tap {
    pub fn start(sink: CategorySink) -> Result<Self, MonitorError> {
        if !unsafe { CGPreflightListenEventAccess() } {
            return Err(MonitorError::PermissionDenied);
        }
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<RunLoopHandle, MonitorError>>(1);
        let thread = std::thread::Builder::new()
            .name("keyclick-event-tap".into())
            .spawn(move || unsafe { run_tap_thread(sink, ready_tx) })
            .map_err(|e| MonitorError::Platform(format!("Could not start keyboard thread: {e}")))?;

        match ready_rx.recv() {
            Ok(Ok(run_loop)) => Ok(Self { run_loop, thread: Some(thread) }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(MonitorError::Platform("Keyboard thread exited unexpectedly.".into())),
        }
    }
}

impl Drop for Tap {
    fn drop(&mut self) {
        unsafe { CFRunLoopStop(self.run_loop.0) };
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        unsafe { CFRelease(self.run_loop.0) };
    }
}

unsafe fn run_tap_thread(
    sink: CategorySink,
    ready: mpsc::SyncSender<Result<RunLoopHandle, MonitorError>>,
) {
    let ctx = Box::into_raw(Box::new(TapContext {
        sink,
        tap: std::ptr::null_mut(),
        prev_flags: 0,
        repeat: RepeatFilter::default(),
    }));
    let mask: u64 =
        (1 << K_CG_EVENT_KEY_DOWN) | (1 << K_CG_EVENT_KEY_UP) | (1 << K_CG_EVENT_FLAGS_CHANGED);

    let tap = CGEventTapCreate(
        K_CG_SESSION_EVENT_TAP,
        K_CG_HEAD_INSERT_EVENT_TAP,
        K_CG_EVENT_TAP_OPTION_LISTEN_ONLY,
        mask,
        tap_callback,
        ctx as *mut c_void,
    );
    if tap.is_null() {
        drop(Box::from_raw(ctx));
        // Preflight passed but creation failed: typically the permission was
        // just granted and the process must be restarted to pick it up.
        let _ = ready.send(Err(MonitorError::PermissionDenied));
        return;
    }
    (*ctx).tap = tap;

    let source = CFMachPortCreateRunLoopSource(std::ptr::null(), tap, 0);
    if source.is_null() {
        CFMachPortInvalidate(tap);
        CFRelease(tap);
        drop(Box::from_raw(ctx));
        let _ = ready.send(Err(MonitorError::Platform("Could not create run loop source.".into())));
        return;
    }

    let run_loop = CFRunLoopGetCurrent();
    CFRetain(run_loop);
    CFRunLoopAddSource(run_loop, source, kCFRunLoopDefaultMode);
    CGEventTapEnable(tap, true);
    let _ = ready.send(Ok(RunLoopHandle(run_loop)));

    // Event-driven: the run loop sleeps until the tap delivers an event.
    // A stop issued before the loop starts is remembered by CFRunLoop.
    loop {
        let result = CFRunLoopRunInMode(kCFRunLoopDefaultMode, 1.0e10, false);
        if result == K_CF_RUN_LOOP_RUN_STOPPED || result == K_CF_RUN_LOOP_RUN_FINISHED {
            break;
        }
    }

    CGEventTapEnable(tap, false);
    CFMachPortInvalidate(tap);
    CFRelease(source);
    CFRelease(tap);
    drop(Box::from_raw(ctx));
}

extern "C" fn tap_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    if user_info.is_null() {
        return event;
    }
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| unsafe {
        let ctx = &mut *(user_info as *mut TapContext);
        match event_type {
            K_CG_EVENT_TAP_DISABLED_BY_TIMEOUT | K_CG_EVENT_TAP_DISABLED_BY_USER_INPUT => {
                if !ctx.tap.is_null() {
                    CGEventTapEnable(ctx.tap, true);
                }
            }
            K_CG_EVENT_KEY_DOWN => {
                if CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_AUTOREPEAT) == 0 {
                    let code = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
                    if ctx.repeat.press(code as u8) {
                        ctx.sink.emit(classify_macos_keycode(code));
                    }
                }
            }
            K_CG_EVENT_KEY_UP => {
                let code = CGEventGetIntegerValueField(event, K_CG_KEYBOARD_EVENT_KEYCODE) as u16;
                if ctx.repeat.release(code as u8) {
                    ctx.sink.emit_release(classify_macos_keycode(code));
                }
            }
            K_CG_EVENT_FLAGS_CHANGED => {
                let flags = CGEventGetFlags(event) & MODIFIER_FLAGS_MASK;
                let newly_pressed = flags & !ctx.prev_flags != 0;
                let newly_released = ctx.prev_flags & !flags != 0;
                ctx.prev_flags = flags;
                if newly_pressed {
                    ctx.sink.emit(KeyCategory::Modifier);
                } else if newly_released {
                    ctx.sink.emit_release(KeyCategory::Modifier);
                }
            }
            _ => {}
        }
    }));
    // Listen-only taps cannot alter events; return it untouched regardless.
    event
}

pub fn permission_status() -> PermissionStatus {
    if unsafe { CGPreflightListenEventAccess() } {
        PermissionStatus::Granted
    } else {
        PermissionStatus::Denied
    }
}

pub fn request_permission() -> PermissionStatus {
    if unsafe { CGRequestListenEventAccess() } {
        PermissionStatus::Granted
    } else {
        PermissionStatus::Denied
    }
}

pub fn open_permission_settings() -> Result<(), String> {
    std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent")
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open System Settings: {e}"))
}
