use super::{
    leftovers::{Integration, Profile, command_target, owns},
    registry_scanner::{Hive, MAX_VALUE_BYTES, RawValue},
};
use crate::{
    uninstall::elevation::ComApartment,
    utils::{
        hashing::file_hash,
        paths::{comparable, is_local_absolute, known_folder, regular_path},
        strings::expand,
    },
};
use std::{
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    ptr::null_mut,
};
use walkdir::WalkDir;
use windows_api::{
    Win32::{
        System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ},
        UI::Shell::{IShellLinkW, SLGP_RAWPATH, ShellLink},
    },
    core::{Interface, PCWSTR},
};
use windows_registry::Value;
use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_Startup};

pub(crate) const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
pub(crate) const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";

pub(crate) fn scan(root: &Path, profile: Profile, findings: &mut Vec<Integration>) {
    let hives: &[_] = if profile == Profile::Deep {
        &[Hive::User, Hive::Machine32, Hive::Machine64]
    } else {
        &[Hive::User]
    };
    for &hive in hives {
        for path in [RUN, RUN_ONCE] {
            if let Ok(key) = hive.key(path)
                && let Ok(entries) = key.values()
            {
                findings.extend(
                    entries.filter_map(|(name, value)| autorun(root, hive, path, name, value)),
                );
            }
        }
    }
    if let Ok(_apartment) = ComApartment::open() {
        for folder in shortcut_roots() {
            for entry in WalkDir::new(folder)
                .max_depth(4)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .take(4_000)
            {
                if entry
                    .path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
                    && let Some(item) = shortcut(entry.path())
                    && matches!(&item, Integration::Shortcut { target, .. } if owns(root, target))
                {
                    findings.push(item);
                }
            }
        }
    }
}

fn autorun(root: &Path, hive: Hive, path: &str, name: String, value: Value) -> Option<Integration> {
    if value.len() >= MAX_VALUE_BYTES {
        return None;
    }
    let command = String::try_from(value.clone()).ok()?;
    let target = command_target(&command)?;
    owns(root, &target).then(|| Integration::Startup {
        hive,
        key: path.into(),
        value: RawValue::new(name, value),
        target,
    })
}

#[cfg(test)]
#[path = "../../tests/integration/startup_inventory.rs"]
mod tests;

fn shortcut_roots() -> impl Iterator<Item = PathBuf> {
    [FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_Startup]
        .into_iter()
        .filter_map(|id| known_folder(&id).ok())
}

fn shortcut(path: &Path) -> Option<Integration> {
    let metadata = regular_path(path).ok()?;
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return None;
    }
    let filename: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let link: IShellLinkW =
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }.ok()?;
    let persist: IPersistFile = link.cast().ok()?;
    let mut buffer = vec![0; 32_768];
    unsafe {
        persist.Load(PCWSTR(filename.as_ptr()), STGM_READ).ok()?;
        link.GetPath(&mut buffer, null_mut(), SLGP_RAWPATH.0 as u32)
            .ok()?;
    }
    let target = String::from_utf16(&buffer[..buffer.iter().position(|unit| *unit == 0)?]).ok()?;
    let target = PathBuf::from(expand(&target).ok()?);
    if !is_local_absolute(&target) {
        return None;
    }
    Some(Integration::Shortcut {
        path: path.to_owned(),
        target,
        digest: file_hash(path).ok()?,
    })
}

pub(crate) fn shortcut_allowed(path: &Path) -> bool {
    is_local_absolute(path)
        && path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"))
        && shortcut_roots().any(|root| comparable(path).starts_with(comparable(&root)))
}
