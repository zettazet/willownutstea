#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
use mini_client::audio;
#[cfg(windows)]
mod diagnostics;
#[cfg(windows)]
mod gui;
#[cfg(windows)]
mod message_input;
#[cfg(windows)]
mod profile_ui;
#[cfg(windows)]
mod sounds;
#[cfg(windows)]
mod theme;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
use mini_client::network;
#[cfg(windows)]
mod settings;
fn main() {
    #[cfg(windows)]
    {
        diagnostics::init();
        if let Err(error) = gui::run() {
            diagnostics::startup_error(&error.to_string());
            std::process::exit(1);
        }
    }
    #[cfg(not(windows))]
    eprintln!("mini-client is a Windows desktop application. Build it on Windows; cargo test validates the portable codec and network components here.");
}
