use super::{backup::directory_digest, compression};
use crate::{
    scanner::{
        filesystem_scanner::inspect_tree, leftovers::Integration, registry_scanner::user_path,
    },
    storage::Locations,
    utils::paths::{move_new, regular_path},
};
use std::{
    fs::{self, File},
    io,
    path::Path,
    ptr::{null, null_mut},
};
use uuid::Uuid;
use windows_registry::Key;
use windows_sys::Win32::System::Registry::{
    KEY_READ, KEY_WRITE, REG_CREATED_NEW_KEY, RegCreateKeyExW,
};

impl Locations {
    pub fn restore(&self, id: Uuid) -> io::Result<()> {
        let path = self.quarantine_dir()?.join(id.to_string());
        let item = self.read_saved(&path)?;
        if let Some(integration) = &item.integration {
            self.log("restore", &item.path.to_string_lossy(), "pending")?;
            integration.restore(&path.join("payload"))?;
            File::create_new(path.join("restored"))?.sync_all()?;
            self.log("restore", &item.path.to_string_lossy(), "ok")?;
            return Ok(());
        }
        let parent = item
            .path
            .parent()
            .ok_or_else(|| io::Error::other("Original path unavailable"))?;
        regular_path(parent)?;
        inspect_tree(&path.join("payload"))?;
        if let Some(digest) = &item.backup_digest
            && &directory_digest(&path.join("payload"))? != digest
        {
            return Err(io::Error::other("Quarantine checksum mismatch"));
        }
        self.log("restore", &item.path.to_string_lossy(), "pending")?;
        let payload = path.join("payload");
        let result = (|| {
            match fs::symlink_metadata(&item.path) {
                Ok(_) => {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Restore target already exists",
                    ));
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error),
            }
            match compression::undo(&path, &payload) {
                Ok(0) => (),
                Ok(skipped) => {
                    let _ = self.log(
                        "decompress",
                        &item.path.to_string_lossy(),
                        &format!("{skipped} files kept compressed"),
                    );
                }
                Err(error) => {
                    let _ = self.log(
                        "decompress",
                        &item.path.to_string_lossy(),
                        &error.to_string(),
                    );
                }
            }
            move_new(&payload, &item.path)
        })();
        let _ = self.log(
            "restore",
            &item.path.to_string_lossy(),
            if result.is_ok() { "ok" } else { "failed" },
        );
        result
    }
}

impl Integration {
    pub fn restore(&self, payload: &Path) -> io::Result<()> {
        if !self.backed_up(payload) {
            return Err(io::Error::other("Backup missing or already restored"));
        }
        match self {
            Self::Registry { key, values, .. } => {
                let (path, name) = key
                    .rsplit_once('\\')
                    .ok_or_else(|| io::Error::other("Invalid registry key"))?;
                let parent = user_path(path, KEY_READ | KEY_WRITE)?;
                let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
                let mut handle = null_mut();
                let mut disposition = 0;
                let status = unsafe {
                    RegCreateKeyExW(
                        parent.as_raw(),
                        name.as_ptr(),
                        0,
                        null(),
                        0,
                        KEY_READ | KEY_WRITE,
                        null(),
                        &mut handle,
                        &mut disposition,
                    )
                };
                if status != 0 {
                    return Err(io::Error::from_raw_os_error(status as i32));
                }
                let target = unsafe { Key::from_raw(handle) };
                if disposition != REG_CREATED_NEW_KEY {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        "Registry key already exists",
                    ));
                }
                for value in values {
                    value.write(&target)?;
                }
            }
            Self::Startup { key, value, .. } => {
                let target = user_path(key, KEY_READ | KEY_WRITE)?;
                match target.get_value(&value.name) {
                    Ok(_) => {
                        return Err(io::Error::new(
                            io::ErrorKind::AlreadyExists,
                            "Startup value already exists",
                        ));
                    }
                    Err(error) if error.code().0 == 0x8007_0002_u32 as i32 => (),
                    Err(error) => return Err(io::Error::other(error)),
                }
                value.write(&target)?;
            }
            Self::Shortcut { path, .. } => {
                regular_path(
                    path.parent()
                        .ok_or_else(|| io::Error::other("Invalid shortcut destination"))?,
                )?;
                move_new(payload, path)?;
            }
            _ => return Err(io::Error::other("Review item is read-only")),
        }
        Ok(())
    }
}
