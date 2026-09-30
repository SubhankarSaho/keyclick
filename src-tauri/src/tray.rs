//! System tray (Windows) / menu bar extra (macOS).

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Wry};

use crate::settings::SettingsPatch;
use crate::{apply_patch, show_main_window, AppState, NAVIGATE_EVENT};

const TRAY_ID: &str = "keyclick";
const VOLUME_STEPS: [u8; 6] = [0, 20, 40, 60, 80, 100];

#[cfg(target_os = "macos")]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray-template.png");
#[cfg(not(target_os = "macos"))]
const TRAY_ICON: &[u8] = include_bytes!("../icons/tray.png");

pub struct TrayItems {
    enabled: CheckMenuItem<Wry>,
    pause: MenuItem<Wry>,
    packs: Vec<(String, CheckMenuItem<Wry>)>,
    volumes: Vec<(u8, CheckMenuItem<Wry>)>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    let settings = state.settings();

    let title = MenuItem::with_id(app, "title", "KeyClick", false, None::<&str>)?;
    let enabled = CheckMenuItem::with_id(app, "enabled", "Enabled", true, settings.enabled, None::<&str>)?;

    let packs: Vec<(String, CheckMenuItem<Wry>)> = state
        .packs
        .iter()
        .map(|p| {
            let id = p.manifest.id.clone();
            let item = CheckMenuItem::with_id(
                app,
                format!("pack:{id}"),
                &p.manifest.name,
                true,
                id == settings.sound_pack,
                None::<&str>,
            )?;
            Ok((id, item))
        })
        .collect::<tauri::Result<_>>()?;
    let pack_menu = Submenu::with_id(app, "packs", "Sound Pack", true)?;
    for (_, item) in &packs {
        pack_menu.append(item)?;
    }

    let current_volume = volume_step(settings.volume);
    let volumes: Vec<(u8, CheckMenuItem<Wry>)> = VOLUME_STEPS
        .iter()
        .map(|v| {
            let item = CheckMenuItem::with_id(
                app,
                format!("vol:{v}"),
                format!("{v}%"),
                true,
                *v == current_volume,
                None::<&str>,
            )?;
            Ok((*v, item))
        })
        .collect::<tauri::Result<_>>()?;
    let volume_menu = Submenu::with_id(app, "volume", "Volume", true)?;
    for (_, item) in &volumes {
        volume_menu.append(item)?;
    }

    let open = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", pause_label(settings.enabled), true, None::<&str>)?;
    let quit_label = if cfg!(target_os = "macos") { "Quit KeyClick" } else { "Quit" };
    let quit = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &title,
            &PredefinedMenuItem::separator(app)?,
            &enabled,
            &pack_menu,
            &volume_menu,
            &PredefinedMenuItem::separator(app)?,
            &open,
            &pause,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(Image::from_bytes(TRAY_ICON)?)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(tooltip(settings.enabled))
        .menu(&menu)
        // macOS menu-bar convention: click opens the menu. Windows: left click opens settings.
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } =
                event
            {
                if !cfg!(target_os = "macos") {
                    show_main_window(tray.app_handle());
                }
            }
        })
        .build(app)?;

    app.manage(TrayItems { enabled, pause, packs, volumes });
    Ok(())
}

/// Re-syncs check marks, labels and tooltip with the current settings.
pub fn refresh(app: &AppHandle) {
    let (Some(state), Some(items)) = (app.try_state::<AppState>(), app.try_state::<TrayItems>()) else {
        return;
    };
    let s = state.settings();
    let _ = items.enabled.set_checked(s.enabled);
    let _ = items.pause.set_text(pause_label(s.enabled));
    for (id, item) in &items.packs {
        let _ = item.set_checked(*id == s.sound_pack);
    }
    let step = volume_step(s.volume);
    for (v, item) in &items.volumes {
        let _ = item.set_checked(*v == step);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip(s.enabled)));
    }
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let id = event.id().as_ref();
    let patch = match id {
        "enabled" | "pause" => {
            let enabled = app.state::<AppState>().settings().enabled;
            Some(SettingsPatch { enabled: Some(!enabled), ..Default::default() })
        }
        "settings" => {
            show_main_window(app);
            let _ = app.emit(NAVIGATE_EVENT, "settings");
            None
        }
        "quit" => {
            app.exit(0);
            None
        }
        _ => {
            if let Some(pack) = id.strip_prefix("pack:") {
                Some(SettingsPatch { sound_pack: Some(pack.to_string()), ..Default::default() })
            } else {
                id.strip_prefix("vol:").and_then(|v| v.parse::<u8>().ok()).map(|v| SettingsPatch {
                    volume: Some(f32::from(v) / 100.0),
                    ..Default::default()
                })
            }
        }
    };
    if let Some(patch) = patch {
        if apply_patch(app, patch).is_err() {
            // The UI shows details; keep check marks truthful.
            refresh(app);
        }
    }
}

fn pause_label(enabled: bool) -> &'static str {
    if enabled {
        "Pause"
    } else {
        "Resume"
    }
}

fn tooltip(enabled: bool) -> &'static str {
    if enabled {
        "KeyClick — On"
    } else {
        "KeyClick — Paused"
    }
}

/// Nearest tray preset for a 0.0..=1.0 volume.
fn volume_step(volume: f32) -> u8 {
    let pct = (crate::settings::clamp_volume(volume) * 100.0).round() as i32;
    *VOLUME_STEPS.iter().min_by_key(|v| (i32::from(**v) - pct).abs()).unwrap_or(&40)
}

#[cfg(test)]
mod tests {
    use super::volume_step;

    #[test]
    fn volume_maps_to_nearest_preset() {
        assert_eq!(volume_step(0.4), 40);
        assert_eq!(volume_step(0.0), 0);
        assert_eq!(volume_step(1.0), 100);
        assert_eq!(volume_step(0.47), 40);
        assert_eq!(volume_step(0.53), 60);
        assert_eq!(volume_step(f32::NAN), 40);
    }
}
