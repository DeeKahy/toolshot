use std::sync::Mutex;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindowBuilder};

#[derive(Default)]
pub struct PickerState(pub Mutex<Option<(u8, u8, u8)>>);

// Async on purpose: sync commands run on the main thread, and building
// a webview window there deadlocks WebView2 on Windows into a white
// window that never loads.
#[tauri::command]
pub async fn pick_color(
    app: AppHandle,
    state: State<'_, PickerState>,
    screen: State<'_, crate::capture::ScreenState>,
    r: u8,
    g: u8,
    b: u8,
) -> Result<(), String> {
    *state.0.lock().unwrap() = Some((r, g, b));
    // The color came from the frozen frame, which is done now.
    *screen.0.lock().unwrap() = None;

    // Popup first, overlay closed after: the window count must never hit
    // zero mid transition. Windows processes the overlay destroy late
    // enough that the busy flag alone cannot cover the gap. The guard
    // clears on success and on the error return alike.
    let _busy = crate::BusyGuard::new(&app);

    if let Some(existing) = app.get_webview_window("colorpick") {
        let _ = existing.close();
    }

    let window = WebviewWindowBuilder::new(&app, "colorpick", WebviewUrl::App("picker.html".into()))
        .title("Color")
        .inner_size(330.0, 420.0)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .always_on_top(true)
        .center()
        .focused(true)
        .build()
        .map_err(|e| e.to_string())?;
    if let Some(overlay) = app.get_webview_window("overlay") {
        let _ = overlay.close();
    }
    let _ = window.set_focus();
    Ok(())
}

#[tauri::command]
pub fn get_picked_color(state: State<'_, PickerState>) -> Result<(u8, u8, u8), String> {
    state
        .0
        .lock()
        .unwrap()
        .ok_or_else(|| "no color picked".to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn close_color_popup(app: AppHandle) {
    if let Some(window) = app.get_webview_window("colorpick") {
        let _ = window.close();
    }
}
