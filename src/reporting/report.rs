use crate::storage::{Locations, migrations::unix_seconds};
use rusqlite::{OptionalExtension, params};
use std::{io, time::SystemTime};

#[derive(Clone)]
pub struct Message {
    it: String,
    en: String,
}

impl Message {
    pub fn new(it: impl Into<String>, en: impl Into<String>) -> Self {
        Self {
            it: it.into(),
            en: en.into(),
        }
    }

    pub fn text(&self, italian: bool) -> &str {
        if italian { &self.it } else { &self.en }
    }
}

impl Locations {
    pub fn save_report(&self, italian: &str, english: &str) -> io::Result<()> {
        self.open_database()?
            .execute(
                "INSERT INTO reports (recorded, italian, english) VALUES (?1, ?2, ?3)",
                params![unix_seconds(SystemTime::now()), italian, english],
            )
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn read_report(&self, offset: i32) -> io::Result<Option<(Message, String, bool)>> {
        let db = self.open_database()?;
        db.query_row(
            "SELECT italian, english, COALESCE(datetime(recorded, 'unixepoch', 'localtime'), ''),
                EXISTS(SELECT 1 FROM reports ORDER BY recorded DESC, id DESC LIMIT 1 OFFSET ?1 + 1)
             FROM reports ORDER BY recorded DESC, id DESC LIMIT 1 OFFSET ?1",
            [offset],
            |row| {
                Ok((
                    Message::new(row.get::<_, String>(0)?, row.get::<_, String>(1)?),
                    row.get(2)?,
                    row.get(3)?,
                ))
            },
        )
        .optional()
        .map_err(io::Error::other)
    }
}
