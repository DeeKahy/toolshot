use base64::Engine;
use serde::Serialize;
use std::io::Cursor;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use xcap::image::codecs::png::{CompressionType, FilterType, PngEncoder};
use xcap::image::{ExtendedColorType, ImageEncoder};

pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub png: Vec<u8>,
}

#[derive(Default)]
pub struct CaptureState(pub Mutex<Option<Capture>>);

// Full-screen frame grabbed the moment the overlay opens. Area selection
// crops from this so the pixels cannot change mid drag, and the magnifier
// loupe samples from it.
pub struct FrozenScreen {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    // Physical pixels per logical overlay pixel.
    pub scale: f64,
}

#[derive(Default)]
pub struct ScreenState(pub Mutex<Option<FrozenScreen>>);

#[derive(Serialize)]
pub struct ScreenMeta {
    pub png: String,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

fn encode_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>, String> {
    let mut png = Vec::new();
    let encoder = PngEncoder::new_with_quality(
        Cursor::new(&mut png),
        CompressionType::Fast,
        FilterType::Adaptive,
    );
    encoder
        .write_image(rgba, width, height, ExtendedColorType::Rgba8)
        .map_err(|e| e.to_string())?;
    Ok(png)
}

fn freeze_screen(pos_x: f64, pos_y: f64, logical_width: f64) -> Result<FrozenScreen, String> {
    let monitor = xcap::Monitor::from_point(pos_x as i32 + 1, pos_y as i32 + 1)
        .or_else(|_| {
            xcap::Monitor::all()
                .map_err(|e| e.to_string())?
                .into_iter()
                .next()
                .ok_or_else(|| "no monitors found".to_string())
        })
        .map_err(|e| e.to_string())?;

    let image = monitor.capture_image().map_err(|e| e.to_string())?;
    let (width, height) = (image.width(), image.height());
    let rgba = image.into_raw();
    let scale = if logical_width > 0.0 {
        width as f64 / logical_width
    } else {
        1.0
    };

    Ok(FrozenScreen { width, height, rgba, scale })
}

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

    // A lingering editor window would end up in the frozen frame.
    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.hide();
        let _ = editor.close();
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    let monitor = match app.primary_monitor() {
        Ok(Some(m)) => m,
        _ => return,
    };
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let pos = monitor.position().to_logical::<f64>(scale);

    // Drop any stale frame before the overlay can ask for it.
    *app.state::<ScreenState>().0.lock().unwrap() = None;

    // Create the window hidden first so the webview boots while the screen
    // capture runs, then reveal it once the frozen frame is ready. Hidden
    // windows do not render, so the overlay is never part of the frame.
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
        .visible(false)
        .focused(false)
        .build();

    let window = match result {
        Ok(window) => window,
        Err(e) => {
            eprintln!("failed to open overlay: {e}");
            return;
        }
    };

    match freeze_screen(pos.x, pos.y, size.width) {
        Ok(frozen) => {
            *app.state::<ScreenState>().0.lock().unwrap() = Some(frozen);
        }
        Err(e) => {
            // Window picking still works without the frozen frame, the
            // overlay just loses the loupe and area selection.
            eprintln!("failed to freeze screen: {e}");
        }
    }

    let _ = window.show();
    // Accessory apps do not activate on their own, without this the
    // overlay never sees keyboard events and Esc does nothing.
    let _ = window.set_focus();
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
    let png = encode_png(&rgba, width, height)?;

    *state.0.lock().unwrap() = Some(Capture { width, height, rgba, png });

    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
    open_editor(&app, width, height);
    Ok(())
}

#[tauri::command]
pub fn get_screen_png(state: State<'_, ScreenState>) -> Result<ScreenMeta, String> {
    let guard = state.0.lock().unwrap();
    let screen = guard.as_ref().ok_or_else(|| "no frozen screen".to_string())?;
    let png = encode_png(&screen.rgba, screen.width, screen.height)?;
    Ok(ScreenMeta {
        png: base64::engine::general_purpose::STANDARD.encode(&png),
        width: screen.width,
        height: screen.height,
        scale: screen.scale,
    })
}

// Rect arrives in logical overlay coordinates, cropping happens in the
// physical pixels of the frozen frame.
#[tauri::command]
pub fn capture_area(
    app: AppHandle,
    screen: State<'_, ScreenState>,
    capture: State<'_, CaptureState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let (crop_rgba, crop_w, crop_h) = {
        let guard = screen.0.lock().unwrap();
        let frozen = guard.as_ref().ok_or_else(|| "no frozen screen".to_string())?;
        let s = frozen.scale;

        let x0 = ((x * s).round().max(0.0) as u32).min(frozen.width);
        let y0 = ((y * s).round().max(0.0) as u32).min(frozen.height);
        let x1 = (((x + width) * s).round().max(0.0) as u32).min(frozen.width);
        let y1 = (((y + height) * s).round().max(0.0) as u32).min(frozen.height);

        let crop_w = x1.saturating_sub(x0);
        let crop_h = y1.saturating_sub(y0);
        if crop_w < 2 || crop_h < 2 {
            return Err("selection too small".to_string());
        }

        let stride = frozen.width as usize * 4;
        let mut crop_rgba = Vec::with_capacity(crop_w as usize * crop_h as usize * 4);
        for row in y0..y1 {
            let start = row as usize * stride + x0 as usize * 4;
            let end = start + crop_w as usize * 4;
            crop_rgba.extend_from_slice(&frozen.rgba[start..end]);
        }
        (crop_rgba, crop_w, crop_h)
    };

    let png = encode_png(&crop_rgba, crop_w, crop_h)?;
    *capture.0.lock().unwrap() = Some(Capture {
        width: crop_w,
        height: crop_h,
        rgba: crop_rgba,
        png,
    });

    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
    open_editor(&app, crop_w, crop_h);
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
