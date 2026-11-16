use super::{
    filesystem_scanner::{Identity, inspect_tree},
    installed_apps::App,
    registry_scanner::{self, Hive, MAX_VALUE_BYTES, RawValue, registry_key, vendor_key},
    service_scanner::{self, SERVICES, task_service, task_targets},
    startup_scanner::{self, RUN, RUN_ONCE, shortcut_allowed},
};
use crate::{
    storage::Locations,
    uninstall::{elevation::ComApartment, runner::split},
    utils::{
        hashing::file_hash,
        paths::{comparable, is_local_absolute, known_folder, regular_path},
        strings::{component, expand},
    },
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use windows_api::core::BSTR;
use windows_registry::LOCAL_MACHINE;
use windows_sys::Win32::System::Registry::REG_LINK;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Evidence {
    InstallLocation,
    UserData,
    WindowsIntegration,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub app: String,
    pub app_id: String,
    pub evidence: Evidence,
    pub bytes: u64,
    pub(crate) identity: Identity,
    #[serde(default)]
    pub integration: Option<Integration>,
    #[serde(default)]
    pub(crate) backup_digest: Option<String>,
}

impl Candidate {
    pub fn writable(&self) -> bool {
        self.integration.as_ref().map_or(
            self.evidence != Evidence::WindowsIntegration,
            Integration::writable,
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Integration {
    Registry {
        hive: Hive,
        key: String,
        values: Vec<RawValue>,
    },
    Startup {
        hive: Hive,
        key: String,
        value: RawValue,
        target: PathBuf,
    },
    Shortcut {
        path: PathBuf,
        target: PathBuf,
        digest: String,
    },
    Service {
        name: String,
        command: String,
        target: PathBuf,
    },
    Task {
        path: String,
        targets: Vec<PathBuf>,
    },
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Profile {
    Safe,
    #[default]
    Balanced,
    Deep,
}

impl Profile {
    pub fn from_index(index: i32) -> Self {
        match index {
            0 => Self::Safe,
            2 => Self::Deep,
            _ => Self::Balanced,
        }
    }

    pub fn index(self) -> i32 {
        match self {
            Self::Safe => 0,
            Self::Balanced => 1,
            Self::Deep => 2,
        }
    }
}

pub(crate) fn command_target(command: &str) -> Option<PathBuf> {
    let command = expand(command).ok()?;
    let (exe, _) = split(&command)?;
    let target = PathBuf::from(exe);
    is_local_absolute(&target).then_some(target)
}

pub(crate) fn owns(root: &Path, target: &Path) -> bool {
    let target = comparable(target);
    target != root
        && target.starts_with(root)
        && target.extension().is_some_and(|value| value == "exe")
}

pub fn associated_root(app: &App, installed: &[App]) -> Option<PathBuf> {
    let path = app.location.as_deref()?;
    let metadata = regular_path(path).ok()?;
    let name = component(&app.name)?;
    if !metadata.is_dir()
        || !path
            .file_name()?
            .to_string_lossy()
            .eq_ignore_ascii_case(name)
    {
        return None;
    }
    let root = comparable(path);
    let parent = root.parent()?;
    if parent.parent().is_none()
        || [
            "windows",
            "system32",
            "windowsapps",
            "common files",
            "programs",
            "program files",
            "program files (x86)",
            "microsoft",
        ]
        .contains(&root.file_name()?.to_str()?)
    {
        return None;
    }
    let windows = known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Windows).ok()?;
    if root.starts_with(comparable(&windows)) {
        return None;
    }
    (!installed
        .iter()
        .filter(|other| other.id != app.id)
        .filter_map(|other| other.location.as_deref())
        .map(comparable)
        .any(|other| root.starts_with(&other) || other.starts_with(&root)))
    .then_some(root)
}

fn hypotheses(locations: &Locations, app: &App) -> Vec<(PathBuf, Evidence)> {
    let Some(name) = component(&app.name) else {
        return Vec::new();
    };
    let publisher = component(&app.publisher);
    let mut paths = Vec::new();
    if !app.portable
        && let Some(path) = &app.location
        && path
            .file_name()
            .is_some_and(|leaf| leaf.to_string_lossy().to_lowercase() == name.to_lowercase())
    {
        paths.push((path.clone(), Evidence::InstallLocation));
    }
    for root in &locations.data {
        paths.push((root.join(name), Evidence::UserData));
        if let Some(publisher) = publisher {
            paths.push((root.join(publisher).join(name), Evidence::UserData));
        }
    }
    paths
}

fn overlaps(first: &Path, second: &Path) -> bool {
    first.starts_with(second) || second.starts_with(first)
}

fn shared(locations: &Locations, path: &Path, owner: &str, installed: &[App]) -> bool {
    let path = comparable(path);
    installed.iter().filter(|app| app.id != owner).any(|app| {
        app.location
            .as_ref()
            .is_some_and(|other| overlaps(&path, &comparable(other)))
            || hypotheses(locations, app)
                .iter()
                .any(|(other, _)| overlaps(&path, &comparable(other)))
    })
}

fn scan_integrations(app: &App, installed: &[App], profile: Profile) -> Vec<Integration> {
    if profile == Profile::Safe || app.package.is_some() {
        return Vec::new();
    }
    let mut findings = Vec::new();
    registry_scanner::scan_vendor(app, installed, profile, &mut findings);
    let Some(root) = associated_root(app, installed) else {
        return findings;
    };
    startup_scanner::scan(&root, profile, &mut findings);
    if profile == Profile::Deep {
        service_scanner::scan(&root, &mut findings);
    }
    findings.sort_by_cached_key(Integration::label);
    findings.dedup_by(|first, second| first.label() == second.label());
    findings
}

pub fn snapshot(
    locations: &Locations,
    app: &App,
    installed: &[App],
    profile: Profile,
) -> Vec<Candidate> {
    if app.package.is_some() {
        return Vec::new();
    }
    let mut approved: Vec<(PathBuf, Evidence)> = Vec::new();
    for (path, evidence) in hypotheses(locations, app) {
        if !locations.allows(&path, evidence)
            || (app.portable
                && app
                    .location
                    .as_ref()
                    .is_some_and(|original| overlaps(&comparable(original), &comparable(&path))))
            || shared(locations, &path, &app.id, installed)
            || approved
                .iter()
                .any(|(other, _)| overlaps(&comparable(other), &comparable(&path)))
        {
            continue;
        }
        approved.push((path, evidence));
    }
    let mut candidates: Vec<_> = approved
        .into_par_iter()
        .filter_map(|(path, evidence)| {
            let (identity, bytes) = inspect_tree(&path).ok()?;
            Some(Candidate {
                path,
                app: app.name.clone(),
                app_id: app.id.clone(),
                evidence,
                bytes,
                identity,
                integration: None,
                backup_digest: None,
            })
        })
        .collect();
    candidates.extend(
        scan_integrations(app, installed, profile)
            .into_iter()
            .filter(|item| !locations.rules.protects_integration(&item.label()))
            .map(|item| Candidate {
                path: PathBuf::from(item.label()),
                app: app.name.clone(),
                app_id: app.id.clone(),
                evidence: Evidence::WindowsIntegration,
                bytes: 0,
                identity: Identity::default(),
                integration: Some(item),
                backup_digest: None,
            }),
    );
    candidates
}

pub fn remaining(locations: &Locations, before: &[Candidate], installed: &[App]) -> Vec<Candidate> {
    before
        .iter()
        .filter_map(|item| {
            if installed.iter().any(|app| app.id == item.app_id) {
                return None;
            }
            if let Some(integration) = &item.integration {
                return (integration.label() == item.path.to_string_lossy()
                    && !locations.rules.protects_integration(&integration.label())
                    && !integration.shared_with(installed, &item.app_id)
                    && integration.present())
                .then(|| item.clone());
            }
            if shared(locations, &item.path, &item.app_id, installed)
                || !locations.allows(&item.path, item.evidence)
            {
                return None;
            }
            let (identity, bytes) = inspect_tree(&item.path).ok()?;
            (identity == item.identity).then(|| Candidate {
                bytes,
                ..item.clone()
            })
        })
        .collect()
}

impl Integration {
    pub fn shared_with(&self, installed: &[App], owner: &str) -> bool {
        installed
            .iter()
            .filter(|app| app.id != owner)
            .any(|app| match self {
                Self::Registry { key, .. } => {
                    format!(r"Software\{}\{}", app.publisher, app.name).eq_ignore_ascii_case(key)
                }
                Self::Startup { target, .. }
                | Self::Shortcut { target, .. }
                | Self::Service { target, .. } => app
                    .location
                    .as_ref()
                    .is_some_and(|location| owns(&comparable(location), target)),
                Self::Task { targets, .. } => app.location.as_ref().is_some_and(|location| {
                    targets
                        .iter()
                        .any(|target| owns(&comparable(location), target))
                }),
            })
    }

    pub fn label(&self) -> String {
        match self {
            Self::Registry { hive, key, .. } => format!(r"{}\{key}", hive.label()),
            Self::Startup {
                hive, key, value, ..
            } => format!(r"{}\{key}\{}", hive.label(), value.name),
            Self::Shortcut { path, .. } => path.to_string_lossy().into_owned(),
            Self::Service { name, .. } => format!(r"Service\{name}"),
            Self::Task { path, .. } => format!("Task{path}"),
        }
    }

    pub fn writable(&self) -> bool {
        match self {
            Self::Registry {
                hive: Hive::User,
                key,
                values,
            } => {
                vendor_key(key)
                    && values.len() <= 128
                    && values
                        .iter()
                        .map(|value| value.name.len() + value.data.len())
                        .sum::<usize>()
                        <= MAX_VALUE_BYTES
                    && values
                        .iter()
                        .all(|value| value.kind != REG_LINK && !value.name.contains('\0'))
            }
            Self::Startup {
                hive: Hive::User,
                key,
                value,
                ..
            } => {
                (key == RUN || key == RUN_ONCE)
                    && value.data.len() <= MAX_VALUE_BYTES
                    && !value.name.contains('\0')
            }
            Self::Shortcut { path, .. } => shortcut_allowed(path),
            _ => false,
        }
    }

    pub fn present(&self) -> bool {
        match self {
            Self::Registry { hive, key, .. } => registry_key(*hive, key).as_ref() == Some(self),
            Self::Startup {
                hive, key, value, ..
            } => hive
                .key(key)
                .ok()
                .and_then(|key| key.get_value(&value.name).ok())
                .is_some_and(|data| RawValue::new(value.name.clone(), data) == *value),
            Self::Shortcut { path, digest, .. } => {
                regular_path(path).is_ok() && file_hash(path).is_ok_and(|hash| &hash == digest)
            }
            Self::Service { name, command, .. } => {
                LOCAL_MACHINE
                    .open(SERVICES)
                    .ok()
                    .and_then(|key| key.open(name).ok())
                    .and_then(|key| key.get_string("ImagePath").ok())
                    .as_ref()
                    == Some(command)
            }
            Self::Task { path, targets } => {
                let Ok(_apartment) = ComApartment::open() else {
                    return false;
                };
                let Some((directory, name)) = path.rsplit_once('\\') else {
                    return false;
                };
                let directory = if directory.is_empty() {
                    r"\"
                } else {
                    directory
                };
                task_service()
                    .and_then(|service| unsafe {
                        let folder = service.GetFolder(&BSTR::from(directory))?;
                        task_targets(&folder.GetTask(&BSTR::from(name))?)
                    })
                    .is_ok_and(|current| current == *targets)
            }
        }
    }
}
