//! Tauri commands exposed to the settings UI. None of them accept or return
//! keyboard content — only preferences and status.

use serde::Serialize;
use tauri::{AppHandle, Manager, State, WebviewWindow};

use crate::audio::{AudioStatus, SoundPackInfo};
use crate::keyboard::{self, KeyCategory, MonitorError, PermissionStatus};
use crate::settings::{Settings, SettingsPatch};
use crate::{lock, AppState};

#[derive(Debug, Clone, Serialize)]
pub struct KeyboardStatus {
    pub active: bool,
    pub permission: PermissionStatus,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Snapshot {
    pub settings: Settings,
    pub packs: Vec<SoundPackInfo>,
    pub audio: AudioStatus,
    pub keyboard: KeyboardStatus,
    pub platform: &'static str,
    pub version: String,
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(windows) {
        "windows"
    } else {
        "other"
    }
}

fn snapshot(app: &AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    let keyboard = {
        let kb = lock(&state.keyboard);
        let permission = match (&kb.error, keyboard::permission_status()) {
            (Some(MonitorError::PermissionDenied), _) => PermissionStatus::Denied,
            (_, p) => p,
        };
        KeyboardStatus {
            active: kb.monitor.is_some(),
            permission,
            error: kb.error.as_ref().map(ToString::to_string),
        }
    };
    Snapshot {
        settings: state.settings(),
        packs: state.packs.iter().map(|p| p.info()).collect(),
        audio: state.audio.status(),
        keyboard,
        platform: platform(),
        version: app.package_info().version.to_string(),
    }
}

#[tauri::command]
pub fn get_state(app: AppHandle) -> Snapshot {
    snapshot(&app)
}

#[tauri::command]
pub fn update_settings(app: AppHandle, patch: SettingsPatch) -> Result<Snapshot, String> {
    crate::apply_patch(&app, patch)?;
    Ok(snapshot(&app))
}

/// Plays a sound from the active pack regardless of the enabled switches.
#[tauri::command]
pub fn preview_sound(state: State<'_, AppState>, category: KeyCategory) {
    state.audio.preview(category);
}

/// Shows the OS permission prompt (macOS) and retries starting the listener.
#[tauri::command]
pub fn request_keyboard_permission(app: AppHandle) -> Snapshot {
    keyboard::request_permission();
    retry_keyboard(app)
}

#[tauri::command]
pub fn retry_keyboard(app: AppHandle) -> Snapshot {
    let state = app.state::<AppState>();
    state.sync_keyboard(state.settings().enabled);
    crate::tray::refresh(&app);
    snapshot(&app)
}

#[tauri::command]
pub fn open_permission_settings() -> Result<(), String> {
    keyboard::open_permission_settings()
}

/// macOS only applies a newly granted Input Monitoring permission after a relaunch.
#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

#[tauri::command]
pub fn hide_window(window: WebviewWindow) {
    let _ = window.hide();
}
