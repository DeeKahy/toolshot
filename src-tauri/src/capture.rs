use base64::Engine;
use serde::Serialize;
use std::io::Cursor;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use xcap::image::{ExtendedColorType, ImageFormat};

pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub png: Vec<u8>,
}

#[derive(Default)]
pub struct CaptureState(pub Mutex<Option<Capture>>);

#[derive(Serialize, Clone)]
pub struct WindowInfo {
    pub id: u32,
    pub app_name: String,
    pub title: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub z: i32,
}

#[cfg(target_os = "macos")]
mod screen_access {
    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn granted() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    // Triggers the system prompt on first call, afterwards the user has to
    // flip the switch in System Settings themselves.
    pub fn request() -> bool {
        unsafe { CGRequestScreenCaptureAccess() }
    }
}

#[cfg(not(target_os = "macos"))]
mod screen_access {
    pub fn granted() -> bool {
        true
    }

    pub fn request() -> bool {
        true
    }
}

#[tauri::command]
pub fn check_screen_permission() -> bool {
    if screen_access::granted() {
        return true;
    }
    screen_access::request()
}

// System surfaces that make no sense as click-to-capture targets.
const EXCLUDED_APPS: &[&str] = &[
    "Window Server",
    "Dock",
    "Control Center",
    "Control Centre",
    "Notification Center",
    "Notification Centre",
    "Spotlight",
    "Wallpaper",
    "Screenshot",
];

pub fn start_window_pick(app: &AppHandle) {
    if let Some(existing) = app.get_webview_window("overlay") {
        let _ = existing.close();
        return;
    }

    let monitor = match app.primary_monitor() {
        Ok(Some(m)) => m,
        _ => return,
    };
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let pos = monitor.position().to_logical::<f64>(scale);

    let result = WebviewWindowBuilder::new(app, "overlay", WebviewUrl::App("overlay.html".into()))
        .title("toolshot overlay")
        .position(pos.x, pos.y)
        .inner_size(size.width, size.height)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .skip_taskbar(true)
        .accept_first_mouse(true)
        .visible_on_all_workspaces(true)
        .focused(true)
        .build();

    match result {
        // Accessory apps do not activate on their own, without this the
        // overlay never sees keyboard events and Esc does nothing.
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("failed to open overlay: {e}"),
    }
}

#[tauri::command]
pub fn list_windows() -> Result<Vec<WindowInfo>, String> {
    let windows = xcap::Window::all().map_err(|e| e.to_string())?;

    let mut infos: Vec<WindowInfo> = windows
        .iter()
        .filter_map(|w| {
            let app_name = w.app_name().unwrap_or_default();
            let title = w.title().unwrap_or_default();
            if w.is_minimized().unwrap_or(false) {
                return None;
            }
            if EXCLUDED_APPS.contains(&app_name.as_str()) {
                return None;
            }
            if app_name.eq_ignore_ascii_case("toolshot") || title == "toolshot overlay" {
                return None;
            }
            let width = w.width().unwrap_or(0);
            let height = w.height().unwrap_or(0);
            if width < 40 || height < 40 {
                return None;
            }
            Some(WindowInfo {
                id: w.id().unwrap_or(0),
                app_name,
                title,
                x: w.x().unwrap_or(0),
                y: w.y().unwrap_or(0),
                width,
                height,
                z: w.z().unwrap_or(0),
            })
        })
        .collect();

    // Topmost first so the frontend can hit test by taking the first match.
    infos.sort_by(|a, b| b.z.cmp(&a.z));
    Ok(infos)
}

#[tauri::command]
pub fn capture_window(
    app: AppHandle,
    state: State<'_, CaptureState>,
    id: u32,
) -> Result<(), String> {
    let windows = xcap::Window::all().map_err(|e| e.to_string())?;
    let target = windows
        .iter()
        .find(|w| w.id().map(|wid| wid == id).unwrap_or(false))
        .ok_or_else(|| "window no longer exists".to_string())?;

    let image = target.capture_image().map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());
    let rgba = image.into_raw();

    let mut png = Vec::new();
    xcap::image::write_buffer_with_format(
        &mut Cursor::new(&mut png),
        &rgba,
        width,
        height,
        ExtendedColorType::Rgba8,
        ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;

    *state.0.lock().unwrap() = Some(Capture { width, height, rgba, png });

    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
    open_editor(&app, width, height);
    Ok(())
}

fn open_editor(app: &AppHandle, img_width: u32, img_height: u32) {
    if let Some(existing) = app.get_webview_window("editor") {
        let _ = existing.close();
    }

    // Captured pixels are physical, window sizes are logical.
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| m.scale_factor())
        .unwrap_or(1.0);

    let chrome_height = 76.0;
    let w = (img_width as f64 / scale + 32.0).clamp(480.0, 1280.0);
    let h = (img_height as f64 / scale + 32.0 + chrome_height).clamp(320.0, 840.0);

    let result = WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("editor.html".into()))
        .title("Toolshot")
        .inner_size(w, h)
        .center()
        .focused(true)
        .build();

    match result {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("failed to open editor: {e}"),
    }
}

#[tauri::command]
pub fn cancel_overlay(app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
}

#[tauri::command]
pub fn get_capture_png(state: State<'_, CaptureState>) -> Result<String, String> {
    let guard = state.0.lock().unwrap();
    let capture = guard.as_ref().ok_or_else(|| "no capture available".to_string())?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&capture.png))
}

#[tauri::command]
pub fn copy_capture(app: AppHandle, state: State<'_, CaptureState>) -> Result<(), String> {
    {
        let guard = state.0.lock().unwrap();
        let capture = guard.as_ref().ok_or_else(|| "no capture available".to_string())?;

        let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
        clipboard
            .set_image(arboard::ImageData {
                width: capture.width as usize,
                height: capture.height as usize,
                bytes: capture.rgba.as_slice().into(),
            })
            .map_err(|e| e.to_string())?;
    }

    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.close();
    }
    Ok(())
}

#[tauri::command]
pub fn close_editor(app: AppHandle) {
    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.close();
    }
}
