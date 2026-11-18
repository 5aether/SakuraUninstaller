use crate::{
    safety::rules::Rules,
    scanner::{
        installed_apps::App,
        leftovers::{Evidence, Profile, remaining, snapshot},
    },
    storage::Locations,
};
use std::{fs, io, os::windows::fs::MetadataExt};
use uuid::Uuid;

fn sample() -> App {
    App {
        id: "fixture".into(),
        name: "Sakura fixture".into(),
        publisher: "Sample team".into(),
        version: String::new(),
        install_date: None,
        size_kib: None,
        location: None,
        icon: None,
        command: None,
        quiet_command: None,
        msi: None,
        package: None,
        protected: false,
        portable: false,
        installer: String::new(),
        source_stamp: 0,
    }
}

#[test]
fn compares_inventory_quarantines_and_restores_without_overwriting() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let data = root.path().join("data");
    let location = Locations {
        data: vec![data.clone()],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let app = sample();
    let original = data.join(&app.name);
    fs::create_dir_all(&original)?;
    fs::write(original.join("preferences"), "fixture")?;

    let before = snapshot(&location, &app, std::slice::from_ref(&app), Profile::Safe);
    assert_eq!(before.len(), 1);
    assert!(remaining(&location, &before, std::slice::from_ref(&app)).is_empty());
    let remaining = remaining(&location, &before, &[]);
    assert_eq!(remaining.len(), 1);
    location.quarantine(&remaining[0], &[])?;
    assert!(!original.exists());
    let saved = location.saved()?;
    assert_eq!(saved.len(), 1);

    fs::create_dir(&original)?;
    fs::write(original.join("preferences"), "new")?;
    assert!(location.restore(saved[0].id).is_err());
    assert_eq!(fs::read_to_string(original.join("preferences"))?, "new");
    // Move our synthetic replacement aside to test a successful retry.
    fs::rename(&original, data.join("replacement"))?;
    location.restore(saved[0].id)?;
    assert_eq!(fs::read_to_string(original.join("preferences"))?, "fixture");
    assert!(location.saved()?.is_empty());
    Ok(())
}

#[test]
fn compresses_ntfs_payload_and_restores_original_attributes() -> io::Result<()> {
    use crate::{safety::compression, scanner::filesystem_scanner::identity};
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_COMPRESSED;

    let root = tempfile::tempdir()?;
    let location = Locations {
        data: vec![root.path().join("data")],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let app = sample();
    let original = location.data[0].join(&app.name);
    fs::create_dir_all(&original)?;
    let file = original.join("large.dat");
    let shared = root.path().join("shared.dat");
    let bytes = vec![b'S'; 256 * 1024];
    fs::write(&file, &bytes)?;
    fs::write(&shared, &bytes)?;
    let supported = compression::supported(&original)?;
    if supported {
        fs::hard_link(&shared, original.join("linked.dat"))?;
    }
    let compressed = |path: &std::path::Path| {
        fs::metadata(path).map(|info| info.file_attributes() & FILE_ATTRIBUTE_COMPRESSED != 0)
    };
    let original_flag = compressed(&file)?;
    let shared_flag = compressed(&shared)?;
    let file_id = identity(&file)?;
    let items = snapshot(&location, &app, &[], Profile::Safe);
    let result = location.quarantine(&items[0], &[])?;
    let saved = location.saved()?;
    let payload = location
        .quarantine_dir()?
        .join(saved[0].id.to_string())
        .join("payload");
    let archived = payload.join("large.dat");
    assert_eq!(
        result.compression.compressed,
        usize::from(supported && !original_flag)
    );
    assert_eq!(compressed(&archived)?, supported || original_flag);
    assert_eq!(compressed(&shared)?, shared_flag);
    assert_eq!(identity(&archived)?, file_id);
    assert_eq!(fs::read(&archived)?, bytes);

    fs::create_dir(&original)?;
    assert_eq!(
        location.restore(saved[0].id).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    assert_eq!(compressed(&archived)?, supported || original_flag);
    fs::remove_dir(&original)?;
    location.restore(saved[0].id)?;
    assert_eq!(compressed(&file)?, original_flag);
    assert_eq!(identity(&file)?, file_id);
    assert_eq!(fs::read(&file)?, bytes);
    assert!(location.saved()?.is_empty());
    Ok(())
}

#[test]
fn refuses_restore_when_payload_checksum_differs() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let location = Locations {
        data: vec![root.path().join("data")],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let app = sample();
    let original = location.data[0].join(&app.name);
    fs::create_dir_all(&original)?;
    fs::write(original.join("profile"), "initial")?;
    let items = snapshot(&location, &app, &[], Profile::Safe);
    location.quarantine(&items[0], &[])?;
    let saved = location.saved()?;
    let payload = location
        .quarantine_dir()?
        .join(saved[0].id.to_string())
        .join("payload");
    fs::write(payload.join("profile"), "changed")?;
    let error = location.restore(saved[0].id).unwrap_err();
    assert_eq!(error.to_string(), "Quarantine checksum mismatch");
    assert!(!original.exists());
    assert!(payload.exists());
    Ok(())
}

#[test]
fn sqlite_migrates_legacy_state_and_invalidates_stale_measurements() -> io::Result<()> {
    use crate::storage::cache::measurement_fingerprint;

    let root = tempfile::tempdir()?;
    let location = Locations {
        data: vec![],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let mut app = sample();
    let directory = root.path().join(&app.name);
    fs::create_dir(&directory)?;
    app.location = Some(directory);
    app.source_stamp = 42;
    let report = format!("report-{}.json", Uuid::new_v4());
    location.write_state("inventory.json", std::slice::from_ref(&app))?;
    location.write_state(&report, &["precedente", "previous"])?;
    assert_eq!(location.inventory_cache()[0].source_stamp, 0);
    assert_eq!(location.inventory_cache()[0].id, app.id);
    assert!(location.store.join(&report).is_file());
    location.save_report("nuovo", "new")?;
    let (latest, _, older) = location.read_report(0)?.unwrap();
    assert_eq!(latest.text(false), "new");
    assert!(older);
    let (earlier, _, older) = location.read_report(1)?.unwrap();
    assert_eq!(earlier.text(true), "precedente");
    assert!(!older);
    assert!(location.read_report(2)?.is_none());

    location.set_inventory_cache(std::slice::from_ref(&app))?;
    assert_eq!(location.inventory_cache(), vec![app.clone()]);
    location.save_measurement(&app, &measurement_fingerprint(&app)?, 1024)?;
    assert_eq!(
        location
            .measurements(std::slice::from_ref(&app))?
            .get(&app.id),
        Some(&1024)
    );
    app.version = "2".into();
    assert!(location.measurements(&[app])?.is_empty());
    Ok(())
}

#[test]
fn rejects_shared_and_system_locations() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let location = Locations {
        data: vec![root.path().to_owned()],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let app = sample();
    let original = root.path().join(&app.name);
    fs::create_dir(&original)?;
    let other = App {
        id: "another".into(),
        ..app.clone()
    };
    assert!(snapshot(&location, &app, &[app.clone(), other], Profile::Safe).is_empty());
    assert!(!location.allows(root.path(), Evidence::UserData));
    assert!(!location.allows(
        &root.path().join("Microsoft").join("child"),
        Evidence::UserData
    ));
    assert!(!location.allows(&root.path().join("store"), Evidence::UserData));
    Ok(())
}

#[test]
fn backs_up_and_recovers_registry_values_without_overwriting_new_keys() -> io::Result<()> {
    use windows_registry::CURRENT_USER;

    struct Fixture(String);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = CURRENT_USER.remove_tree(&self.0);
        }
    }
    let scratch = tempfile::tempdir()?;
    let location = Locations {
        data: Vec::new(),
        programs: Vec::new(),
        store: scratch.path().join("store"),
        rules: Rules::default(),
    };
    let mut app = sample();
    app.publisher = format!("Sakura Registry Test {}", Uuid::new_v4());
    let vendor = Fixture(format!(r"Software\{}", app.publisher));
    let key_path = format!(r"{}\{}", vendor.0, app.name);
    let key = CURRENT_USER.create(&key_path).map_err(io::Error::other)?;
    key.set_expand_string("Install", r"%LOCALAPPDATA%\Demo")
        .map_err(io::Error::other)?;
    key.set_u64("Setting", 0x1122_3344_5566_7788)
        .map_err(io::Error::other)?;
    let expected = key.get_value("Install").map_err(io::Error::other)?;
    drop(key);
    let items = snapshot(&location, &app, &[], Profile::Balanced);
    assert_eq!(items.len(), 1);
    assert!(items[0].writable());
    location.backup_before_uninstall(&items)?;
    assert!(location.saved()?.is_empty());
    location.quarantine(&items[0], &[])?;
    assert!(CURRENT_USER.open(&key_path).is_err());
    let saved = location.saved()?;
    assert_eq!(saved.len(), 1);
    let newer = CURRENT_USER.create(&key_path).map_err(io::Error::other)?;
    newer
        .set_string("Install", "replacement")
        .map_err(io::Error::other)?;
    assert!(location.restore(saved[0].id).is_err());
    assert_eq!(
        newer.get_string("Install").map_err(io::Error::other)?,
        "replacement"
    );
    drop(newer);
    CURRENT_USER
        .remove_tree(&key_path)
        .map_err(io::Error::other)?;
    location.restore(saved[0].id)?;
    let restored = CURRENT_USER.open(&key_path).map_err(io::Error::other)?;
    assert_eq!(
        restored.get_value("Install").map_err(io::Error::other)?,
        expected
    );
    assert_eq!(
        restored.get_u64("Setting").map_err(io::Error::other)?,
        0x1122_3344_5566_7788
    );
    assert!(location.saved()?.is_empty());
    Ok(())
}
