//! KeyClick — make typing sound satisfying.
//!
//! Data flow (privacy by construction):
//! OS key event -> `KeyCategory` (inside the OS callback) -> bounded channel ->
//! audio thread plays a preloaded clip -> category is dropped. Nothing about
//! the key is stored, logged or sent anywhere.

mod audio;
mod commands;
mod keyboard;
mod settings;
mod tray;

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use crossbeam_channel::Sender;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use audio::{AudioEngine, LoadedPack, PackEntry};
use keyboard::{CategorySink, KeyEvent, KeyboardMonitor, MonitorError};
use settings::persistence::SettingsStore;
use settings::{Settings, SettingsPatch};

/// Passed by the login item so an automatic start stays in the tray.
pub const AUTOSTART_ARG: &str = "--autostart";
pub const MAIN_WINDOW: &str = "main";
/// Emitted to the UI whenever settings or status change (payload-free).
pub const STATE_EVENT: &str = "state-changed";
pub const NAVIGATE_EVENT: &str = "navigate";
/// Small on purpose: if audio ever falls behind, stale key events are dropped.
const KEY_QUEUE: usize = 64;

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
struct KeyboardState {
    monitor: Option<KeyboardMonitor>,
    error: Option<MonitorError>,
}

pub struct AppState {
    settings: Mutex<Settings>,
    store: SettingsStore,
    audio: AudioEngine,
    packs: Vec<PackEntry>,
    keyboard: Mutex<KeyboardState>,
    key_tx: Sender<KeyEvent>,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Decodes the requested pack (falling back to the first available one)
    /// and hands it to the audio engine. Returns the id actually activated.
    fn activate_pack(&self, id: &str) -> Result<String, String> {
        let entry = self
            .packs
            .iter()
            .find(|p| p.manifest.id == id)
            .or_else(|| self.packs.first())
            .ok_or("No sound packs are installed.")?;
        let pack = LoadedPack::load(entry)?;
        self.audio.set_pack(Arc::new(pack));
        Ok(entry.manifest.id.clone())
    }

    /// Installs the OS keyboard listener while enabled and removes it while
    /// disabled, so a paused KeyClick does not observe the keyboard at all.
    fn sync_keyboard(&self, enabled: bool) {
        let mut kb = lock(&self.keyboard);
        if enabled {
            if kb.monitor.is_none() {
                match KeyboardMonitor::start(CategorySink::new(self.key_tx.clone())) {
                    Ok(m) => {
                        kb.monitor = Some(m);
                        kb.error = None;
                    }
                    Err(e) => kb.error = Some(e),
                }
            }
        } else {
            kb.monitor = None;
            kb.error = None;
        }
    }
}

/// Applies a settings change from the UI or tray, persists it and notifies
/// every surface. Errors are reported but never leave settings inconsistent.
pub fn apply_patch(app: &AppHandle, patch: SettingsPatch) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let mut error = None;
    let new = {
        let mut current = lock(&state.settings);
        let old = current.clone();
        let mut new = patch.apply_to(&old);

        if new.sound_pack != old.sound_pack {
            match state.activate_pack(&new.sound_pack) {
                Ok(id) => new.sound_pack = id,
                Err(e) => {
                    new.sound_pack = old.sound_pack.clone();
                    error = Some(e);
                }
            }
        }
        if new.start_on_login != old.start_on_login {
            let autolaunch = app.autolaunch();
            let result = if new.start_on_login { autolaunch.enable() } else { autolaunch.disable() };
            if let Err(e) = result {
                new.start_on_login = old.start_on_login;
                error = Some(format!("Could not change the login item: {e}"));
            }
        }

        *current = new.clone();
        state.audio.update_settings(new.clone());
        if new.enabled != old.enabled {
            state.sync_keyboard(new.enabled);
        }
        if let Err(e) = state.store.save(&new) {
            error.get_or_insert(format!("Could not save settings: {e}"));
        }
        new
    };
    tray::refresh(app);
    let _ = app.emit(STATE_EVENT, ());
    error.map_or(Ok(new), Err)
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

/// Built-in packs first, then user-installed packs (the custom-pack extension point).
fn pack_roots(app: &AppHandle) -> Vec<(PathBuf, bool)> {
    let mut roots = Vec::new();
    if let Ok(dir) = app.path().resource_dir() {
        roots.push((dir.join("sounds"), true));
    }
    #[cfg(debug_assertions)]
    roots.push((PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sounds"), true));
    if let Ok(dir) = app.path().app_data_dir() {
        roots.push((dir.join("soundpacks"), false));
    }
    roots
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let handle = app.handle().clone();
    let store = SettingsStore::new(&app.path().app_config_dir()?);
    let mut settings = store.load();
    // The OS login-item state is the source of truth (the user may remove it elsewhere).
    if let Ok(enabled) = handle.autolaunch().is_enabled() {
        settings.start_on_login = enabled;
    }

    let (key_tx, key_rx) = crossbeam_channel::bounded(KEY_QUEUE);
    let state = AppState {
        audio: AudioEngine::start(settings.clone(), key_rx),
        packs: audio::soundpack::discover(&pack_roots(&handle)),
        settings: Mutex::new(settings.clone()),
        store,
        keyboard: Mutex::new(KeyboardState::default()),
        key_tx,
    };
    if let Ok(id) = state.activate_pack(&settings.sound_pack) {
        settings.sound_pack = id;
    }
    *lock(&state.settings) = settings.clone();
    state.audio.update_settings(settings.clone());
    state.sync_keyboard(settings.enabled);
    app.manage(state);

    tray::create(&handle)?;

    let autostarted = std::env::args().any(|a| a == AUTOSTART_ARG);
    if !autostarted || !settings.onboarding_complete {
        show_main_window(&handle);
    }
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        // Must be registered first: a second launch just focuses the running app.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .app_name("KeyClick")
                .arg(AUTOSTART_ARG)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::update_settings,
            commands::preview_sound,
            commands::request_keyboard_permission,
            commands::retry_keyboard,
            commands::open_permission_settings,
            commands::restart_app,
            commands::hide_window,
        ])
        .on_window_event(|window, event| {
            // Closing the window only hides it; KeyClick keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(setup)
        .build(tauri::generate_context!())
        .expect("failed to start KeyClick");

    app.run(|_app, event| match event {
        // Only an explicit Quit (which passes an exit code) ends the process.
        RunEvent::ExitRequested { code: None, api, .. } => api.prevent_exit(),
        #[cfg(target_os = "macos")]
        RunEvent::Reopen { .. } => show_main_window(_app),
        _ => {}
    });
}
