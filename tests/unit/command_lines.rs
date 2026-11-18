use super::{
    BatchControl, Launch, Outcome, UninstallOptions, current, elevation, is_local_absolute,
    prepare, quarantine, split, uninstall,
};
use crate::{
    safety::rules::Rules,
    scanner::{
        installed_apps::App,
        leftovers::{self, Candidate, Profile},
    },
    storage::Locations,
};
use std::{io, path::Path};

#[test]
fn finds_this_process_by_its_executable_directory() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let directory = executable.parent().unwrap();
    assert!(
        crate::uninstall::processes::running_under(Some(directory)).contains(&std::process::id())
    );
    let unrelated = tempfile::tempdir()?;
    assert!(
        !crate::uninstall::processes::running_under(Some(unrelated.path()))
            .contains(&std::process::id())
    );
    Ok(())
}

#[test]
fn preserves_installer_arguments_and_does_not_treat_the_path_as_a_command() {
    assert_eq!(
        split(r#""C:\Program Files\Sakura\uninstall.exe" /x "a b""#),
        Some((r"C:\Program Files\Sakura\uninstall.exe", r#"/x "a b""#))
    );
    assert_eq!(
        split(r"C:\Program Files\Sakura\uninstall.exe /S"),
        Some((r"C:\Program Files\Sakura\uninstall.exe", "/S"))
    );
    assert_eq!(split(r#""C:\uninstall.exe"/x"#), None);
    assert!(!is_local_absolute(Path::new(r"\\server\share\remove.exe")));
    assert!(!is_local_absolute(Path::new(r"C:\safe\..\Windows")));
    assert!(!is_local_absolute(Path::new(r"C:\safe\file:ads")));
}

#[test]
fn waits_for_a_real_process_without_uninstalling_software() -> io::Result<()> {
    let launch = Launch {
        executable: std::env::current_exe()?,
        arguments: "--list".into(),
    };
    assert_eq!(elevation::run(&launch)?, 0);
    Ok(())
}

fn launchable_app() -> io::Result<App> {
    Ok(App {
        id: "fixture".into(),
        name: "Sakura fixture".into(),
        publisher: String::new(),
        version: String::new(),
        install_date: None,
        size_kib: None,
        location: None,
        icon: None,
        command: Some(format!(
            "\"{}\" /original",
            std::env::current_exe()?.display()
        )),
        quiet_command: None,
        msi: None,
        package: None,
        protected: false,
        portable: false,
        installer: String::new(),
        source_stamp: 0,
    })
}

#[test]
fn derives_inno_quiet_flags_only_when_identified_and_preserves_original_arguments() -> io::Result<()>
{
    let mut app = launchable_app()?;
    assert!(!app.quiet_supported());
    assert!(prepare(&app, true).is_err());
    app.installer = "Inno Setup".into();
    assert!(app.quiet_supported());
    assert_eq!(prepare(&app, false)?.arguments, "/original");
    assert_eq!(
        prepare(&app, true)?.arguments,
        "/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /original"
    );
    Ok(())
}

#[test]
fn preserves_nsis_final_directory_argument_when_adding_silent_flag() -> io::Result<()> {
    let mut app = launchable_app()?;
    app.installer = "NSIS".into();
    app.command = Some(format!(
        "\"{}\" _?=C:\\Program Files\\Fixture",
        std::env::current_exe()?.display()
    ));
    assert_eq!(
        prepare(&app, true)?.arguments,
        "/S _?=C:\\Program Files\\Fixture"
    );
    Ok(())
}

#[test]
fn registered_quiet_command_takes_priority_over_derived_switches() -> io::Result<()> {
    let mut app = launchable_app()?;
    app.installer = "Inno Setup".into();
    app.quiet_command = Some(format!(
        "\"{}\" /vendor-silent \"a b\"",
        std::env::current_exe()?.display()
    ));
    assert_eq!(prepare(&app, true)?.arguments, "/vendor-silent \"a b\"");
    Ok(())
}

#[test]
fn rejects_nul_commands_before_environment_expansion() -> io::Result<()> {
    let mut app = launchable_app()?;
    app.command
        .as_mut()
        .unwrap()
        .push_str("\0 hidden arguments");
    assert!(split(app.command.as_ref().unwrap()).is_none());
    assert!(prepare(&app, false).is_err());
    Ok(())
}

#[test]
fn rejects_invalid_msi_codes_instead_of_falling_back_to_an_exe() -> io::Result<()> {
    let mut app = launchable_app()?;
    app.msi = Some("{invalid} /extra".into());
    assert_eq!(
        prepare(&app, false).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    Ok(())
}

#[test]
fn requires_review_if_the_installation_directory_or_installer_kind_changed() -> io::Result<()> {
    let selected = launchable_app()?;
    let mut fresh = selected.clone();
    fresh.location = Some(r"C:\Other application".into());
    assert!(current(&[fresh], &selected).is_err());
    let mut fresh = selected.clone();
    fresh.installer = "NSIS".into();
    assert!(current(&[fresh], &selected).is_err());
    Ok(())
}

#[test]
fn never_prepares_a_protected_app_even_with_a_silent_mode() -> io::Result<()> {
    let mut app = launchable_app()?;
    app.installer = "Inno Setup".into();
    app.protected = true;
    assert!(!app.quiet_supported());
    assert_eq!(
        prepare(&app, true).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
    Ok(())
}

#[test]
fn stopping_before_a_batch_does_not_scan_or_launch_any_uninstaller() -> io::Result<()> {
    let root = tempfile::tempdir()?;
    let locations = Locations {
        data: vec![],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let control = BatchControl::default();
    control.request_stop();
    let mut outcome = Outcome::default();
    let mut events = 0;
    uninstall(
        &locations,
        &[launchable_app()?, launchable_app()?],
        UninstallOptions {
            quiet: false,
            profile: Profile::Safe,
            review_leftovers: true,
        },
        &mut vec![],
        &mut outcome,
        &control,
        &mut |_| events += 1,
    )?;
    assert!(outcome.cancelled && !outcome.failed);
    assert_eq!(events, 0);
    assert!(
        outcome
            .report
            .iter()
            .any(|line| line.text(false) == "Programs not started: 2.")
    );
    outcome.finish_status();
    assert!(
        outcome
            .status
            .unwrap()
            .text(false)
            .contains("Queue stopped")
    );
    Ok(())
}

#[test]
fn reports_restart_and_cancellation_without_claiming_success() {
    let mut outcome = Outcome {
        restart_required: true,
        ..Outcome::default()
    };
    outcome.finish_status();
    assert!(
        outcome
            .status
            .as_ref()
            .unwrap()
            .text(false)
            .contains("restart")
    );
    outcome.cancelled = true;
    outcome.finish_status();
    let text = outcome.status.as_ref().unwrap().text(false);
    assert!(text.contains("Queue stopped") && text.contains("restart"));
    outcome.failed = true;
    outcome.finish_status();
    let text = outcome.status.as_ref().unwrap().text(false);
    assert!(text.contains("attention") && text.contains("restart"));
}

fn cleanup_fixture() -> io::Result<(tempfile::TempDir, Locations, Vec<Candidate>)> {
    let root = tempfile::tempdir()?;
    let data = root.path().join("data");
    let locations = Locations {
        data: vec![data.clone()],
        programs: vec![],
        store: root.path().join("store"),
        rules: Rules::default(),
    };
    let mut items = Vec::new();
    for name in ["Fixture One", "Fixture Two"] {
        let mut app = launchable_app()?;
        app.id = uuid::Uuid::new_v4().to_string();
        app.name = name.into();
        let folder = data.join(name);
        std::fs::create_dir_all(&folder)?;
        std::fs::write(folder.join("preferences"), "fixture")?;
        items.extend(leftovers::snapshot(&locations, &app, &[], Profile::Safe));
    }
    assert_eq!(items.len(), 2);
    Ok((root, locations, items))
}

#[test]
fn cleanup_reports_each_item_and_stops_between_items_without_losing_backups() -> io::Result<()> {
    use super::ItemState;
    let (_root, locations, items) = cleanup_fixture()?;
    std::fs::write(
        items[0].path.join("preferences"),
        "verified current contents",
    )?;
    let control = BatchControl::default();
    let mut outcome = Outcome::default();
    let mut events = Vec::new();
    quarantine(
        &locations,
        &items,
        &mut outcome,
        &control,
        &mut |progress| {
            if let Some((_, state)) = progress.item {
                events.push(state);
                if state == ItemState::Quarantined {
                    control.request_stop();
                }
            }
        },
        || Ok(vec![]),
    );
    assert_eq!(events, [ItemState::Cleaning, ItemState::Quarantined]);
    assert!(outcome.cancelled && !outcome.failed);
    assert_eq!(outcome.quarantined.len(), 1);
    assert_eq!(outcome.quarantined[0].bytes, 25);
    assert!(!items[0].path.exists());
    assert!(items[1].path.exists());
    let backups = locations.saved()?;
    assert_eq!(backups.len(), 1);
    locations.restore(backups[0].id)?;
    assert_eq!(
        std::fs::read_to_string(items[0].path.join("preferences"))?,
        "verified current contents"
    );
    Ok(())
}

#[test]
fn cleanup_errors_never_increase_the_quarantined_count() -> io::Result<()> {
    use super::ItemState;
    let (_root, locations, items) = cleanup_fixture()?;
    let mut outcome = Outcome::default();
    let mut states = Vec::new();
    quarantine(
        &locations,
        &items,
        &mut outcome,
        &BatchControl::default(),
        &mut |progress| {
            if let Some((_, state)) = progress.item {
                states.push(state);
            }
        },
        || {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Fixture inventory error",
            ))
        },
    );
    assert!(outcome.failed);
    assert!(outcome.quarantined.is_empty());
    assert_eq!(
        states,
        [
            ItemState::Cleaning,
            ItemState::Failed,
            ItemState::Cleaning,
            ItemState::Failed
        ]
    );
    assert!(items.iter().all(|item| item.path.exists()));
    assert!(locations.saved()?.is_empty());
    Ok(())
}
