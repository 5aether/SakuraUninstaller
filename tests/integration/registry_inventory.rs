use super::read_app;
use crate::utils::strings::matches;
use std::io;
use windows_registry::CURRENT_USER;

struct Fixture(String);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = CURRENT_USER.remove_tree(&self.0);
    }
}

#[test]
fn parses_only_removable_named_msi_fixtures_and_searches_metadata() -> io::Result<()> {
    let fixture = Fixture(format!(
        r"Software\SakuraUninstaller-test-{}",
        uuid::Uuid::new_v4()
    ));
    let key = CURRENT_USER.create(&fixture.0).map_err(io::Error::other)?;
    let code = "{053F961B-3743-4024-AFA7-FB952630EE6A}";
    assert!(read_app(&key, "test", code).is_none());
    key.set_string("DisplayName", "Sakura Editor")
        .map_err(io::Error::other)?;
    key.set_string("Publisher", "Test Studio")
        .map_err(io::Error::other)?;
    key.set_string("DisplayVersion", "3.1")
        .map_err(io::Error::other)?;
    key.set_string("InstallDate", "20240229")
        .map_err(io::Error::other)?;
    key.set_u32("EstimatedSize", 512)
        .map_err(io::Error::other)?;
    key.set_u32("WindowsInstaller", 1)
        .map_err(io::Error::other)?;
    let app = read_app(&key, "test", code).unwrap();
    assert!(app.removable() && app.quiet_supported());
    assert_eq!(app.msi.as_deref(), Some(code));
    assert_eq!(app.size_kib, Some(512));
    assert_eq!(app.install_date.as_deref(), Some("2024-02-29"));
    assert!(matches("studio editor 3.1", &app.search_text()));
    assert!(!matches("airport", &app.search_text()));

    key.set_u32("NoRemove", 1).map_err(io::Error::other)?;
    assert!(!read_app(&key, "test", code).unwrap().removable());
    key.set_u32("SystemComponent", 1)
        .map_err(io::Error::other)?;
    assert!(read_app(&key, "test", code).is_none());
    Ok(())
}

#[test]
fn identifies_installer_metadata_and_requires_a_registered_quiet_command() -> io::Result<()> {
    let fixture = Fixture(format!(
        r"Software\SakuraUninstaller-test-{}",
        uuid::Uuid::new_v4()
    ));
    let root = CURRENT_USER.create(&fixture.0).map_err(io::Error::other)?;
    for (name, label, field, value) in [
        ("inno", "Inno Setup", "Inno Setup: Setup Version", "6.2"),
        ("nsis", "NSIS", "NSIS:Version", "3.10"),
        (
            "electron",
            "Squirrel / Electron",
            "UninstallString",
            r#""C:\Fixture\Update.exe" --uninstall"#,
        ),
    ] {
        let key = root.create(name).map_err(io::Error::other)?;
        key.set_string("DisplayName", name)
            .map_err(io::Error::other)?;
        key.set_string("UninstallString", r"C:\Fixture\uninstall.exe")
            .map_err(io::Error::other)?;
        key.set_string(field, value).map_err(io::Error::other)?;
        let app = read_app(&key, "test", name).unwrap();
        assert_eq!(app.installer, label);
        assert!(app.removable());
        assert!(!app.quiet_supported());
        key.set_string("QuietUninstallString", r"C:\Fixture\uninstall.exe /S")
            .map_err(io::Error::other)?;
        assert!(read_app(&key, "test", name).unwrap().quiet_supported());
    }
    Ok(())
}
