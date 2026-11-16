use crate::{storage::Locations, utils::paths::regular_path};
use serde::Serialize;
use std::{
    fs::OpenOptions,
    io::{self, Write},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
struct Event<'a> {
    unix_seconds: u64,
    action: &'a str,
    subject: &'a str,
    result: &'a str,
}

impl Locations {
    pub fn log(&self, action: &str, subject: &str, result: &str) -> io::Result<()> {
        let path = self.root()?.join("activity.jsonl");
        match regular_path(&path) {
            Ok(metadata) if metadata.is_file() => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            _ => return Err(io::Error::other("Invalid activity log")),
        }
        let mut file = OpenOptions::new().append(true).create(true).open(path)?;
        let mut line = serde_json::to_vec(&Event {
            unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            action,
            subject,
            result,
        })
        .map_err(io::Error::other)?;
        line.push(b'\n');
        file.write_all(&line)
    }
}
