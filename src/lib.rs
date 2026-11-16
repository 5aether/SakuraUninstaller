#![cfg(windows)]

mod reporting;
mod safety;
mod scanner;
mod state;
mod storage;
mod ui;
mod uninstall;
mod updates;
mod utils;

slint::include_modules!();

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args_os().any(|argument| argument == "--smoke-test") {
        let (inventory, warning) = scanner::installed_apps::full()?;
        let packaged = inventory.iter().filter(|app| app.package.is_some()).count();
        println!(
            "Inventory smoke test: {} Win32 entries, {} MSIX entries",
            inventory.len() - packaged,
            packaged
        );
        if let Some(error) = warning {
            eprintln!("MSIX inventory unavailable: {error}");
        }
        AppWindow::new()?;
        return Ok(());
    }

    let locations = storage::Locations::discover()?;
    let _application_lock = locations.lock()?;
    let window = AppWindow::new()?;
    ui::connect(&window, locations);
    window.run()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/integration/quarantine.rs"]
mod quarantine_tests;
