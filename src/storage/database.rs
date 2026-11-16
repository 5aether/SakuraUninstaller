use super::migrations::{decode_json, migrate};
use crate::{
    safety::rules::Rules,
    scanner::leftovers::Profile,
    utils::paths::{known_folder, regular_path},
};
use rusqlite::Connection;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
use uuid::Uuid;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX64,
    FOLDERID_ProgramFilesX86, FOLDERID_RoamingAppData,
};

#[derive(Clone)]
pub struct Locations {
    pub(crate) data: Vec<PathBuf>,
    pub(crate) programs: Vec<PathBuf>,
    pub(crate) store: PathBuf,
    pub(crate) rules: Rules,
}

impl Locations {
    pub fn discover() -> io::Result<Self> {
        let local = known_folder(&FOLDERID_LocalAppData)?;
        let store = local.join("SakuraUninstaller");
        let rules_path = store.join("rules.json");
        let rules = match Rules::load(&rules_path) {
            Ok(rules) => rules,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Rules::default(),
            Err(_) => Rules::default(),
        };
        Ok(Self {
            data: [
                Some(local.clone()),
                known_folder(&FOLDERID_RoamingAppData).ok(),
            ]
            .into_iter()
            .flatten()
            .collect(),
            programs: [
                known_folder(&FOLDERID_ProgramFiles).ok(),
                known_folder(&FOLDERID_ProgramFilesX86).ok(),
                known_folder(&FOLDERID_ProgramFilesX64).ok(),
            ]
            .into_iter()
            .flatten()
            .collect(),
            store,
            rules,
        })
    }

    pub(crate) fn root(&self) -> io::Result<&Path> {
        fs::create_dir_all(&self.store)?;
        if !regular_path(&self.store)?.is_dir() {
            return Err(io::Error::other("Invalid application data directory"));
        }
        Ok(&self.store)
    }

    pub fn data_folder(&self) -> io::Result<PathBuf> {
        Ok(self.root()?.to_owned())
    }

    pub(crate) fn open_database(&self) -> io::Result<Connection> {
        let root = self.root()?;
        for name in [
            "sakura.sqlite3",
            "sakura.sqlite3-journal",
            "sakura.sqlite3-wal",
            "sakura.sqlite3-shm",
        ] {
            match regular_path(&root.join(name)) {
                Ok(metadata) if metadata.is_file() => (),
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                _ => return Err(io::Error::other("Invalid Sakura database path")),
            }
        }
        let mut connection =
            Connection::open(root.join("sakura.sqlite3")).map_err(io::Error::other)?;
        connection
            .busy_timeout(Duration::from_secs(2))
            .map_err(io::Error::other)?;
        connection
            .pragma_update(None, "trusted_schema", false)
            .map_err(io::Error::other)?;
        migrate(&mut connection, self)?;
        Ok(connection)
    }

    pub fn lock(&self) -> io::Result<File> {
        let path = self.root()?.join("in-use.lock");
        match regular_path(&path) {
            Ok(metadata) if metadata.is_file() => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            _ => return Err(io::Error::other("Invalid application lock")),
        }
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(path)
    }

    pub(crate) fn read_state<T: serde::de::DeserializeOwned>(
        &self,
        filename: &str,
    ) -> io::Result<Option<T>> {
        let path = self.store.join(filename);
        let metadata = match regular_path(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() || metadata.len() > 8 * 1024 * 1024 {
            return Err(io::Error::other("Invalid Sakura state file"));
        }
        decode_json(File::open(path)?)
    }

    pub(crate) fn write_state<T: Serialize + ?Sized>(
        &self,
        filename: &str,
        value: &T,
    ) -> io::Result<()> {
        let root = self.root()?;
        let temp = root.join(format!("{}.tmp", Uuid::new_v4()));
        let mut file = File::create_new(&temp)?;
        let result = serde_json::to_writer(&mut file, value)
            .map_err(io::Error::other)
            .and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| fs::rename(&temp, root.join(filename)));
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }

    pub fn profile(&self) -> Profile {
        self.read_state("profile.json")
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    pub fn set_profile(&self, profile: Profile) -> io::Result<()> {
        self.write_state("profile.json", &profile)
    }

    pub fn auto_update(&self) -> bool {
        self.read_state("auto-update.json")
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    pub fn set_auto_update(&self, enabled: bool) -> io::Result<()> {
        self.write_state("auto-update.json", &enabled)
    }

    pub fn diagnostics(&self) -> bool {
        self.read_state("diagnostics.json")
            .ok()
            .flatten()
            .unwrap_or(false)
    }

    pub fn set_diagnostics(&self, enabled: bool) -> io::Result<()> {
        self.write_state("diagnostics.json", &enabled)
    }

    pub fn measure_event(&self, ok: bool) -> io::Result<()> {
        if self.diagnostics() {
            let mut counts = self.diagnostic_counts()?;
            let index = usize::from(!ok);
            counts[index] = counts[index].saturating_add(1);
            self.write_state("diagnostic-counts.json", &counts)?;
        }
        Ok(())
    }

    pub fn diagnostic_counts(&self) -> io::Result<[u64; 2]> {
        Ok(self
            .read_state("diagnostic-counts.json")?
            .unwrap_or_default())
    }

    pub fn language(&self) -> Option<bool> {
        self.read_state("italian.json").ok().flatten()
    }

    pub fn set_language(&self, italian: bool) -> io::Result<()> {
        self.write_state("italian.json", &italian)
    }
}
