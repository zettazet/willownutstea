use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
fn path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("MiniVoice/client.log")
}
pub fn record(message: &str) {
    let file = path();
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if std::fs::metadata(&file).is_ok_and(|m| m.len() > 256 * 1024) {
        let _ = std::fs::rename(&file, file.with_extension("previous.log"));
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(file) {
        let time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(f, "{time} {message}");
    }
}
fn alert(message: &str) {
    let text: Vec<u16> = message.encode_utf16().chain(Some(0)).collect();
    let title: Vec<u16> = "Mini Voice".encode_utf16().chain(Some(0)).collect();
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
pub fn startup_error(error: &str) {
    record(&format!("startup error: {error}"));
    alert(&format!(
        "Mini Voice could not start: {error}\n\nDiagnostic log: {}",
        path().display()
    ));
}
pub fn init() {
    record("starting client");
    std::panic::set_hook(Box::new(|info| {
        // Do not log panic payloads, credentials, tokens or message contents.
        record(&format!(
            "panic at {:?}\n{}",
            info.location(),
            std::backtrace::Backtrace::force_capture()
        ));
        alert(&format!(
            "Mini Voice encountered an unexpected error and must close.\nDiagnostic log: {}",
            path().display()
        ));
    }));
}
