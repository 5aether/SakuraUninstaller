use crate::{
    scanner::filesystem_scanner::{Identity, file_info},
    storage::migrations::decode_json,
    utils::paths::regular_path,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Component, Path, PathBuf},
    ptr::null_mut,
};
use walkdir::WalkDir;
use windows_sys::Win32::{
    Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_COMPRESSED, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_ENCRYPTED, FILE_ATTRIBUTE_REPARSE_POINT, FILE_ATTRIBUTE_SPARSE_FILE,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, GetVolumeInformationByHandleW,
    },
    System::{
        IO::DeviceIoControl, Ioctl::FSCTL_SET_COMPRESSION, SystemServices::FILE_FILE_COMPRESSION,
    },
};

const LEDGER: &str = "compression.json";
const MAX_ENTRIES: usize = 100_000;
const MAX_LEDGER: usize = 32 * 1024 * 1024;
const MIN_SIZE: u64 = 4096;
const MAX_SIZE: u64 = 30_000_000_000;
const EXCLUDED: u32 = FILE_ATTRIBUTE_DIRECTORY
    | FILE_ATTRIBUTE_ENCRYPTED
    | FILE_ATTRIBUTE_REPARSE_POINT
    | FILE_ATTRIBUTE_SPARSE_FILE;

#[derive(Serialize, Deserialize)]
struct Entry {
    relative: PathBuf,
    identity: Identity,
}

#[derive(Default, Debug)]
pub(crate) struct Summary {
    pub compressed: usize,
    pub skipped: usize,
}

pub(crate) fn supported(root: &Path) -> io::Result<bool> {
    let handle = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(root)?;
    let mut flags = 0;
    let result = unsafe {
        GetVolumeInformationByHandleW(
            handle.as_raw_handle(),
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(flags & FILE_FILE_COMPRESSION != 0)
}

fn open_file(path: &Path) -> io::Result<File> {
    regular_path(path)?;
    OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

fn eligible(info: &BY_HANDLE_FILE_INFORMATION) -> bool {
    // Compression is an attribute of the file itself, including all its hard links.
    info.nNumberOfLinks == 1 && info.dwFileAttributes & EXCLUDED == 0
}

fn resolve(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if relative.as_os_str().is_empty()
        || !relative.components().all(|part| match part {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                !name.ends_with(['.', ' ']) && !name.contains([':', '\0', '*', '?'])
            }
            _ => false,
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid relative compression path",
        ));
    }
    Ok(root.join(relative))
}

fn change(root: &Path, entry: &Entry, compressed: bool) -> io::Result<bool> {
    let path = resolve(root, &entry.relative)?;
    let file = open_file(&path)?;
    let info = file_info(&file)?;
    if !eligible(&info) || Identity::from_info(&info) != entry.identity {
        return Err(io::Error::other("File identity or link count changed"));
    }
    if (info.dwFileAttributes & FILE_ATTRIBUTE_COMPRESSED != 0) == compressed {
        return Ok(false);
    }
    let format: u16 = u16::from(compressed);
    let mut returned = 0;
    let result = unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            FSCTL_SET_COMPRESSION,
            (&raw const format).cast(),
            size_of::<u16>() as u32,
            null_mut(),
            0,
            &mut returned,
            null_mut(),
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(true)
}

pub(crate) fn compress(record: &Path, payload: &Path) -> io::Result<Summary> {
    if !supported(payload)? {
        return Ok(Summary::default());
    }
    let mut summary = Summary::default();
    let mut planned = Vec::new();
    for (index, entry) in WalkDir::new(payload)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .enumerate()
    {
        if index >= MAX_ENTRIES {
            return Err(io::Error::other(
                "Compression scan exceeded its entry limit",
            ));
        }
        let entry = entry.map_err(io::Error::other)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let metadata = entry.metadata().map_err(io::Error::other)?;
        if !(MIN_SIZE..=MAX_SIZE).contains(&metadata.len())
            || metadata.file_attributes() & (EXCLUDED | FILE_ATTRIBUTE_COMPRESSED) != 0
        {
            continue;
        }
        let Ok(info) = open_file(entry.path()).and_then(|file| file_info(&file)) else {
            summary.skipped += 1;
            continue;
        };
        if !eligible(&info) || info.dwFileAttributes & FILE_ATTRIBUTE_COMPRESSED != 0 {
            summary.skipped += 1;
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(payload)
            .map_err(io::Error::other)?;
        resolve(payload, relative)?;
        planned.push(Entry {
            relative: relative.to_owned(),
            identity: Identity::from_info(&info),
        });
    }
    if planned.is_empty() {
        return Ok(summary);
    }

    // The recovery plan must be durable before changing any file attributes.
    let bytes = serde_json::to_vec(&planned).map_err(io::Error::other)?;
    if bytes.len() > MAX_LEDGER {
        return Err(io::Error::other(
            "Compression recovery journal exceeds its size limit",
        ));
    }
    let mut ledger = File::create_new(record.join(LEDGER))?;
    ledger.write_all(&bytes)?;
    ledger.sync_all()?;
    drop(ledger);

    for entry in &planned {
        match change(payload, entry, true) {
            Ok(true) => summary.compressed += 1,
            Ok(false) => (),
            Err(_) => summary.skipped += 1,
        }
    }
    Ok(summary)
}

pub(crate) fn undo(record: &Path, payload: &Path) -> io::Result<usize> {
    let ledger = record.join(LEDGER);
    let meta = match regular_path(&ledger) {
        Ok(meta) => meta,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error),
    };
    if !meta.is_file() || meta.len() > MAX_LEDGER as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid compression recovery journal",
        ));
    }
    let entries: Vec<Entry> = decode_json(File::open(ledger)?)?;
    if entries.len() > MAX_ENTRIES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Too many compression records",
        ));
    }
    // Validate every path before changing the first attribute.
    for entry in &entries {
        resolve(payload, &entry.relative)?;
    }
    Ok(entries
        .iter()
        .filter(|entry| change(payload, entry, false).is_err())
        .count())
}
