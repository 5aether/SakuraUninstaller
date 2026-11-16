use super::Locations;
use crate::scanner::installed_apps::App;
use rusqlite::{Connection, params};
use serde::de::DeserializeOwned;
use std::{
    fs,
    io::{self, Read},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

/// Persisted types supply defaults for fields introduced after the first version.
pub(crate) fn decode_json<T: DeserializeOwned>(reader: impl Read) -> io::Result<T> {
    serde_json::from_reader(io::BufReader::new(reader)).map_err(io::Error::other)
}

pub(crate) fn unix_seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
        .unwrap_or_default()
}

pub(crate) fn migrate(connection: &mut Connection, locations: &Locations) -> io::Result<()> {
    let transaction = connection.transaction().map_err(io::Error::other)?;
    let version: u32 = transaction
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(io::Error::other)?;
    match version {
        1 => return Ok(()),
        0 => (),
        _ => return Err(io::Error::other("Sakura database requires a newer version")),
    }
    transaction
        .execute_batch(
            "CREATE TABLE inventory (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            snapshot TEXT NOT NULL,
            scanned INTEGER NOT NULL
        );
        CREATE TABLE measurements (
            app_id TEXT PRIMARY KEY,
            fingerprint TEXT NOT NULL,
            bytes TEXT NOT NULL
        );
        CREATE TABLE reports (
            id INTEGER PRIMARY KEY,
            recorded INTEGER NOT NULL,
            italian TEXT NOT NULL,
            english TEXT NOT NULL
        );
        CREATE INDEX reports_recent ON reports (recorded DESC, id DESC);",
        )
        .map_err(io::Error::other)?;
    // These legacy files stay available to previous releases and to data recovery.
    if let Ok(Some(mut apps)) = locations.read_state::<Vec<App>>("inventory.json") {
        for app in &mut apps {
            app.source_stamp = 0;
        }
        let snapshot = serde_json::to_string(&apps).map_err(io::Error::other)?;
        transaction
            .execute("INSERT INTO inventory VALUES (1, ?1, 0)", [snapshot])
            .map_err(io::Error::other)?;
    }
    for entry in fs::read_dir(locations.root()?)? {
        let entry = entry?;
        let filename = entry.file_name();
        let Some(filename) = filename.to_str() else {
            continue;
        };
        let Some(id) = filename
            .strip_prefix("report-")
            .and_then(|name| name.strip_suffix(".json"))
        else {
            continue;
        };
        if Uuid::parse_str(id).is_err() {
            continue;
        }
        if let Ok(Some([italian, english])) = locations.read_state::<[String; 2]>(filename) {
            let recorded = entry
                .metadata()?
                .modified()
                .map(unix_seconds)
                .unwrap_or_default();
            transaction
                .execute(
                    "INSERT INTO reports (recorded, italian, english) VALUES (?1, ?2, ?3)",
                    params![recorded, italian, english],
                )
                .map_err(io::Error::other)?;
        }
    }
    transaction
        .pragma_update(None, "user_version", 1)
        .map_err(io::Error::other)?;
    transaction.commit().map_err(io::Error::other)
}
