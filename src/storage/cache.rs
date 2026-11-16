use super::{Locations, migrations::unix_seconds};
use crate::scanner::{filesystem_scanner::identity, installed_apps::App, leftovers::Candidate};
use crate::utils::{hashing::sha256, paths::regular_path};
use rusqlite::{OptionalExtension, params};
use std::{collections::HashMap, io, os::windows::fs::MetadataExt, time::SystemTime};

impl Locations {
    pub fn reviews(&self) -> io::Result<Vec<Candidate>> {
        Ok(self.read_state("reviews.json")?.unwrap_or_default())
    }

    pub fn save_reviews(&self, candidates: &[Candidate]) -> io::Result<()> {
        self.write_state("reviews.json", candidates)
    }

    pub fn inventory_cache(&self) -> Vec<App> {
        self.load_inventory().unwrap_or_else(|_| {
            self.read_state("inventory.json")
                .ok()
                .flatten()
                .unwrap_or_default()
        })
    }

    fn load_inventory(&self) -> io::Result<Vec<App>> {
        let db = self.open_database()?;
        let snapshot: Option<String> = db
            .query_row("SELECT snapshot FROM inventory WHERE id = 1", [], |row| {
                row.get(0)
            })
            .optional()
            .map_err(io::Error::other)?;
        snapshot
            .map(|json| serde_json::from_str(&json).map_err(io::Error::other))
            .transpose()
            .map(Option::unwrap_or_default)
    }

    pub fn set_inventory_cache(&self, apps: &[App]) -> io::Result<()> {
        let db = self.open_database()?;
        db.execute(
            "INSERT INTO inventory VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET snapshot = excluded.snapshot, scanned = excluded.scanned",
            params![serde_json::to_string(apps).map_err(io::Error::other)?, unix_seconds(SystemTime::now())],
        ).map_err(io::Error::other)?;
        Ok(())
    }

    pub fn save_measurement(&self, app: &App, fingerprint: &str, bytes: u64) -> io::Result<()> {
        let db = self.open_database()?;
        db.execute(
            "INSERT INTO measurements VALUES (?1, ?2, ?3)
             ON CONFLICT(app_id) DO UPDATE SET fingerprint = excluded.fingerprint, bytes = excluded.bytes",
            params![app.id, fingerprint, bytes.to_string()],
        ).map_err(io::Error::other)?;
        Ok(())
    }

    pub fn measurements(&self, apps: &[App]) -> io::Result<HashMap<String, u64>> {
        let db = self.open_database()?;
        let mut query = db
            .prepare("SELECT fingerprint, bytes FROM measurements WHERE app_id = ?1")
            .map_err(io::Error::other)?;
        let mut measured = HashMap::new();
        for app in apps {
            let stored: Option<(String, String)> = query
                .query_row([&app.id], |row| Ok((row.get(0)?, row.get(1)?)))
                .optional()
                .map_err(io::Error::other)?;
            if let Some((fingerprint, bytes)) = stored
                && measurement_fingerprint(app).ok().as_deref() == Some(fingerprint.as_str())
                && let Ok(bytes) = bytes.parse()
            {
                measured.insert(app.id.clone(), bytes);
            }
        }
        Ok(measured)
    }
}

pub(crate) fn measurement_fingerprint(app: &App) -> io::Result<String> {
    let path = app
        .location
        .as_deref()
        .ok_or_else(|| io::Error::other("Missing installation path"))?;
    let modified = regular_path(path)?.last_write_time();
    let metadata =
        serde_json::to_vec(&(app, identity(path)?, modified)).map_err(io::Error::other)?;
    Ok(sha256(&metadata))
}
