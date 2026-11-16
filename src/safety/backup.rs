use crate::{
    scanner::{
        filesystem_scanner::inspect_tree,
        leftovers::{Candidate, Integration},
    },
    storage::Locations,
    utils::{
        hashing::{file_hash, sha256},
        paths::regular_path,
    },
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
};
use uuid::Uuid;
use walkdir::WalkDir;
use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;

pub(crate) fn directory_digest(path: &Path) -> io::Result<String> {
    inspect_tree(path)?;
    let mut hash = Sha256::new();
    for entry in WalkDir::new(path).sort_by_file_name().follow_links(false) {
        let entry = entry.map_err(io::Error::other)?;
        let relative = entry
            .path()
            .strip_prefix(path)
            .map_err(io::Error::other)?
            .as_os_str();
        let relative = relative.to_string_lossy();
        hash.update((relative.len() as u64).to_le_bytes());
        hash.update(relative.as_bytes());
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("Reparse point in backup"));
        }
        hash.update([u8::from(metadata.is_file())]);
        if metadata.is_file() {
            hash.update(file_hash(entry.path())?.as_bytes());
        }
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub(crate) fn integration_digest(item: &Integration) -> io::Result<String> {
    let data = serde_json::to_vec(item).map_err(io::Error::other)?;
    Ok(sha256(&data))
}

impl Locations {
    pub(crate) fn quarantine_dir(&self) -> io::Result<PathBuf> {
        let path = self.root()?.join("quarantine");
        fs::create_dir_all(&path)?;
        if !regular_path(&path)?.is_dir() {
            return Err(io::Error::other("Invalid quarantine directory"));
        }
        Ok(path)
    }

    pub(crate) fn write_backup(&self, item: &Candidate) -> io::Result<PathBuf> {
        let path = self.quarantine_dir()?.join(Uuid::new_v4().to_string());
        fs::create_dir(&path)?;
        let mut manifest = File::create_new(path.join("manifest.json"))?;
        serde_json::to_writer(&mut manifest, item).map_err(io::Error::other)?;
        manifest.sync_all()?;
        Ok(path)
    }

    pub fn backup_before_uninstall(&self, items: &[Candidate]) -> io::Result<()> {
        for item in items {
            if let Some(integration @ (Integration::Registry { .. } | Integration::Startup { .. })) =
                &item.integration
                && integration.writable()
                && integration.present()
            {
                let mut backup = item.clone();
                backup.backup_digest = Some(integration_digest(integration)?);
                self.write_backup(&backup)?;
                self.log("preflight-backup", &integration.label(), "ok")?;
            }
        }
        Ok(())
    }
}
