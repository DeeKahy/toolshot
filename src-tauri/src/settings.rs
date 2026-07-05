use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Settings {
    pub capture_shortcut: Option<String>,
    pub picker_shortcut: Option<String>,
}

pub struct SettingsState(pub Mutex<Settings>);

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    settings_path(app)
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn register_shortcut(app: &AppHandle, kind: &str, accel: &str) -> Result<(), String> {
    let kind = kind.to_string();
    app.global_shortcut()
        .on_shortcut(accel, move |app, _shortcut, event| {
            if event.state() == ShortcutState::Pressed {
                match kind.as_str() {
                    "capture" => crate::capture::start_window_pick(app),
                    "picker" => crate::capture::start_color_pick(app),
                    _ => {}
                }
            }
        })
        .map_err(|e| e.to_string())
}

// Called once at startup: load persisted settings and arm the shortcuts.
pub fn init(app: &AppHandle) {
    let settings = load_settings(app);
    if let Some(accel) = &settings.capture_shortcut {
        if let Err(e) = register_shortcut(app, "capture", accel) {
            eprintln!("could not register capture shortcut {accel}: {e}");
        }
    }
    if let Some(accel) = &settings.picker_shortcut {
        if let Err(e) = register_shortcut(app, "picker", accel) {
            eprintln!("could not register picker shortcut {accel}: {e}");
        }
    }
    app.manage(SettingsState(Mutex::new(settings)));
}

pub fn open_settings_window(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window("settings") {
        let _ = existing.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Toolshot Settings")
        .inner_size(430.0, 320.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(true)
        .build();
    match result {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("failed to open settings: {e}"),
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Settings {
    state.0.lock().unwrap().clone()
}

// kind is "capture" or "picker"; accel of None clears the binding.
#[tauri::command]
pub fn set_shortcut(
    app: AppHandle,
    state: State<'_, SettingsState>,
    kind: String,
    accel: Option<String>,
) -> Result<(), String> {
    let old = {
        let settings = state.0.lock().unwrap();
        match kind.as_str() {
            "capture" => settings.capture_shortcut.clone(),
            "picker" => settings.picker_shortcut.clone(),
            _ => return Err("unknown shortcut kind".to_string()),
        }
    };

    if let Some(old) = old {
        let _ = app.global_shortcut().unregister(old.as_str());
    }

    // Register first so an invalid or taken combo never gets persisted.
    if let Some(accel) = &accel {
        register_shortcut(&app, &kind, accel)?;
    }

    let mut settings = state.0.lock().unwrap();
    match kind.as_str() {
        "capture" => settings.capture_shortcut = accel,
        "picker" => settings.picker_shortcut = accel,
        _ => {}
    }
    save_settings(&app, &settings)
}

#[tauri::command]
pub fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn close_settings(app: AppHandle) {
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.close();
    }
}
