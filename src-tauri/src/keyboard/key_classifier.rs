//! Platform-neutral key categories.
//!
//! Every native keyboard event is converted into a [`KeyCategory`] immediately
//! inside the OS callback. The native key code is never stored, logged or
//! forwarded: only the category leaves this module.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyCategory {
    Normal,
    Space,
    Enter,
    Backspace,
    Tab,
    Escape,
    Modifier,
    Arrow,
    Function,
    Other,
}

impl KeyCategory {
    pub const ALL: [KeyCategory; 10] = [
        KeyCategory::Normal,
        KeyCategory::Space,
        KeyCategory::Enter,
        KeyCategory::Backspace,
        KeyCategory::Tab,
        KeyCategory::Escape,
        KeyCategory::Modifier,
        KeyCategory::Arrow,
        KeyCategory::Function,
        KeyCategory::Other,
    ];

    /// Label used for (debug) logging. Never log anything more specific than this.
    pub fn as_str(self) -> &'static str {
        match self {
            KeyCategory::Normal => "NORMAL",
            KeyCategory::Space => "SPACE",
            KeyCategory::Enter => "ENTER",
            KeyCategory::Backspace => "BACKSPACE",
            KeyCategory::Tab => "TAB",
            KeyCategory::Escape => "ESC",
            KeyCategory::Modifier => "MODIFIER",
            KeyCategory::Arrow => "ARROW",
            KeyCategory::Function => "FUNCTION",
            KeyCategory::Other => "OTHER",
        }
    }
}

/// Classify a Windows virtual-key code (`KBDLLHOOKSTRUCT::vkCode`).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn classify_windows_vk(vk: u32) -> KeyCategory {
    match vk {
        0x08 => KeyCategory::Backspace,                 // VK_BACK
        0x2E => KeyCategory::Backspace,                 // VK_DELETE
        0x09 => KeyCategory::Tab,                       // VK_TAB
        0x0D => KeyCategory::Enter,                     // VK_RETURN (incl. numpad enter)
        0x1B => KeyCategory::Escape,                    // VK_ESCAPE
        0x20 => KeyCategory::Space,                     // VK_SPACE
        0x25..=0x28 => KeyCategory::Arrow,              // VK_LEFT..VK_DOWN
        0x10..=0x12 | 0x14 => KeyCategory::Modifier,    // SHIFT, CONTROL, MENU, CAPITAL
        0x5B | 0x5C => KeyCategory::Modifier,           // LWIN, RWIN
        0xA0..=0xA5 => KeyCategory::Modifier,           // L/R SHIFT, CONTROL, MENU
        0x70..=0x87 => KeyCategory::Function,           // F1..F24
        0x30..=0x39 | 0x41..=0x5A => KeyCategory::Normal, // 0-9, A-Z
        0x60..=0x6F => KeyCategory::Normal,             // numpad digits and operators
        0xBA..=0xC0 | 0xDB..=0xDF | 0xE2 => KeyCategory::Normal, // OEM punctuation
        0xE5 => KeyCategory::Normal,                    // VK_PROCESSKEY (IME composition)
        _ => KeyCategory::Other,
    }
}

/// Classify a macOS virtual key code (`kCGKeyboardEventKeycode`, `kVK_*`).
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn classify_macos_keycode(code: u16) -> KeyCategory {
    match code {
        0x24 | 0x34 | 0x4C => KeyCategory::Enter, // Return, (legacy) Enter, Keypad Enter
        0x30 => KeyCategory::Tab,
        0x31 => KeyCategory::Space,
        0x33 | 0x75 => KeyCategory::Backspace, // Delete, Forward Delete
        0x35 => KeyCategory::Escape,
        0x36..=0x3F => KeyCategory::Modifier, // Command, Shift, CapsLock, Option, Control, Fn
        0x7B..=0x7E => KeyCategory::Arrow,
        0x00..=0x32 => KeyCategory::Normal, // ANSI letters, digits, punctuation
        0x41 | 0x43 | 0x45 | 0x47 | 0x4B | 0x4E | 0x51..=0x59 | 0x5B | 0x5C => KeyCategory::Normal, // keypad
        0x5D..=0x5F => KeyCategory::Normal, // JIS Yen, Underscore, Keypad Comma
        0x40 | 0x4F | 0x50 | 0x5A | 0x60..=0x65 | 0x67 | 0x69..=0x6B | 0x6D | 0x6F | 0x71 | 0x76
        | 0x78 | 0x7A => KeyCategory::Function, // F1..F20
        _ => KeyCategory::Other,
    }
}

/// Suppresses OS auto-repeat so that holding a key produces a single sound.
///
/// Only a transient "is currently held" bit per key code is kept; it is cleared
/// on key-up and never records order or history.
#[derive(Default)]
pub struct RepeatFilter {
    down: [u64; 4],
}

impl RepeatFilter {
    /// Returns `true` if this key-down is a fresh press (not an auto-repeat).
    pub fn press(&mut self, code: u8) -> bool {
        let (word, bit) = ((code >> 6) as usize, 1u64 << (code & 63));
        let fresh = self.down[word] & bit == 0;
        self.down[word] |= bit;
        fresh
    }

    /// Returns `true` if the key was held (i.e. this is a real release).
    pub fn release(&mut self, code: u8) -> bool {
        let (word, bit) = ((code >> 6) as usize, 1u64 << (code & 63));
        let was_down = self.down[word] & bit != 0;
        self.down[word] &= !bit;
        was_down
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_letters_digits_punctuation_are_normal() {
        for vk in (0x41..=0x5A).chain(0x30..=0x39) {
            assert_eq!(classify_windows_vk(vk), KeyCategory::Normal, "vk {vk:#x}");
        }
        assert_eq!(classify_windows_vk(0xBC), KeyCategory::Normal); // comma
        assert_eq!(classify_windows_vk(0xBE), KeyCategory::Normal); // period
        assert_eq!(classify_windows_vk(0x61), KeyCategory::Normal); // numpad 1
    }

    #[test]
    fn windows_special_keys() {
        assert_eq!(classify_windows_vk(0x20), KeyCategory::Space);
        assert_eq!(classify_windows_vk(0x0D), KeyCategory::Enter);
        assert_eq!(classify_windows_vk(0x08), KeyCategory::Backspace);
        assert_eq!(classify_windows_vk(0x2E), KeyCategory::Backspace);
        assert_eq!(classify_windows_vk(0x09), KeyCategory::Tab);
        assert_eq!(classify_windows_vk(0x1B), KeyCategory::Escape);
        for vk in [0x10, 0x11, 0x12, 0x14, 0x5B, 0x5C, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5] {
            assert_eq!(classify_windows_vk(vk), KeyCategory::Modifier, "vk {vk:#x}");
        }
        for vk in 0x25..=0x28 {
            assert_eq!(classify_windows_vk(vk), KeyCategory::Arrow);
        }
        assert_eq!(classify_windows_vk(0x70), KeyCategory::Function);
        assert_eq!(classify_windows_vk(0x87), KeyCategory::Function);
        assert_eq!(classify_windows_vk(0xAD), KeyCategory::Other); // volume mute
        assert_eq!(classify_windows_vk(0x24), KeyCategory::Other); // home
    }

    #[test]
    fn macos_keys() {
        assert_eq!(classify_macos_keycode(0x00), KeyCategory::Normal); // A
        assert_eq!(classify_macos_keycode(0x12), KeyCategory::Normal); // 1
        assert_eq!(classify_macos_keycode(0x2B), KeyCategory::Normal); // comma
        assert_eq!(classify_macos_keycode(0x31), KeyCategory::Space);
        assert_eq!(classify_macos_keycode(0x24), KeyCategory::Enter);
        assert_eq!(classify_macos_keycode(0x4C), KeyCategory::Enter);
        assert_eq!(classify_macos_keycode(0x33), KeyCategory::Backspace);
        assert_eq!(classify_macos_keycode(0x30), KeyCategory::Tab);
        assert_eq!(classify_macos_keycode(0x35), KeyCategory::Escape);
        assert_eq!(classify_macos_keycode(0x37), KeyCategory::Modifier);
        assert_eq!(classify_macos_keycode(0x38), KeyCategory::Modifier);
        assert_eq!(classify_macos_keycode(0x7B), KeyCategory::Arrow);
        assert_eq!(classify_macos_keycode(0x7A), KeyCategory::Function); // F1
        assert_eq!(classify_macos_keycode(0x6F), KeyCategory::Function); // F12
        assert_eq!(classify_macos_keycode(0x48), KeyCategory::Other); // volume up
        assert_eq!(classify_macos_keycode(0x52), KeyCategory::Normal); // keypad 0
    }

    #[test]
    fn category_labels_are_generic() {
        for c in KeyCategory::ALL {
            let label = c.as_str();
            assert!(label.len() > 1 && label.chars().all(|ch| ch.is_ascii_uppercase()));
        }
    }

    #[test]
    fn repeat_filter_suppresses_auto_repeat() {
        let mut f = RepeatFilter::default();
        assert!(f.press(0x41));
        assert!(!f.press(0x41));
        assert!(!f.press(0x41));
        assert!(f.press(0x42), "other keys are independent");
        assert!(f.release(0x41));
        assert!(!f.release(0x41), "release without press is ignored");
        assert!(f.press(0x41));
        assert!(f.press(0xFF));
        assert!(!f.press(0xFF));
    }
}
