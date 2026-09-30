//! Windows keyboard detection via a low-level keyboard hook (`WH_KEYBOARD_LL`).
//!
//! The hook is observe-only: the callback always forwards the event with
//! `CallNextHookEx` and never returns a non-zero "swallow" value, so every
//! application keeps receiving exactly the same input. The callback does the
//! minimum work possible (classify + non-blocking channel send) so Windows'
//! low-level hook timeout is never approached.

use std::cell::RefCell;
use std::ptr::{null, null_mut};
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{GetLastError, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
    PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
    WM_USER,
};

use super::key_classifier::{classify_windows_vk, RepeatFilter};
use super::{CategorySink, MonitorError};

struct HookState {
    sink: CategorySink,
    repeat: RepeatFilter,
}

thread_local! {
    // Low-level hooks are invoked on the thread that installed them, so this
    // state is only ever touched by the hook thread.
    static STATE: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

pub struct Hook {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Hook {
    pub fn start(sink: CategorySink) -> Result<Self, MonitorError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<u32, u32>>(1);

        let thread = std::thread::Builder::new()
            .name("keyclick-keyboard-hook".into())
            .spawn(move || unsafe { run_hook_thread(sink, ready_tx) })
            .map_err(|e| MonitorError::Platform(format!("Could not start keyboard thread: {e}")))?;

        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Self { thread_id, thread: Some(thread) }),
            Ok(Err(code)) => {
                let _ = thread.join();
                Err(MonitorError::Platform(format!(
                    "Could not install keyboard hook (error {code})."
                )))
            }
            Err(_) => Err(MonitorError::Platform("Keyboard thread exited unexpectedly.".into())),
        }
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

unsafe fn run_hook_thread(sink: CategorySink, ready: mpsc::SyncSender<Result<u32, u32>>) {
    STATE.with(|s| {
        *s.borrow_mut() = Some(HookState { sink, repeat: RepeatFilter::default() });
    });

    // Force creation of this thread's message queue before reporting ready,
    // so a WM_QUIT posted by `Drop` can never be lost.
    let mut msg: MSG = std::mem::zeroed();
    PeekMessageW(&mut msg, null_mut(), WM_USER, WM_USER, PM_NOREMOVE);

    let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(null()), 0);
    if hook.is_null() {
        let _ = ready.send(Err(GetLastError()));
        STATE.with(|s| s.borrow_mut().take());
        return;
    }
    let _ = ready.send(Ok(GetCurrentThreadId()));

    // Event-driven: GetMessageW sleeps until the OS has something for us.
    while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    UnhookWindowsHookEx(hook);
    STATE.with(|s| s.borrow_mut().take());
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam != 0 {
        let vk = (*(lparam as *const KBDLLHOOKSTRUCT)).vkCode;
        let message = wparam as u32;
        // A panic must never cross the FFI boundary or affect input delivery.
        let _ = std::panic::catch_unwind(|| handle_event(vk, message));
    }
    // Always pass the event on, unchanged.
    CallNextHookEx(null_mut(), code, wparam, lparam)
}

#[inline]
fn handle_event(vk: u32, message: u32) {
    STATE.with(|s| {
        let Ok(mut guard) = s.try_borrow_mut() else { return };
        let Some(state) = guard.as_mut() else { return };
        let code = (vk & 0xFF) as u8;
        match message {
            WM_KEYDOWN | WM_SYSKEYDOWN => {
                if state.repeat.press(code) {
                    state.sink.emit(classify_windows_vk(vk));
                }
            }
            WM_KEYUP | WM_SYSKEYUP => {
                if state.repeat.release(code) {
                    state.sink.emit_release(classify_windows_vk(vk));
                }
            }
            _ => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_F24,
    };

    use super::*;
    use crate::keyboard::{KeyCategory, KeyEvent};

    fn send(keys: &[(u16, bool)]) -> u32 {
        let inputs: Vec<INPUT> = keys
            .iter()
            .map(|&(vk, up)| INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: vk,
                        wScan: 0,
                        dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                        time: 0,
                        dwExtraInfo: 0,
                    },
                },
            })
            .collect();
        unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) }
    }

    // F24 exists on no physical keyboard and no app reacts to it.
    #[test]
    #[ignore = "installs a real global hook and injects F24 key presses"]
    fn hook_delivers_categories_and_filters_repeat() {
        let (tx, rx) = crossbeam_channel::bounded(16);
        let hook = Hook::start(CategorySink::new(tx)).expect("hook installs");

        // press, auto-repeat, release, press again, release
        let seq = [(VK_F24, false), (VK_F24, false), (VK_F24, true), (VK_F24, false), (VK_F24, true)];
        assert_eq!(send(&seq), seq.len() as u32);
        let wait = Duration::from_secs(2);
        assert_eq!(rx.recv_timeout(wait), Ok(KeyEvent::press(KeyCategory::Function)));
        assert_eq!(rx.recv_timeout(wait), Ok(KeyEvent::release(KeyCategory::Function)));
        assert_eq!(rx.recv_timeout(wait), Ok(KeyEvent::press(KeyCategory::Function)));
        assert_eq!(rx.recv_timeout(wait), Ok(KeyEvent::release(KeyCategory::Function)));
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "auto-repeat suppressed");

        drop(hook);
        send(&[(VK_F24, false), (VK_F24, true)]);
        assert!(rx.recv_timeout(Duration::from_millis(300)).is_err(), "nothing after unhook");
    }
}
