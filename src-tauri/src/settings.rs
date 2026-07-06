use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(default)]
pub struct Settings {
    pub capture_shortcut: Option<String>,
    pub fullscreen_shortcut: Option<String>,
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

// Called once at startup. The daemon owns the live shortcuts and reloads
// them when the settings file changes, this process only reads and edits
// the file.
pub fn init(app: &AppHandle) {
    let settings = load_settings(app);
    app.manage(SettingsState(Mutex::new(settings)));
}

pub fn open_settings_window(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window("settings") {
        let _ = existing.set_focus();
        return;
    }
    let result = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Toolshot Settings")
        .inner_size(430.0, 448.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .focused(true)
        .build();
    match result {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => {
            eprintln!("failed to open settings: {e}");
            app.exit(1);
        }
    }
}

#[tauri::command]
pub fn get_settings(state: State<'_, SettingsState>) -> Settings {
    state.0.lock().unwrap().clone()
}

// kind is "capture", "fullscreen" or "picker"; accel of None clears the
// binding.
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
            "fullscreen" => settings.fullscreen_shortcut.clone(),
            "picker" => settings.picker_shortcut.clone(),
            _ => return Err("unknown shortcut kind".to_string()),
        }
    };

    // Validate by registering once and letting go again, so an invalid
    // or taken combo never gets persisted. The daemon picks up the saved
    // file and does the real registration. Re-recording the unchanged
    // combo skips the test, the daemon itself still holds it.
    if let Some(accel) = &accel {
        if old.as_deref() != Some(accel.as_str()) {
            let shortcuts = app.global_shortcut();
            shortcuts
                .register(accel.as_str())
                .map_err(|e| e.to_string())?;
            let _ = shortcuts.unregister(accel.as_str());
        }
    }

    let mut settings = state.0.lock().unwrap();
    match kind.as_str() {
        "capture" => settings.capture_shortcut = accel,
        "fullscreen" => settings.fullscreen_shortcut = accel,
        "picker" => settings.picker_shortcut = accel,
        _ => {}
    }
    save_settings(&app, &settings)
}

// Autostart must launch the resident daemon, not this UI binary. The
// daemon sits next to us: both in target/ during dev and in the app
// bundle's MacOS dir.
fn autolaunch() -> Result<auto_launch::AutoLaunch, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or_else(|| "no exe dir".to_string())?;
    let daemon = dir.join(if cfg!(windows) { "toolshot.exe" } else { "toolshot" });
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Toolshot")
        .set_app_path(&daemon.to_string_lossy())
        .set_use_launch_agent(true)
        .build()
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_autostart() -> bool {
    autolaunch()
        .map(|a| a.is_enabled().unwrap_or(false))
        .unwrap_or(false)
}

#[tauri::command]
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let manager = autolaunch()?;
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

#[tauri::command]
pub fn get_app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

// Only opens the project's own pages, updates are manual downloads.
#[tauri::command]
pub fn open_url(url: String) -> Result<(), String> {
    let allowed = url.starts_with("https://deekahy.github.io/toolshot")
        || url.starts_with("https://github.com/DeeKahy/toolshot");
    if !allowed {
        return Err("refusing to open unexpected url".to_string());
    }

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(&url).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn();

    result.map(|_| ()).map_err(|e| e.to_string())
}
