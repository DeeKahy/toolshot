mod capture;
mod tray;

pub fn run() {
    let app = tauri::Builder::default()
        .manage(capture::CaptureState::default())
        .manage(capture::ScreenState::default())
        .invoke_handler(tauri::generate_handler![
            capture::check_screen_permission,
            capture::list_windows,
            capture::capture_window,
            capture::get_screen_meta,
            capture::get_screen_rgba,
            capture::capture_area,
            capture::cancel_overlay,
            capture::get_capture_png,
            capture::copy_capture,
            capture::close_editor,
        ])
        .setup(|app| {
            // Tray-only app, no dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            // Close the overlay when the user swipes to another desktop.
            // Focus events do not cover this: an accessory app can keep the
            // key window across a space switch, so listen for the actual
            // workspace notification instead.
            #[cfg(target_os = "macos")]
            {
                use block2::RcBlock;
                use objc2_app_kit::{NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification};
                use objc2_foundation::{NSNotification, NSOperationQueue};
                use std::ptr::NonNull;
                use tauri::Manager;

                let handle = app.handle().clone();
                let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                    if let Some(overlay) = handle.get_webview_window("overlay") {
                        let _ = overlay.close();
                    }
                });
                unsafe {
                    let center = NSWorkspace::sharedWorkspace().notificationCenter();
                    let token = center.addObserverForName_object_queue_usingBlock(
                        Some(NSWorkspaceActiveSpaceDidChangeNotification),
                        None,
                        Some(&NSOperationQueue::mainQueue()),
                        &block,
                    );
                    // Listens for the lifetime of the app.
                    std::mem::forget(token);
                }
            }

            tray::create_tray(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building toolshot");

    app.run(|_app, event| {
        // Keep running from the tray after the last window closes.
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            if code.is_none() {
                api.prevent_exit();
            }
        }
    });
}
