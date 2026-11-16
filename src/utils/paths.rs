use super::strings::wide;
use std::{
    ffi::OsString,
    fs::{self, Metadata},
    io,
    os::windows::{ffi::OsStringExt, fs::MetadataExt},
    path::{Component, Path, PathBuf, Prefix},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_WRITE_THROUGH, MoveFileExW},
    System::{Com::CoTaskMemFree, SystemInformation::GetSystemDirectoryW},
    UI::Shell::SHGetKnownFolderPath,
};

pub fn known_folder(id: &windows_sys::core::GUID) -> io::Result<PathBuf> {
    let mut pointer = null_mut();
    let status = unsafe { SHGetKnownFolderPath(id, 0, null_mut(), &mut pointer) };
    if status < 0 || pointer.is_null() {
        if !pointer.is_null() {
            unsafe { CoTaskMemFree(pointer.cast()) };
        }
        return Err(io::Error::other(format!(
            "Windows folder 0x{:08X}",
            status as u32
        )));
    }
    // The Shell returns an allocated, null-terminated UTF-16 string.
    let path = unsafe {
        let mut length = 0;
        while *pointer.add(length) != 0 {
            length += 1;
        }
        let path = PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(
            pointer, length,
        )));
        CoTaskMemFree(pointer.cast());
        path
    };
    if !is_local_absolute(&path) {
        return Err(io::Error::other("Windows folder is not on a local drive"));
    }
    Ok(path)
}

pub(crate) fn system_directory() -> io::Result<PathBuf> {
    let mut buffer = [0; 32768];
    let written = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if written == 0 || written as usize >= buffer.len() {
        return Err(io::Error::last_os_error());
    }
    Ok(OsString::from_wide(&buffer[..written as usize]).into())
}

pub fn is_local_absolute(path: &Path) -> bool {
    path.is_absolute()
        && matches!(
            path.components().next(),
            Some(Component::Prefix(prefix))
                if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        )
        && path.components().all(|component| match component {
            Component::ParentDir | Component::CurDir => false,
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                !name.ends_with(['.', ' ']) && !name.contains([':', '*', '?', '\0'])
            }
            _ => true,
        })
}

pub fn comparable(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(&value).to_lowercase())
}

pub fn move_new(source: &Path, destination: &Path) -> io::Result<()> {
    let source = wide(source.as_os_str());
    let destination = wide(destination.as_os_str());
    // Without COPY_ALLOWED or REPLACE_EXISTING, this is a move on the same volume.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn open_folder(path: &Path) -> io::Result<()> {
    let system = system_directory()?;
    let windows = system
        .parent()
        .ok_or_else(|| io::Error::other("Windows path unavailable"))?;
    std::process::Command::new(windows.join("explorer.exe"))
        .arg(path)
        .spawn()?;
    Ok(())
}

pub(crate) fn regular_path(path: &Path) -> io::Result<Metadata> {
    if !is_local_absolute(path) {
        return Err(io::Error::other("Non-local path"));
    }
    let original = fs::symlink_metadata(path)?;
    for ancestor in path.ancestors().filter(|part| part.has_root()) {
        if fs::symlink_metadata(ancestor)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(io::Error::other("Reparse point"));
        }
    }
    Ok(original)
}
