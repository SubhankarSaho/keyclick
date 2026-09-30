//! Global keyboard event detection.
//!
//! Privacy contract: platform hooks observe key-down events in listen-only
//! mode, convert them to a [`KeyCategory`] inside the OS callback and send only
//! that category over a bounded channel. Events are never modified, blocked,
//! injected, stored or logged.

pub mod key_classifier;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

use crossbeam_channel::Sender;
use serde::Serialize;

pub use key_classifier::KeyCategory;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)] // variants are platform-specific
pub enum PermissionStatus {
    Granted,
    Denied,
    NotRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)] // variants are platform-specific
pub enum MonitorError {
    PermissionDenied,
    Unsupported,
    Platform(String),
}

impl std::fmt::Display for MonitorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MonitorError::PermissionDenied => f.write_str("Keyboard access permission is required."),
            MonitorError::Unsupported => f.write_str("Keyboard detection is not supported on this platform."),
            MonitorError::Platform(msg) => f.write_str(msg),
        }
    }
}

/// What the audio side receives: a generic category and whether the key went
/// down or came back up. No key identity, text or timing history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEvent {
    pub category: KeyCategory,
    pub release: bool,
}

impl KeyEvent {
    pub const fn press(category: KeyCategory) -> Self {
        Self { category, release: false }
    }

    pub const fn release(category: KeyCategory) -> Self {
        Self { category, release: true }
    }
}

/// Sends events without ever blocking the OS input callback.
/// If the audio side is slow or gone, the event is simply dropped.
#[derive(Clone)]
pub struct CategorySink(Sender<KeyEvent>);

impl CategorySink {
    pub fn new(tx: Sender<KeyEvent>) -> Self {
        Self(tx)
    }

    #[inline]
    pub fn emit(&self, category: KeyCategory) {
        let _ = self.0.try_send(KeyEvent::press(category));
    }

    #[inline]
    pub fn emit_release(&self, category: KeyCategory) {
        let _ = self.0.try_send(KeyEvent::release(category));
    }
}

/// A running keyboard listener. Dropping it removes the OS hook.
pub struct KeyboardMonitor {
    #[cfg(windows)]
    _inner: windows::Hook,
    #[cfg(target_os = "macos")]
    _inner: macos::Tap,
}

impl KeyboardMonitor {
    #[allow(unused_variables)]
    pub fn start(sink: CategorySink) -> Result<Self, MonitorError> {
        #[cfg(windows)]
        {
            windows::Hook::start(sink).map(|h| Self { _inner: h })
        }
        #[cfg(target_os = "macos")]
        {
            macos::Tap::start(sink).map(|t| Self { _inner: t })
        }
        #[cfg(not(any(windows, target_os = "macos")))]
        {
            Err(MonitorError::Unsupported)
        }
    }
}

pub fn permission_status() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::permission_status()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Asks the OS to show its permission prompt (macOS) and returns the current status.
pub fn request_permission() -> PermissionStatus {
    #[cfg(target_os = "macos")]
    {
        macos::request_permission()
    }
    #[cfg(not(target_os = "macos"))]
    {
        PermissionStatus::NotRequired
    }
}

/// Opens the OS privacy settings page where keyboard monitoring is granted.
pub fn open_permission_settings() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        macos::open_permission_settings()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sink_never_blocks_when_full_or_disconnected() {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let sink = CategorySink::new(tx);
        sink.emit(KeyCategory::Normal);
        sink.emit(KeyCategory::Space); // full: dropped, no block
        assert_eq!(rx.try_recv(), Ok(KeyEvent::press(KeyCategory::Normal)));
        assert!(rx.try_recv().is_err());
        drop(rx);
        sink.emit(KeyCategory::Enter); // disconnected: ignored, no panic
        sink.emit_release(KeyCategory::Enter);
    }

    /// Privacy guard: the code that sees raw key codes must never print, log,
    /// persist or transmit anything.
    #[test]
    fn keyboard_sources_have_no_output_paths() {
        let sources = [
            include_str!("mod.rs"),
            include_str!("key_classifier.rs"),
            include_str!("windows.rs"),
            include_str!("macos.rs"),
        ];
        let forbidden = [
            concat!("print", "ln!"),
            concat!("eprint", "ln!"),
            concat!("dbg", "!("),
            concat!("log", "::"),
            concat!("std::fs", "::"),
            concat!("std::net", "::"),
            concat!("File", "::create"),
        ];
        for src in sources {
            for f in forbidden {
                assert!(!src.contains(f), "keyboard module must not use `{f}`");
            }
        }
    }
}
