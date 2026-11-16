use super::{
    backup::{directory_digest, integration_digest},
    compression,
};
use crate::{
    scanner::{
        filesystem_scanner::identity,
        installed_apps::App,
        leftovers::{Candidate, Integration, remaining},
        registry_scanner::user_path,
    },
    storage::{Locations, migrations::decode_json},
    utils::{
        hashing::file_hash,
        paths::{move_new, regular_path},
    },
};
use std::{
    fs::{self, File},
    io,
    path::Path,
};
use uuid::Uuid;
use windows_sys::Win32::System::Registry::{KEY_READ, KEY_WRITE, RegDeleteKeyW};

#[derive(Clone)]
pub struct Saved {
    pub id: Uuid,
    pub item: Candidate,
}

impl Locations {
    pub fn saved(&self) -> io::Result<Vec<Saved>> {
        let mut entries = Vec::new();
        let quarantine = self.store.join("quarantine");
        match regular_path(&quarantine) {
            Ok(metadata) if metadata.is_dir() => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(entries),
            _ => return Err(io::Error::other("Invalid quarantine directory")),
        }
        for folder in fs::read_dir(quarantine)? {
            let folder = folder?;
            let Some(id) = folder
                .file_name()
                .to_str()
                .and_then(|name| Uuid::parse_str(name).ok())
            else {
                continue;
            };
            let path = folder.path();
            if let Ok(item) = self.read_saved(&path) {
                entries.push(Saved { id, item });
            }
        }
        entries.sort_by(|a, b| {
            a.item
                .app
                .cmp(&b.item.app)
                .then(a.item.path.cmp(&b.item.path))
                .then(a.item.backup_digest.cmp(&b.item.backup_digest))
        });
        entries.dedup_by(|a, b| a.item.integration.is_some() && a.item == b.item);
        Ok(entries)
    }

    pub(crate) fn read_saved(&self, record: &Path) -> io::Result<Candidate> {
        let manifest = record.join("manifest.json");
        let metadata = regular_path(&manifest)?;
        if !metadata.is_file() || metadata.len() > 128 * 1024 {
            return Err(io::Error::other("Invalid manifest"));
        }
        let item: Candidate = decode_json(File::open(manifest)?)?;
        if let Some(integration) = &item.integration {
            if record.join("restored").exists()
                || self.rules.protects_integration(&integration.label())
                || !integration.backed_up(&record.join("payload"))
                || item.backup_digest.as_deref() != Some(&integration_digest(integration)?)
            {
                return Err(io::Error::other("Integration backup unavailable"));
            }
            return Ok(item);
        }
        let contents = regular_path(&record.join("payload"))?;
        if !contents.is_dir()
            || identity(&record.join("payload"))? != item.identity
            || !self.allows(&item.path, item.evidence)
        {
            return Err(io::Error::other("Quarantine record unavailable"));
        }
        Ok(item)
    }

    pub(crate) fn quarantine(
        &self,
        item: &Candidate,
        installed: &[App],
    ) -> io::Result<compression::Summary> {
        let verified = remaining(self, std::slice::from_ref(item), installed);
        let mut item = verified
            .first()
            .filter(|item| item.writable())
            .ok_or_else(|| io::Error::other("Candidate changed, is read-only or is still in use"))?
            .clone();
        item.backup_digest = Some(match &item.integration {
            Some(integration) => integration_digest(integration)?,
            None => directory_digest(&item.path)?,
        });
        let path = self.write_backup(&item)?;
        self.log("quarantine", &item.path.to_string_lossy(), "pending")?;
        let result = match &item.integration {
            Some(integration) => integration.remove(&path.join("payload")),
            None => move_new(&item.path, &path.join("payload")),
        };
        let _ = self.log(
            "quarantine",
            &item.path.to_string_lossy(),
            if result.is_ok() { "ok" } else { "failed" },
        );
        result?;
        if item.integration.is_some() {
            return Ok(compression::Summary::default());
        }
        let summary = match compression::compress(&path, &path.join("payload")) {
            Ok(summary) => summary,
            Err(error) => {
                let _ = self.log("compress", &item.path.to_string_lossy(), &error.to_string());
                return Ok(compression::Summary {
                    compressed: 0,
                    skipped: 1,
                });
            }
        };
        if summary.compressed != 0 || summary.skipped != 0 {
            let _ = self.log(
                "compress",
                &item.path.to_string_lossy(),
                &format!(
                    "{} compressed, {} skipped",
                    summary.compressed, summary.skipped
                ),
            );
        }
        Ok(summary)
    }
}

impl Integration {
    pub fn backed_up(&self, payload: &Path) -> bool {
        if !self.writable() {
            return false;
        }
        match self {
            Self::Shortcut { digest, .. } => {
                regular_path(payload).is_ok()
                    && file_hash(payload).is_ok_and(|hash| &hash == digest)
            }
            _ => !self.present(),
        }
    }

    pub fn remove(&self, payload: &Path) -> io::Result<()> {
        if !self.writable() || !self.present() {
            return Err(io::Error::other("Review item changed or is read-only"));
        }
        match self {
            Self::Registry { key, .. } => {
                let (parent, name) = key
                    .rsplit_once('\\')
                    .ok_or_else(|| io::Error::other("Invalid registry key"))?;
                let parent = user_path(parent, KEY_READ | KEY_WRITE)?;
                let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
                // RegDeleteKeyW refuses to remove a key with subkeys.
                let status = unsafe { RegDeleteKeyW(parent.as_raw(), name.as_ptr()) };
                if status != 0 {
                    return Err(io::Error::from_raw_os_error(status as i32));
                }
            }
            Self::Startup { key, value, .. } => user_path(key, KEY_READ | KEY_WRITE)?
                .remove_value(&value.name)
                .map_err(io::Error::other)?,
            Self::Shortcut { path, .. } => move_new(path, payload)?,
            _ => return Err(io::Error::other("Review item is read-only")),
        }
        Ok(())
    }
}
