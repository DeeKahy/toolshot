use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle,
};

pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let capture_window = MenuItem::with_id(app, "capture_window", "Capture Window", true, None::<&str>)?;
    let capture_area = MenuItem::with_id(app, "capture_area", "Capture Area", false, None::<&str>)?;
    let capture_fullscreen = MenuItem::with_id(app, "capture_fullscreen", "Capture Fullscreen", false, None::<&str>)?;
    let color_picker = MenuItem::with_id(app, "color_picker", "Color Picker", false, None::<&str>)?;
    let presenting = MenuItem::with_id(app, "presenting", "Presenting Mode", false, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", false, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Toolshot", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &capture_window,
            &capture_area,
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
            "capture_window" => crate::capture::start_window_pick(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    Ok(())
}
