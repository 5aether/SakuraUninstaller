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
        self.package.is_some() || self.msi.is_some() || self.quiet_command.is_some()
    }

    pub fn search_text(&self) -> String {
        format!("{} {} {}", self.name, self.publisher, self.version).to_lowercase()
    }
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
        Err(error) => Some(error),
    };
    apps.sort_by_cached_key(|app| (app.name.to_lowercase(), app.id.clone()));
    Ok((apps, warning))
}
