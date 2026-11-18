use super::{BatchControl, ExitStatus, verify_removal};
use crate::scanner::installed_apps::App;
use std::{io, time::Duration};

fn sample() -> App {
    App {
        id: "fixture".into(),
        name: "Sakura fixture".into(),
        publisher: String::new(),
        version: String::new(),
        install_date: None,
        size_kib: None,
        location: None,
        icon: None,
        command: Some(r"C:\Fixture\uninstall.exe".into()),
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
fn interprets_msi_codes_only_for_msi_uninstallers() {
    for (code, expected) in [
        (0, ExitStatus::Completed),
        (1602, ExitStatus::Cancelled),
        (1614, ExitStatus::Completed),
        (1641, ExitStatus::RestartInitiated),
        (3010, ExitStatus::RestartRequired),
        (1618, ExitStatus::Failed(1618)),
        (1603, ExitStatus::Failed(1603)),
    ] {
        assert_eq!(ExitStatus::classify(code, true), expected);
        if code != 0 {
            assert_eq!(ExitStatus::classify(code, false), ExitStatus::Failed(code));
        }
    }
    assert_eq!(ExitStatus::classify(1223, false), ExitStatus::Cancelled);
}

#[test]
fn stop_requests_reach_the_worker_and_do_not_leak_into_new_batches() {
    let control = BatchControl::default();
    let worker = control.clone();
    std::thread::spawn(move || worker.request_stop())
        .join()
        .unwrap();
    assert!(control.stop_requested());
    assert!(!BatchControl::default().stop_requested());
}

#[test]
fn waits_for_delayed_registration_removal() -> io::Result<()> {
    let selected = sample();
    let mut calls = 0;
    let after = verify_removal(
        &selected,
        || {
            calls += 1;
            Ok((
                if calls < 3 {
                    vec![selected.clone()]
                } else {
                    vec![]
                },
                None,
            ))
        },
        5,
        Duration::ZERO,
    )?;
    assert!(after.is_some_and(|apps| apps.is_empty()));
    assert_eq!(calls, 3);
    Ok(())
}

#[test]
fn does_not_confirm_removal_when_the_registration_remains() -> io::Result<()> {
    let selected = sample();
    let mut calls = 0;
    assert!(
        verify_removal(
            &selected,
            || {
                calls += 1;
                Ok((vec![selected.clone()], None))
            },
            3,
            Duration::ZERO
        )?
        .is_none()
    );
    assert_eq!(calls, 3);
    Ok(())
}

#[test]
fn does_not_confirm_msix_removal_from_an_incomplete_inventory() {
    let mut selected = sample();
    selected.package = Some("Fixture_1.0_x64__test".into());
    let result = verify_removal(
        &selected,
        || {
            Ok((
                vec![],
                Some(io::Error::other("Package manager unavailable")),
            ))
        },
        1,
        Duration::ZERO,
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Cannot verify MSIX removal")
    );
}

#[test]
fn can_verify_win32_removal_when_only_msix_enumeration_fails() -> io::Result<()> {
    assert!(
        verify_removal(
            &sample(),
            || Ok((
                vec![],
                Some(io::Error::other("Package manager unavailable"))
            )),
            1,
            Duration::ZERO
        )?
        .is_some()
    );
    Ok(())
}

#[test]
fn propagates_registry_errors_instead_of_confirming_removal() {
    let result = verify_removal(
        &sample(),
        || {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Registry unavailable",
            ))
        },
        1,
        Duration::ZERO,
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn recognizes_the_same_msi_product_under_another_registry_entry() -> io::Result<()> {
    let mut selected = sample();
    selected.msi = Some("{053F961B-3743-4024-AFA7-FB952630EE6A}".into());
    let mut alias = selected.clone();
    alias.id = "other registry view".into();
    alias.msi = selected.msi.as_ref().map(|code| code.to_lowercase());
    assert!(
        verify_removal(
            &selected,
            || Ok((vec![alias.clone()], None)),
            1,
            Duration::ZERO
        )?
        .is_none()
    );
    Ok(())
}
