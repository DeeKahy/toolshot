// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// The resident process: a winit event loop with no windows, holding the
// tray icon and global shortcuts. Everything the user sees is the
// spawned toolshot-ui (Tauri) process, launched per session and gone
// when dismissed, so this process is all that stays in memory. Keep it
// free of UI, capture and image dependencies.

mod logging;

use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant, SystemTime};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::WindowId;

const SETTINGS_POLL: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum UserEvent {
    Menu(MenuEvent),
    Hotkey(GlobalHotKeyEvent),
}

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Capture,
    Fullscreen,
    Picker,
}

impl Action {
    fn arg(self) -> &'static str {
        match self {
            Action::Capture => "capture",
            Action::Fullscreen => "fullscreen",
            Action::Picker => "pick",
        }
    }

    fn settings_key(self) -> &'static str {
        match self {
            Action::Capture => "capture_shortcut",
            Action::Fullscreen => "fullscreen_shortcut",
            Action::Picker => "picker_shortcut",
        }
    }
}

fn main() {
    // Double-clicking the app while the daemon runs must not produce a
    // second tray icon. The lock lives for the process lifetime.
    let _lock = match acquire_instance_lock() {
        Some(lock) => lock,
        None => {
            eprintln!("toolshot daemon already running");
            return;
        }
    };

    // After the lock: a second instance must not truncate the log the
    // running daemon is writing to.
    logging::init("toolshot daemon", true);

    let mut builder = EventLoop::<UserEvent>::with_user_event();
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory);
    }
    let event_loop = builder.build().expect("failed to build event loop");

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event));
    }));
    let hotkey_proxy = event_loop.create_proxy();
    GlobalHotKeyEvent::set_event_handler(Some(move |event| {
        let _ = hotkey_proxy.send_event(UserEvent::Hotkey(event));
    }));

    let hotkey_manager = GlobalHotKeyManager::new().expect("failed to init global shortcuts");

    let mut daemon = Daemon {
        tray: None,
        hotkey_manager,
        desired: Vec::new(),
        registered: Vec::new(),
        settings_mtime: settings_mtime(),
        session: None,
        settings_child: None,
    };
    daemon.reload_shortcuts();

    event_loop.run_app(&mut daemon).expect("event loop failed");
}

struct Daemon {
    tray: Option<TrayIcon>,
    hotkey_manager: GlobalHotKeyManager,
    // What the settings file asks for, and what actually stuck. They can
    // differ transiently: the settings UI test-registers a combo before
    // saving, and on Windows the OS may not have released it yet when we
    // first try, so unregistered leftovers are retried every poll tick.
    desired: Vec<(Action, String)>,
    registered: Vec<(u32, Action, HotKey)>,
    settings_mtime: Option<SystemTime>,
    // One capture/pick/fullscreen session at a time; settings is separate
    // so it can stay open while capturing.
    session: Option<Child>,
    settings_child: Option<Child>,
}

impl Daemon {
    fn reload_shortcuts(&mut self) {
        for (_, _, hk) in self.registered.drain(..) {
            let _ = self.hotkey_manager.unregister(hk);
        }
        self.desired.clear();
        let settings = load_settings_json();
        for action in [Action::Capture, Action::Fullscreen, Action::Picker] {
            if let Some(accel) = settings
                .as_ref()
                .and_then(|s| s.get(action.settings_key()))
                .and_then(|v| v.as_str())
            {
                self.desired.push((action, accel.to_string()));
            }
        }
        self.register_missing(true);
    }

    fn register_missing(&mut self, log_errors: bool) {
        for (action, accel) in self.desired.clone() {
            if self.registered.iter().any(|(_, a, _)| *a == action) {
                continue;
            }
            match parse_accel(&accel) {
                Ok(hk) => match self.hotkey_manager.register(hk) {
                    Ok(()) => self.registered.push((hk.id(), action, hk)),
                    Err(e) => {
                        if log_errors {
                            eprintln!("could not register shortcut {accel}, will retry: {e}");
                        }
                    }
                },
                Err(e) => {
                    if log_errors {
                        eprintln!("could not parse shortcut {accel}: {e}");
                    }
                }
            }
        }
    }

    fn build_tray(&mut self) {
        let capture = MenuItem::with_id("capture", "Capture", true, None);
        let fullscreen = MenuItem::with_id("capture_fullscreen", "Capture Fullscreen", true, None);
        let color_picker = MenuItem::with_id("color_picker", "Color Picker", true, None);
        let presenting = MenuItem::with_id("presenting", "Presenting Mode", false, None);
        let settings = MenuItem::with_id("settings", "Settings", true, None);
        let quit = MenuItem::with_id("quit", "Quit Toolshot", true, None);

        let menu = Menu::new();
        let sep1 = PredefinedMenuItem::separator();
        let sep2 = PredefinedMenuItem::separator();
        if let Err(e) = menu.append_items(&[
            &capture,
            &fullscreen,
            &sep1,
            &color_picker,
            &presenting,
            &sep2,
            &settings,
            &quit,
        ]) {
            eprintln!("failed to build tray menu: {e}");
            return;
        }

        let mut builder = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Toolshot")
            .with_menu_on_left_click(true);
        if let Some(icon) = load_tray_icon() {
            builder = builder.with_icon(icon).with_icon_as_template(true);
        }
        match builder.build() {
            Ok(tray) => self.tray = Some(tray),
            Err(e) => eprintln!("failed to create tray icon: {e}"),
        }
    }

    fn spawn(&mut self, mode: &str) -> Option<Child> {
        let ui = ui_binary()?;
        match Command::new(&ui).arg(mode).spawn() {
            Ok(child) => Some(child),
            Err(e) => {
                eprintln!("failed to spawn {} {mode}: {e}", ui.display());
                None
            }
        }
    }

    fn kill_session(&mut self) {
        if let Some(mut c) = self.session.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }

    // Capture and pick toggle: triggering them while their session is
    // open dismisses it, like the old in-process overlay toggle.
    fn toggle_session(&mut self, action: Action) {
        if child_alive(&mut self.session) {
            self.kill_session();
            return;
        }
        self.session = self.spawn(action.arg());
    }

    fn start_fullscreen(&mut self) {
        // A lingering overlay or editor would end up in the frame; the UI
        // process waits before grabbing it.
        if child_alive(&mut self.session) {
            self.kill_session();
        }
        self.session = self.spawn(Action::Fullscreen.arg());
    }

    fn open_settings(&mut self) {
        if child_alive(&mut self.settings_child) {
            return;
        }
        self.settings_child = self.spawn("settings");
    }

    fn quit(&mut self, event_loop: &ActiveEventLoop) {
        self.kill_session();
        if let Some(mut c) = self.settings_child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        event_loop.exit();
    }
}

impl ApplicationHandler<UserEvent> for Daemon {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        match cause {
            // The tray must be created after the event loop is running,
            // macOS rejects it earlier.
            StartCause::Init => self.build_tray(),
            StartCause::ResumeTimeReached { .. } => {
                // The settings window is a separate process, shortcut
                // changes arrive by watching the settings file. Bindings
                // that failed to register are retried quietly.
                let mtime = settings_mtime();
                if mtime != self.settings_mtime {
                    self.settings_mtime = mtime;
                    self.reload_shortcuts();
                } else {
                    self.register_missing(false);
                }
            }
            _ => {}
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Menu(e) => match e.id().as_ref() {
                "capture" => self.toggle_session(Action::Capture),
                "capture_fullscreen" => self.start_fullscreen(),
                "color_picker" => self.toggle_session(Action::Picker),
                "settings" => self.open_settings(),
                "quit" => self.quit(event_loop),
                _ => {}
            },
            UserEvent::Hotkey(e) => {
                if e.state() != HotKeyState::Pressed {
                    return;
                }
                let action = self
                    .registered
                    .iter()
                    .find(|(id, _, _)| *id == e.id())
                    .map(|(_, a, _)| *a);
                match action {
                    Some(Action::Capture) => self.toggle_session(Action::Capture),
                    Some(Action::Fullscreen) => self.start_fullscreen(),
                    Some(Action::Picker) => self.toggle_session(Action::Picker),
                    None => {}
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::WaitUntil(Instant::now() + SETTINGS_POLL));
    }
}

// The UI binary sits next to the daemon, both in target/ during dev and
// in Contents/MacOS inside the app bundle.
fn ui_binary() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let name = if cfg!(windows) {
        "toolshot-ui.exe"
    } else {
        "toolshot-ui"
    };
    let path = dir.join(name);
    if !path.exists() {
        eprintln!("ui binary not found at {}", path.display());
        return None;
    }
    Some(path)
}

fn child_alive(child: &mut Option<Child>) -> bool {
    match child {
        Some(c) => match c.try_wait() {
            Ok(None) => true,
            _ => {
                *child = None;
                false
            }
        },
        None => false,
    }
}

fn config_dir() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("dev.deekahy.toolshot");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

fn acquire_instance_lock() -> Option<std::fs::File> {
    use fs2::FileExt;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(config_dir()?.join("daemon.lock"))
        .ok()?;
    file.try_lock_exclusive().ok()?;
    Some(file)
}

fn settings_path() -> Option<PathBuf> {
    Some(config_dir()?.join("settings.json"))
}

fn settings_mtime() -> Option<SystemTime> {
    std::fs::metadata(settings_path()?).ok()?.modified().ok()
}

fn load_settings_json() -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(settings_path()?).ok()?;
    serde_json::from_str(&text).ok()
}

// Accelerators are stored the way the settings page records them:
// modifiers then a W3C key code, joined by '+', e.g. "Cmd+Shift+KeyC".
fn parse_accel(accel: &str) -> Result<HotKey, String> {
    use global_hotkey::hotkey::{Code, Modifiers};
    use std::str::FromStr;

    let parts: Vec<&str> = accel.split('+').collect();
    let (code_str, mod_strs) = parts
        .split_last()
        .ok_or_else(|| "empty accelerator".to_string())?;

    let mut mods = Modifiers::empty();
    for m in mod_strs {
        match *m {
            "Cmd" => mods |= Modifiers::META,
            "Ctrl" => mods |= Modifiers::CONTROL,
            "Alt" => mods |= Modifiers::ALT,
            "Shift" => mods |= Modifiers::SHIFT,
            other => return Err(format!("unknown modifier: {other}")),
        }
    }
    if mods.is_empty() {
        return Err("shortcut needs at least one modifier".to_string());
    }

    let code = Code::from_str(code_str).map_err(|_| format!("unknown key: {code_str}"))?;
    Ok(HotKey::new(Some(mods), code))
}

#[cfg(test)]
mod tests {
    use super::parse_accel;
    use global_hotkey::hotkey::{Code, HotKey, Modifiers};

    #[test]
    fn parses_modifiers_and_key() {
        let hk = parse_accel("Cmd+Shift+KeyC").unwrap();
        assert_eq!(hk, HotKey::new(Some(Modifiers::META | Modifiers::SHIFT), Code::KeyC));
    }

    #[test]
    fn parses_every_modifier_name() {
        let hk = parse_accel("Cmd+Ctrl+Alt+Shift+F12").unwrap();
        let all = Modifiers::META | Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
        assert_eq!(hk, HotKey::new(Some(all), Code::F12));
    }

    #[test]
    fn rejects_missing_modifier() {
        assert!(parse_accel("KeyC").is_err());
    }

    #[test]
    fn rejects_unknown_modifier_and_key() {
        assert!(parse_accel("Hyper+KeyC").is_err());
        assert!(parse_accel("Cmd+NotAKey").is_err());
    }

    #[test]
    fn rejects_empty_accelerator() {
        assert!(parse_accel("").is_err());
    }
}

fn load_tray_icon() -> Option<tray_icon::Icon> {
    let bytes: &[u8] = include_bytes!("../../src-tauri/icons/32x32.png");
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf.chunks(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        _ => return None,
    };
    tray_icon::Icon::from_rgba(rgba, info.width, info.height).ok()
}
