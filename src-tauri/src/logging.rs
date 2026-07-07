// Both processes are windowless, so stderr goes nowhere the user can
// see. Redirect it into a log file in the config dir at startup; all
// existing eprintln calls keep working and become debuggable after the
// fact. The daemon crate carries its own copy of this file, the two
// must agree on the log path.

use std::fs::OpenOptions;
use std::path::PathBuf;

const MAX_LOG_BYTES: u64 = 512 * 1024;

pub fn log_path() -> Option<PathBuf> {
    let dir = dirs::config_dir()?.join("dev.deekahy.toolshot");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("toolshot.log"))
}

// `truncate_large` starts the file over once it grows past the cap; only
// the daemon passes true, so short-lived UI sessions cannot yank the file
// out from under a running daemon that has it open.
pub fn init(process: &str, truncate_large: bool) {
    let Some(path) = log_path() else { return };
    if truncate_large {
        if let Ok(meta) = std::fs::metadata(&path) {
            if meta.len() > MAX_LOG_BYTES {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    if redirect_stderr(&file).is_ok() {
        // The OS handle must outlive the redirect; one leaked fd per
        // process, reclaimed at exit.
        std::mem::forget(file);
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        eprintln!("=== {process} started (unix time {secs}) ===");
    }
}

#[cfg(unix)]
fn redirect_stderr(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::io::AsRawFd;
    if unsafe { libc::dup2(file.as_raw_fd(), 2) } == -1 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(windows)]
fn redirect_stderr(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::Console::{SetStdHandle, STD_ERROR_HANDLE};
    if unsafe { SetStdHandle(STD_ERROR_HANDLE, file.as_raw_handle() as _) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}
