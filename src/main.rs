#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    sakura_uninstaller::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Sakura Uninstaller is a Windows application.");
}
