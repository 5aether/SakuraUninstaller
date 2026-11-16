use super::installed_apps::App;
use crate::utils::paths::regular_path;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io,
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
};
use walkdir::WalkDir;
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, GetFileInformationByHandle,
};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct Identity {
    volume: u32,
    index: u64,
    created: u64,
}

impl Identity {
    pub(crate) fn from_info(info: &BY_HANDLE_FILE_INFORMATION) -> Self {
        Self {
            volume: info.dwVolumeSerialNumber,
            index: u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow),
            created: u64::from(info.ftCreationTime.dwHighDateTime) << 32
                | u64::from(info.ftCreationTime.dwLowDateTime),
        }
    }
}

pub(crate) fn file_info(file: &File) -> io::Result<BY_HANDLE_FILE_INFORMATION> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

pub(crate) fn identity(path: &Path) -> io::Result<Identity> {
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    let info = file_info(&file)?;
    if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(io::Error::other("Directory changed into a reparse point"));
    }
    Ok(Identity::from_info(&info))
}

pub(crate) fn inspect_tree(path: &Path) -> io::Result<(Identity, u64)> {
    let root = regular_path(path)?;
    if !root.is_dir() {
        return Err(io::Error::other("Expected a directory"));
    }
    let before = identity(path)?;
    let mut bytes = 0_u64;
    for (index, entry) in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if index >= 100_000 {
            return Err(io::Error::other("Directory exceeds scan limit"));
        }
        let entry = entry.map_err(io::Error::other)?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("Reparse point in directory"));
        }
        if metadata.is_file() {
            bytes = bytes.saturating_add(metadata.len());
        }
    }
    if identity(path)? != before {
        return Err(io::Error::other("Directory changed during scan"));
    }
    Ok((before, bytes))
}

pub fn measured_size(app: &App) -> io::Result<u64> {
    let path = app
        .location
        .as_deref()
        .ok_or_else(|| io::Error::other("Missing installation path"))?;
    if !path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&app.name))
        || path.parent().and_then(Path::parent).is_none()
    {
        return Err(io::Error::other(
            "Installation location is not an individual program directory",
        ));
    }
    inspect_tree(path).map(|(_, bytes)| bytes)
}
