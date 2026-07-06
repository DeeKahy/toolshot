use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle,
};

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let capture = MenuItem::with_id(app, "capture", "Capture", true, None::<&str>)?;
    let capture_fullscreen = MenuItem::with_id(app, "capture_fullscreen", "Capture Fullscreen", true, None::<&str>)?;
    let color_picker = MenuItem::with_id(app, "color_picker", "Color Picker", true, None::<&str>)?;
    let presenting = MenuItem::with_id(app, "presenting", "Presenting Mode", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Toolshot", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &capture,
            &capture_fullscreen,
            &PredefinedMenuItem::separator(app)?,
            &color_picker,
            &presenting,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().expect("bundled icon missing").clone())
        .icon_as_template(true)
        .tooltip("Toolshot")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "capture" => crate::capture::start_window_pick(app),
            "capture_fullscreen" => crate::capture::capture_fullscreen(app),
            "color_picker" => crate::capture::start_color_pick(app),
            "settings" => crate::settings::open_settings_window(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}
