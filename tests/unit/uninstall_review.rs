use super::*;
use crate::scanner::{
    filesystem_scanner::Identity,
    leftovers::Evidence,
    registry_scanner::{Hive, RawValue},
};
use std::path::PathBuf;

fn app(id: &str) -> App {
    App {
        id: id.into(),
        name: "Fixture".into(),
        publisher: String::new(),
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
        portable: true,
        installer: String::new(),
        source_stamp: 0,
    }
}

fn file(owner: &str, path: &str, bytes: u64) -> Candidate {
    Candidate {
        path: PathBuf::from(path),
        app: "Fixture".into(),
        app_id: owner.into(),
        evidence: Evidence::UserData,
        bytes,
        identity: Identity::default(),
        integration: None,
        backup_digest: None,
    }
}

fn registry(owner: &str) -> Candidate {
    Candidate {
        integration: Some(Integration::Registry {
            hive: Hive::User,
            key: r"Software\Fixture Studio\Fixture".into(),
            values: vec![],
        }),
        evidence: Evidence::WindowsIntegration,
        ..file(owner, r"HKCU\Software\Fixture Studio\Fixture", 0)
    }
}

fn service(owner: &str) -> Candidate {
    Candidate {
        integration: Some(Integration::Service {
            name: "Fixture".into(),
            command: r"C:\Fixture\app.exe".into(),
            target: r"C:\Fixture\app.exe".into(),
        }),
        evidence: Evidence::WindowsIntegration,
        ..file(owner, "Fixture service", 0)
    }
}

#[test]
fn separates_registry_startup_files_shortcuts_and_read_only_integrations() {
    let mut startup = registry("a");
    startup.integration = Some(Integration::Startup {
        hive: Hive::User,
        key: "Software\\Run".into(),
        value: RawValue {
            name: "Fixture".into(),
            kind: 1,
            data: vec![],
        },
        target: r"C:\Fixture\app.exe".into(),
    });
    let mut shortcut = file("a", r"C:\Fixture.lnk", 40);
    shortcut.integration = Some(Integration::Shortcut {
        path: shortcut.path.clone(),
        target: r"C:\Fixture\app.exe".into(),
        digest: String::new(),
    });
    assert_eq!(category(&registry("a")), Category::Registry);
    assert_eq!(category(&startup), Category::Registry);
    assert_eq!(
        category(&file("a", r"C:\Data\Fixture", 10)),
        Category::Files
    );
    assert_eq!(category(&shortcut), Category::Files);
    assert_eq!(category(&service("a")), Category::Other);
}

#[test]
fn shows_only_the_current_batch_without_selecting_leftovers_automatically() {
    let mut session = ReviewSession::new(&[app("a"), app("b")]);
    let a = file("a", r"C:\Data\shared-name", 10);
    let b = file("b", r"C:\Data\shared-name", 20);
    let unrelated = file("other", r"C:\Data\other", 90);
    session.finish_uninstall(
        &[a.clone(), a, b, unrelated],
        &["a".into(), "a".into(), "other".into()],
        false,
        false,
        false,
    );
    assert_eq!(session.items.len(), 2);
    assert_eq!(session.removed, 1);
    assert_eq!(session.phase, Phase::Review);
    assert!(session.selected_candidates().is_empty());
    assert_ne!(
        item_key(&session.items[0].candidate),
        item_key(&session.items[1].candidate)
    );
}

#[test]
fn category_selection_never_selects_read_only_items_or_other_categories() {
    let mut session = ReviewSession::new(&[app("a")]);
    let mut protected_registry = registry("a");
    protected_registry.path = r"HKLM\Software\Fixture Studio\Fixture".into();
    if let Some(Integration::Registry { hive, .. }) = &mut protected_registry.integration {
        *hive = Hive::Machine64;
    }
    session.finish_uninstall(
        &[
            registry("a"),
            file("a", "folder", 100),
            protected_registry,
            service("a"),
        ],
        &[],
        false,
        false,
        false,
    );
    session.select_category(Category::Registry as i32, true);
    let selected = session.selected_candidates();
    assert_eq!(selected.len(), 1);
    assert!(selected[0].writable());
    assert_eq!(category(&selected[0]), Category::Registry);
    for item in session
        .items
        .iter()
        .filter(|item| !item.selectable())
        .map(|item| item_key(&item.candidate))
        .collect::<Vec<_>>()
    {
        session.select(&item, true);
    }
    assert_eq!(session.selected_candidates().len(), 1);
    session.select_category(Category::Registry as i32, false);
    assert!(session.selected_candidates().is_empty());
}

#[test]
fn counts_only_successful_cleanup_and_uses_verified_receipt_sizes() {
    let mut session = ReviewSession::new(&[app("a")]);
    let first = file("a", "one", 10);
    let second = file("a", "two", 20);
    session.finish_uninstall(
        &[first.clone(), second.clone(), registry("a")],
        &["a".into()],
        false,
        false,
        false,
    );
    session.select_category(Category::Files as i32, true);
    assert_eq!(session.totals(|item| item.selected).bytes, 30);
    session.begin_cleanup();
    session.update(&Progress::for_item(
        Message::new("failed", "failed"),
        &second,
        ItemState::Failed,
    ));
    let receipt = Candidate {
        bytes: 150,
        ..first.clone()
    };
    session.finish_cleanup(&[receipt], true, false);
    let cleaned = session.totals(|item| item.state == ItemState::Quarantined);
    assert_eq!(
        (cleaned.files, cleaned.registry, cleaned.bytes),
        (1, 0, 150)
    );
    assert_eq!(session.phase, Phase::Summary);
    assert!(session.failed);
    assert!(
        !session
            .items
            .iter()
            .find(|item| item.candidate.path == first.path)
            .unwrap()
            .selected
    );
    session.phase = Phase::Review;
    session.select(&item_key(&first), true);
    assert_eq!(session.selected_candidates().len(), 1);
    assert_eq!(session.selected_candidates()[0].path, second.path);
}

#[test]
fn preserves_unprocessed_items_when_cleanup_is_stopped() {
    let mut session = ReviewSession::new(&[app("a")]);
    let item = file("a", "pending", 15);
    session.finish_uninstall(std::slice::from_ref(&item), &[], false, false, false);
    session.select(&item_key(&item), true);
    session.begin_cleanup();
    session.update(&Progress::for_item(
        Message::new("working", "working"),
        &item,
        ItemState::Cleaning,
    ));
    session.finish_cleanup(&[], false, true);
    assert_eq!(
        session
            .totals(|item| item.state == ItemState::Quarantined)
            .count(),
        0
    );
    assert_eq!(session.items[0].state, ItemState::Pending);
    assert!(session.cancelled && session.items[0].selectable());
}

#[test]
fn cannot_change_selection_while_uninstalling_cleaning_or_showing_the_summary() {
    let mut session = ReviewSession::new(&[app("a")]);
    let item = file("a", "pending", 15);
    session.finish_uninstall(std::slice::from_ref(&item), &[], false, false, false);
    for phase in [Phase::Uninstalling, Phase::Cleaning, Phase::Summary] {
        session.phase = phase;
        session.select(&item_key(&item), true);
        session.select_category(Category::Files as i32, true);
        assert!(session.selected_candidates().is_empty());
    }
}

#[test]
fn an_empty_or_cancelled_operation_does_not_claim_programs_were_removed() {
    let mut session = ReviewSession::new(&[app("a")]);
    session.finish_uninstall(&[], &[], false, true, true);
    assert_eq!(session.phase, Phase::Summary);
    assert_eq!(session.removed, 0);
    assert!(session.cancelled && session.restart_required);
}
