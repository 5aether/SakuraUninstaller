use super::registry_scanner::registered;
use crate::uninstall::msix;
use serde::{Deserialize, Serialize};
use std::{io, path::PathBuf};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub version: String,
    #[serde(default)]
    pub install_date: Option<String>,
    pub size_kib: Option<u64>,
    pub location: Option<PathBuf>,
    #[serde(default)]
    pub icon: Option<PathBuf>,
    pub command: Option<String>,
    pub quiet_command: Option<String>,
    pub msi: Option<String>,
    pub package: Option<String>,
    pub protected: bool,
    #[serde(default)]
    pub portable: bool,
    #[serde(default)]
    pub installer: String,
    #[serde(default)]
    pub source_stamp: u64,
}

impl App {
    pub fn removable(&self) -> bool {
        !self.protected && (self.package.is_some() || self.msi.is_some() || self.command.is_some())
    }

    pub fn selectable(&self) -> bool {
        self.portable || self.removable()
    }

    pub fn quiet_supported(&self) -> bool {
        self.removable()
            && (self.package.is_some()
                || self.msi.is_some()
                || self.quiet_command.is_some()
                || matches!(self.installer.as_str(), "Inno Setup" | "NSIS"))
    }

    pub fn search_text(&self) -> String {
        format!("{} {} {}", self.name, self.publisher, self.version).to_lowercase()
    }

    /// File whose icon represents the app: the one recorded by the installer, or the
    /// launcher we can still find next to the install folder / uninstall command.
    pub fn icon_file(&self) -> Option<PathBuf> {
        self.icon.clone()
    }

    /// Launcher to use when the installer did not record an icon.
    ///
    /// The app's own executable wins over the uninstall command, which often points at an
    /// updater (Discord's `Update.exe`) whose icon is not the app icon.
    fn derived_icon(&self) -> Option<PathBuf> {
        self.location
            .as_deref()
            .and_then(|folder| executable(folder, &self.name))
            .or_else(|| launcher(self.command.as_deref()))
            .or_else(|| launcher(self.quiet_command.as_deref()))
    }
}

/// Helper executables that are never the app the user recognises.
const NOT_THE_APP: [&str; 7] = [
    "unins",
    "uninstall",
    "setup",
    "update",
    "helper",
    "crash",
    "squirrel",
];

/// How close an executable name is to the app name: exact match beats a partial one.
fn name_score(file: &str, app: &str) -> u8 {
    let clean = |value: &str| -> String {
        value
            .chars()
            .filter(|letter| letter.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let (file, app) = (clean(file), clean(app));
    if file.is_empty() || app.is_empty() {
        0
    } else if file == app {
        2
    } else if file.contains(&app) || app.contains(&file) {
        1
    } else {
        0
    }
}

/// Executable referenced by an uninstall command such as `"C:\\…\\rustup.exe" self uninstall`.
fn launcher(command: Option<&str>) -> Option<PathBuf> {
    let command = command?.trim();
    let path = if let Some(rest) = command.strip_prefix('"') {
        rest.split_once('"').map(|(path, _)| path)?
    } else {
        command.split_whitespace().next()?
    };
    let path = PathBuf::from(path);
    let executable = path
        .extension()
        .map(|value| value.to_string_lossy().eq_ignore_ascii_case("exe"))
        .unwrap_or(false);
    if !executable || !path.is_absolute() || !path.is_file() {
        return None;
    }
    let name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if name == "msiexec.exe" || NOT_THE_APP.iter().any(|skip| name.contains(skip)) {
        return None;
    }
    Some(path)
}

/// Launcher of an install folder: the executable that shares the app name wins over the
/// largest one, and helpers/uninstallers are ignored.
fn executable(folder: &std::path::Path, app: &str) -> Option<PathBuf> {
    let mut best: Option<(u8, u64, PathBuf)> = None;
    let mut pending = vec![(folder.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((folder, depth)) = pending.pop() {
        if depth > 2 || visited > 200 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for item in entries.flatten() {
            visited += 1;
            let path = item.path();
            if path.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            let is_executable = path
                .extension()
                .map(|value| value.to_string_lossy().eq_ignore_ascii_case("exe"))
                .unwrap_or(false);
            if !is_executable {
                continue;
            }
            let file = path
                .file_stem()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if NOT_THE_APP.iter().any(|skip| file.contains(skip)) {
                continue;
            }
            let score = name_score(&file, app);
            let size = item.metadata().map(|meta| meta.len()).unwrap_or(0);
            if best.as_ref().is_none_or(|(known_score, known_size, _)| {
                (score, size) > (*known_score, *known_size)
            }) {
                best = Some((score, size, path));
            }
        }
    }
    best.map(|(_, _, path)| path)
}

pub fn scan() -> io::Result<Vec<App>> {
    full().map(|(apps, _)| apps)
}

pub fn full() -> io::Result<(Vec<App>, Option<io::Error>)> {
    full_cached(&[])
}

pub fn full_cached(previous: &[App]) -> io::Result<(Vec<App>, Option<io::Error>)> {
    let mut apps = registered(previous)?;
    let warning = match msix::list() {
        Ok(packages) => {
            apps.extend(packages);
            None
        }
        Err(error) => {
            // A transient package error must not erase MSIX entries from the cache.
            apps.extend(previous.iter().filter(|app| app.package.is_some()).cloned());
            Some(error)
        }
    };
    for app in apps.iter_mut().filter(|app| app.icon.is_none()) {
        app.icon = app.derived_icon();
    }
    apps.sort_by_cached_key(|app| (app.name.to_lowercase(), app.id.clone()));
    Ok((apps, warning))
}
