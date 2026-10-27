#![cfg_attr(windows, windows_subsystem = "windows")]
#[cfg(windows)]
use mini_client::audio;
#[cfg(windows)]
mod gui;
#[cfg(windows)]
use mini_client::network;
#[cfg(windows)]
mod settings;
fn main() {
    #[cfg(windows)]
    gui::run();
    #[cfg(not(windows))]
    eprintln!("mini-client is a Windows desktop application. Build it on Windows; cargo test validates the portable codec and network components here.");
}
