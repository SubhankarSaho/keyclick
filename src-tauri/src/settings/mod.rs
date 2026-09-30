//! User settings. Only preferences are stored here — never keyboard content.

pub mod persistence;

use serde::{Deserialize, Serialize};

use crate::keyboard::KeyCategory;

pub const DEFAULT_SOUND_PACK: &str = "mechanical";
pub const DEFAULT_VOLUME: f32 = 0.4;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub enabled: bool,
    /// 0.0 ..= 1.0
    pub volume: f32,
    pub sound_pack: String,
    pub randomize: bool,
    pub space_sound: bool,
    pub enter_sound: bool,
    pub backspace_sound: bool,
    pub modifier_sound: bool,
    pub arrow_sound: bool,
    /// Key-up ("top-out") sound, like a real keyboard makes when a key returns.
    pub release_sound: bool,
    pub start_on_login: bool,
    pub onboarding_complete: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: DEFAULT_VOLUME,
            sound_pack: DEFAULT_SOUND_PACK.to_string(),
            randomize: true,
            space_sound: true,
            enter_sound: true,
            backspace_sound: true,
            modifier_sound: false,
            arrow_sound: true,
            release_sound: true,
            start_on_login: false,
            onboarding_complete: false,
        }
    }
}

impl Settings {
    /// Repairs out-of-range or malformed values (e.g. from a hand-edited file).
    pub fn sanitized(mut self) -> Self {
        self.volume = clamp_volume(self.volume);
        if !is_valid_pack_id(&self.sound_pack) {
            self.sound_pack = DEFAULT_SOUND_PACK.to_string();
        }
        self
    }

    /// Whether a sound should be played for this category at all.
    pub fn category_enabled(&self, category: KeyCategory) -> bool {
        match category {
            KeyCategory::Space => self.space_sound,
            KeyCategory::Enter => self.enter_sound,
            KeyCategory::Backspace => self.backspace_sound,
            KeyCategory::Modifier => self.modifier_sound,
            KeyCategory::Arrow => self.arrow_sound,
            KeyCategory::Normal
            | KeyCategory::Tab
            | KeyCategory::Escape
            | KeyCategory::Function
            | KeyCategory::Other => true,
        }
    }
}

pub fn clamp_volume(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        DEFAULT_VOLUME
    }
}

/// Pack ids double as folder names, so only allow a safe character set.
pub fn is_valid_pack_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// Partial update sent by the UI or tray. `None` fields are left unchanged.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SettingsPatch {
    pub enabled: Option<bool>,
    pub volume: Option<f32>,
    pub sound_pack: Option<String>,
    pub randomize: Option<bool>,
    pub space_sound: Option<bool>,
    pub enter_sound: Option<bool>,
    pub backspace_sound: Option<bool>,
    pub modifier_sound: Option<bool>,
    pub arrow_sound: Option<bool>,
    pub release_sound: Option<bool>,
    pub start_on_login: Option<bool>,
    pub onboarding_complete: Option<bool>,
}

impl SettingsPatch {
    pub fn apply_to(self, base: &Settings) -> Settings {
        let mut s = base.clone();
        macro_rules! set {
            ($($field:ident),*) => { $( if let Some(v) = self.$field { s.$field = v; } )* };
        }
        set!(
            enabled,
            volume,
            sound_pack,
            randomize,
            space_sound,
            enter_sound,
            backspace_sound,
            modifier_sound,
            arrow_sound,
            release_sound,
            start_on_login,
            onboarding_complete
        );
        s.sanitized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_spec() {
        let s = Settings::default();
        assert!(s.enabled);
        assert_eq!(s.volume, 0.4);
        assert_eq!(s.sound_pack, "mechanical");
        assert!(s.randomize && s.space_sound && s.enter_sound && s.backspace_sound);
        assert!(!s.modifier_sound);
        assert!(s.release_sound);
        assert!(!s.start_on_login, "start at login must be opt-in");
        assert!(!s.onboarding_complete);
    }

    #[test]
    fn volume_is_clamped() {
        assert_eq!(clamp_volume(-1.0), 0.0);
        assert_eq!(clamp_volume(2.0), 1.0);
        assert_eq!(clamp_volume(0.55), 0.55);
        assert_eq!(clamp_volume(f32::NAN), DEFAULT_VOLUME);
        assert_eq!(clamp_volume(f32::INFINITY), DEFAULT_VOLUME);
    }

    #[test]
    fn invalid_pack_id_falls_back() {
        let s = Settings { sound_pack: "../../etc".into(), ..Default::default() }.sanitized();
        assert_eq!(s.sound_pack, DEFAULT_SOUND_PACK);
        let s = Settings { sound_pack: "".into(), ..Default::default() }.sanitized();
        assert_eq!(s.sound_pack, DEFAULT_SOUND_PACK);
        let s = Settings { sound_pack: "thock".into(), ..Default::default() }.sanitized();
        assert_eq!(s.sound_pack, "thock");
    }

    #[test]
    fn patch_only_changes_given_fields() {
        let base = Settings::default();
        let patch = SettingsPatch { volume: Some(1.5), enabled: Some(false), ..Default::default() };
        let s = patch.apply_to(&base);
        assert_eq!(s.volume, 1.0);
        assert!(!s.enabled);
        assert_eq!(s.sound_pack, base.sound_pack);
        assert_eq!(s.randomize, base.randomize);
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{ "volume": 0.8 }"#).unwrap();
        assert_eq!(s.volume, 0.8);
        assert!(s.enabled);
        assert_eq!(s.sound_pack, "mechanical");
    }

    #[test]
    fn unknown_fields_are_ignored() {
        let s: Settings = serde_json::from_str(r#"{ "enabled": false, "future_option": 3 }"#).unwrap();
        assert!(!s.enabled);
    }

    #[test]
    fn category_toggles() {
        let mut s = Settings::default();
        assert!(s.category_enabled(KeyCategory::Normal));
        assert!(!s.category_enabled(KeyCategory::Modifier));
        s.space_sound = false;
        s.enter_sound = false;
        s.backspace_sound = false;
        s.arrow_sound = false;
        s.modifier_sound = true;
        assert!(!s.category_enabled(KeyCategory::Space));
        assert!(!s.category_enabled(KeyCategory::Enter));
        assert!(!s.category_enabled(KeyCategory::Backspace));
        assert!(!s.category_enabled(KeyCategory::Arrow));
        assert!(s.category_enabled(KeyCategory::Modifier));
        assert!(s.category_enabled(KeyCategory::Tab));
    }

    #[test]
    fn serialized_settings_contain_no_key_data() {
        let json = serde_json::to_value(Settings::default()).unwrap();
        let keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        for k in &keys {
            assert!(!k.contains("text") && !k.contains("history") && !k.contains("typed"));
        }
    }
}
