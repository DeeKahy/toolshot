use serde::Serialize;
use std::io::Cursor;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};
use xcap::image::codecs::png::{CompressionType, FilterType, PngEncoder};
use xcap::image::{ExtendedColorType, ImageEncoder};

use crate::BusyGuard;

// The finished capture, PNG-encoded, waiting for the editor to display it.
#[derive(Default)]
pub struct CaptureState(pub Mutex<Option<Vec<u8>>>);

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

// What the overlay is currently being used for: "capture" or "pick".
pub struct OverlayMode(pub Mutex<String>);

impl Default for OverlayMode {
    fn default() -> Self {
        OverlayMode(Mutex::new("capture".to_string()))
    }
}

#[tauri::command]
pub fn get_overlay_mode(state: State<'_, OverlayMode>) -> String {
    state.0.lock().unwrap().clone()
}

#[derive(Serialize)]
pub struct ScreenMeta {
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

// The monitor under the cursor, falling back to the primary, so capture
// starts where the user is working instead of always on the primary
// display. The containment test uses tauri's own monitor list, keeping
// the physical coordinate spaces consistent.
fn target_monitor(app: &AppHandle) -> Option<tauri::Monitor> {
    if let Ok(pos) = app.cursor_position() {
        if let Ok(monitors) = app.available_monitors() {
            for m in monitors {
                let p = m.position();
                let s = m.size();
                if pos.x >= p.x as f64
                    && pos.x < (p.x + s.width as i32) as f64
                    && pos.y >= p.y as f64
                    && pos.y < (p.y + s.height as i32) as f64
                {
                    return Some(m);
                }
            }
        }
    }
    app.primary_monitor().ok().flatten()
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
    open_overlay(app, "capture");
}

pub fn start_color_pick(app: &AppHandle) {
    open_overlay(app, "pick");
}

fn open_overlay(app: &AppHandle, mode: &str) {
    if let Some(existing) = app.get_webview_window("overlay") {
        let _ = existing.close();
        // The frozen frame is a full monitor of raw RGBA, tens of MB on
        // retina screens. Never keep it alive without an overlay using it.
        *app.state::<ScreenState>().0.lock().unwrap() = None;
        return;
    }

    *app.state::<OverlayMode>().0.lock().unwrap() = mode.to_string();

    // Window churn below must not count as the session ending; the guard
    // clears on every exit path, including early returns.
    let _busy = BusyGuard::new(app);

    // A lingering editor window would end up in the frozen frame.
    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.hide();
        let _ = editor.close();
        std::thread::sleep(std::time::Duration::from_millis(150));
    }

    let monitor = match target_monitor(app) {
        Some(m) => m,
        None => return,
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
        // The frozen frame canvas covers the window, so transparency is
        // cosmetic (it hides the instant before the frame paints). Only
        // macOS gets it: transparent undecorated windows are a known
        // white-screen trigger for WebView2 on Windows.
        .transparent(cfg!(target_os = "macos"))
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
            app.exit(1);
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

    if cfg!(target_os = "macos") {
        let _ = window.show();
        // Accessory apps do not activate on their own, without this the
        // overlay never sees keyboard events and Esc does nothing.
        let _ = window.set_focus();
    } else {
        // The window is opaque off macOS, showing it before the frozen
        // frame has painted flashes black over the screen. The page
        // calls overlay_ready once the frame (or a notice) is up; the
        // timer is a safety net so a wedged page cannot leave an
        // invisible session running.
        let fallback = window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            if !fallback.is_visible().unwrap_or(true) {
                let _ = fallback.show();
                let _ = fallback.set_focus();
            }
        });
    }
}

// Called by the overlay page once it has something to show: the frozen
// frame is painted, or a notice is displayed instead.
#[tauri::command]
pub fn overlay_ready(app: AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        if !overlay.is_visible().unwrap_or(false) {
            let _ = overlay.show();
            let _ = overlay.set_focus();
        }
    }
}

pub fn capture_fullscreen(app: &AppHandle) {
    // The capture runs with zero windows open, keep the process alive
    // until the editor exists. The guard rides into the thread and drops
    // when it finishes, on success and on every failure path alike.
    let busy = BusyGuard::new(app);

    // A lingering overlay or editor window would end up in the frame.
    let mut closed_window = false;
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
        closed_window = true;
    }
    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.hide();
        let _ = editor.close();
        closed_window = true;
    }

    let app = app.clone();
    std::thread::spawn(move || {
        let _busy = busy;
        if !screen_access::granted() && !screen_access::request() {
            eprintln!("screen recording permission not granted");
            app.exit(1);
            return;
        }

        // Give the tray menu and any just-closed windows time to leave
        // the screen before the frame is grabbed.
        let delay = if closed_window { 250 } else { 150 };
        std::thread::sleep(std::time::Duration::from_millis(delay));

        let monitor = match target_monitor(&app) {
            Some(m) => m,
            None => {
                app.exit(1);
                return;
            }
        };
        let scale = monitor.scale_factor();
        let pos = monitor.position().to_logical::<f64>(scale);

        let frozen = match freeze_screen(pos.x, pos.y, 0.0) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("fullscreen capture failed: {e}");
                app.exit(1);
                return;
            }
        };
        let png = match encode_png(&frozen.rgba, frozen.width, frozen.height) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to encode fullscreen capture: {e}");
                app.exit(1);
                return;
            }
        };
        *app.state::<CaptureState>().0.lock().unwrap() = Some(png);
        open_editor(&app, frozen.width, frozen.height);
    });
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
// Async on purpose: sync commands run on the main thread, and building
// a webview window there deadlocks WebView2 on Windows into a white
// window that never loads.
pub async fn capture_window(
    app: AppHandle,
    state: State<'_, CaptureState>,
    screen: State<'_, ScreenState>,
    id: u32,
) -> Result<(), String> {
    // Closing the overlay before the editor exists must not end the
    // process. The guard drops when the command returns: after the
    // editor is built on success, with the overlay still open on error.
    let _busy = BusyGuard::new(&app);
    capture_window_inner(&app, &state, &screen, id)
}

fn capture_window_inner(
    app: &AppHandle,
    state: &State<'_, CaptureState>,
    screen: &State<'_, ScreenState>,
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

    *state.0.lock().unwrap() = Some(png);
    *screen.0.lock().unwrap() = None;

    // Editor first, overlay after: the window count must never hit zero
    // mid transition. Windows processes the overlay destroy late enough
    // that the busy flag alone cannot cover the gap.
    open_editor(app, width, height);
    close_overlay_refocus_editor(app);
    Ok(())
}

fn close_overlay_refocus_editor(app: &AppHandle) {
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.set_focus();
    }
}

#[tauri::command]
pub fn get_screen_meta(state: State<'_, ScreenState>) -> Result<ScreenMeta, String> {
    let guard = state.0.lock().unwrap();
    let screen = guard.as_ref().ok_or_else(|| "no frozen screen".to_string())?;
    Ok(ScreenMeta {
        width: screen.width,
        height: screen.height,
        scale: screen.scale,
    })
}

// Raw RGBA over binary IPC, skipping PNG and base64 entirely. This is what
// keeps the loupe close to instant.
#[tauri::command]
pub fn get_screen_rgba(state: State<'_, ScreenState>) -> Result<tauri::ipc::Response, String> {
    let guard = state.0.lock().unwrap();
    let screen = guard.as_ref().ok_or_else(|| "no frozen screen".to_string())?;
    Ok(tauri::ipc::Response::new(screen.rgba.clone()))
}

// Rect arrives in logical overlay coordinates, cropping happens in the
// physical pixels of the frozen frame.
#[tauri::command]
// Async for the same WebView2 reason as capture_window.
pub async fn capture_area(
    app: AppHandle,
    screen: State<'_, ScreenState>,
    capture: State<'_, CaptureState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // Same guard rule as capture_window.
    let _busy = BusyGuard::new(&app);
    capture_area_inner(&app, &screen, &capture, x, y, width, height)
}

// Logical selection rect to physical crop bounds inside the frozen
// frame, clamped to its edges. Returns (x0, y0, w, h) in physical pixels.
fn crop_bounds(
    scale: f64,
    frame_w: u32,
    frame_h: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(u32, u32, u32, u32), String> {
    let x0 = ((x * scale).round().max(0.0) as u32).min(frame_w);
    let y0 = ((y * scale).round().max(0.0) as u32).min(frame_h);
    let x1 = (((x + width) * scale).round().max(0.0) as u32).min(frame_w);
    let y1 = (((y + height) * scale).round().max(0.0) as u32).min(frame_h);

    let crop_w = x1.saturating_sub(x0);
    let crop_h = y1.saturating_sub(y0);
    if crop_w < 2 || crop_h < 2 {
        return Err("selection too small".to_string());
    }
    Ok((x0, y0, crop_w, crop_h))
}

#[allow(clippy::too_many_arguments)]
fn capture_area_inner(
    app: &AppHandle,
    screen: &State<'_, ScreenState>,
    capture: &State<'_, CaptureState>,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let (crop_rgba, crop_w, crop_h) = {
        let guard = screen.0.lock().unwrap();
        let frozen = guard.as_ref().ok_or_else(|| "no frozen screen".to_string())?;

        let (x0, y0, crop_w, crop_h) =
            crop_bounds(frozen.scale, frozen.width, frozen.height, x, y, width, height)?;

        let stride = frozen.width as usize * 4;
        let mut crop_rgba = Vec::with_capacity(crop_w as usize * crop_h as usize * 4);
        for row in y0..y0 + crop_h {
            let start = row as usize * stride + x0 as usize * 4;
            let end = start + crop_w as usize * 4;
            crop_rgba.extend_from_slice(&frozen.rgba[start..end]);
        }
        (crop_rgba, crop_w, crop_h)
    };

    // The full frozen frame has served its purpose, free the tens of MB
    // instead of letting them idle until the next capture.
    *screen.0.lock().unwrap() = None;

    let png = encode_png(&crop_rgba, crop_w, crop_h)?;
    *capture.0.lock().unwrap() = Some(png);

    // Same ordering rule as capture_window: editor before overlay close.
    open_editor(app, crop_w, crop_h);
    close_overlay_refocus_editor(app);
    Ok(())
}

fn open_editor(app: &AppHandle, img_width: u32, img_height: u32) {
    if let Some(existing) = app.get_webview_window("editor") {
        let _ = existing.close();
    }

    // Captured pixels are physical, window sizes are logical. The editor
    // opens on the monitor the capture happened on (cursor's monitor).
    let monitor = target_monitor(app);
    let scale = monitor.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);

    let chrome_height = 76.0;
    let w = (img_width as f64 / scale + 32.0).clamp(480.0, 1280.0);
    let h = (img_height as f64 / scale + 32.0 + chrome_height).clamp(320.0, 840.0);

    let mut builder = WebviewWindowBuilder::new(app, "editor", WebviewUrl::App("editor.html".into()))
        .title("Toolshot")
        .inner_size(w, h)
        .focused(true);

    // center() centers on the primary monitor, so position by hand when
    // the capture came from another one.
    if let Some(m) = &monitor {
        let mpos = m.position().to_logical::<f64>(m.scale_factor());
        let msize = m.size().to_logical::<f64>(m.scale_factor());
        builder = builder.position(
            mpos.x + ((msize.width - w) / 2.0).max(0.0),
            mpos.y + ((msize.height - h) / 2.0).max(0.0),
        );
    } else {
        builder = builder.center();
    }

    match builder.build() {
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => {
            eprintln!("failed to open editor: {e}");
            app.exit(1);
        }
    }
}

#[tauri::command]
pub fn cancel_overlay(app: AppHandle, screen: State<'_, ScreenState>) {
    *screen.0.lock().unwrap() = None;
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
}

// PNG bytes over binary IPC: a fullscreen retina capture is several MB,
// and the base64+JSON detour used to cost real time on the editor open.
#[tauri::command]
pub fn get_capture_png(state: State<'_, CaptureState>) -> Result<tauri::ipc::Response, String> {
    let guard = state.0.lock().unwrap();
    let png = guard.as_ref().ok_or_else(|| "no capture available".to_string())?;
    Ok(tauri::ipc::Response::new(png.clone()))
}

// The editor sends the composited canvas (image plus annotations) as one
// raw payload: width and height as little endian u32s, then RGBA bytes.
fn parse_annotated(bytes: &[u8]) -> Result<(usize, usize, &[u8]), String> {
    if bytes.len() < 8 {
        return Err("payload too short".to_string());
    }
    let width = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let rgba = &bytes[8..];
    if rgba.len() != width * height * 4 {
        return Err("payload size mismatch".to_string());
    }
    Ok((width, height, rgba))
}

fn finish_editor_session(app: &AppHandle, capture: &State<'_, CaptureState>) {
    *capture.0.lock().unwrap() = None;
    if let Some(editor) = app.get_webview_window("editor") {
        let _ = editor.close();
    }
}

#[tauri::command]
pub fn copy_annotated(
    app: AppHandle,
    capture: State<'_, CaptureState>,
    request: tauri::ipc::Request<'_>,
) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected raw payload".to_string());
    };
    let (width, height, rgba) = parse_annotated(bytes)?;

    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard
        .set_image(arboard::ImageData {
            width,
            height,
            bytes: rgba.into(),
        })
        .map_err(|e| e.to_string())?;

    finish_editor_session(&app, &capture);
    Ok(())
}

// Same payload as copy_annotated, but written to a PNG picked in a save
// dialog. The suggested filename rides in a header because the body is
// raw bytes. The command validates and returns immediately; the dialog
// and the write happen on a worker thread (the blocking dialog API must
// not run on the main thread, which is where sync commands live). The
// editor window stays open until the save succeeds, so there is no
// zero-window gap to guard.
#[tauri::command]
pub fn save_annotated(app: AppHandle, request: tauri::ipc::Request<'_>) -> Result<(), String> {
    let tauri::ipc::InvokeBody::Raw(bytes) = request.body() else {
        return Err("expected raw payload".to_string());
    };
    let (width, height, rgba) = parse_annotated(bytes)?;
    let rgba = rgba.to_vec();

    let filename = request
        .headers()
        .get("x-filename")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("toolshot.png")
        .to_string();

    std::thread::spawn(move || {
        use tauri_plugin_dialog::DialogExt;

        let Some(path) = app
            .dialog()
            .file()
            .add_filter("PNG image", &["png"])
            .set_file_name(&filename)
            .blocking_save_file()
        else {
            return; // canceled, the editor stays open
        };
        let path = match path.into_path() {
            Ok(p) => p,
            Err(e) => {
                eprintln!("save failed: {e}");
                return;
            }
        };
        let png = match encode_png(&rgba, width as u32, height as u32) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("failed to encode png for save: {e}");
                return;
            }
        };
        if let Err(e) = std::fs::write(&path, png) {
            eprintln!("failed to write {}: {e}", path.display());
            return;
        }
        *app.state::<CaptureState>().0.lock().unwrap() = None;
        if let Some(editor) = app.get_webview_window("editor") {
            let _ = editor.close();
        }
    });
    Ok(())
}

#[tauri::command]
pub fn close_editor(app: AppHandle, capture: State<'_, CaptureState>) {
    finish_editor_session(&app, &capture);
}

#[cfg(test)]
mod tests {
    use super::{crop_bounds, encode_png, parse_annotated};

    #[test]
    fn crop_bounds_scales_logical_to_physical() {
        // 2x retina: logical 10,20 100x50 is physical 20,40 200x100.
        let (x0, y0, w, h) = crop_bounds(2.0, 3000, 2000, 10.0, 20.0, 100.0, 50.0).unwrap();
        assert_eq!((x0, y0, w, h), (20, 40, 200, 100));
    }

    #[test]
    fn crop_bounds_clamps_to_frame_edges() {
        // Selection dragged past the top-left and the bottom-right.
        let (x0, y0, w, h) = crop_bounds(1.0, 800, 600, -50.0, -50.0, 900.0, 700.0).unwrap();
        assert_eq!((x0, y0, w, h), (0, 0, 800, 600));
    }

    #[test]
    fn crop_bounds_rejects_tiny_selections() {
        assert!(crop_bounds(1.0, 800, 600, 10.0, 10.0, 1.0, 1.0).is_err());
        // Fully outside the frame collapses to zero size.
        assert!(crop_bounds(1.0, 800, 600, 900.0, 700.0, 50.0, 50.0).is_err());
    }

    #[test]
    fn crop_bounds_rounds_fractional_logical_pixels() {
        // 1.5x scale: 3.0 logical is 4.5 physical, rounds to 5 (as today's
        // behavior does); width 10 logical spans 4.5..19.5 -> 5..20.
        let (x0, y0, w, h) = crop_bounds(1.5, 1000, 1000, 3.0, 3.0, 10.0, 10.0).unwrap();
        assert_eq!((x0, y0), (5, 5));
        assert_eq!((w, h), (15, 15));
    }

    #[test]
    fn parse_annotated_roundtrip() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&3u32.to_le_bytes());
        payload.extend_from_slice(&[7u8; 2 * 3 * 4]);
        let (w, h, rgba) = parse_annotated(&payload).unwrap();
        assert_eq!((w, h), (2, 3));
        assert_eq!(rgba.len(), 24);
    }

    #[test]
    fn parse_annotated_rejects_bad_payloads() {
        assert!(parse_annotated(&[1, 2, 3]).is_err());
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&2u32.to_le_bytes());
        payload.extend_from_slice(&[0u8; 15]); // needs 16
        assert!(parse_annotated(&payload).is_err());
    }

    #[test]
    fn encode_png_roundtrips_pixels() {
        let rgba: Vec<u8> = vec![
            255, 0, 0, 255, /**/ 0, 255, 0, 255, //
            0, 0, 255, 255, /**/ 255, 255, 255, 128,
        ];
        let png = encode_png(&rgba, 2, 2).unwrap();
        let decoded = xcap::image::load_from_memory(&png).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 2));
        assert_eq!(decoded.into_raw(), rgba);
    }
}
