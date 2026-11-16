use super::{
    installed_apps::App,
    leftovers::{Integration, Profile},
};
use crate::{
    uninstall::{inno_setup, msi::is_product_code, nsis},
    utils::strings::{component, expand},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    io,
    path::PathBuf,
    ptr::{null, null_mut},
};
use windows_registry::{CURRENT_USER, Key, LOCAL_MACHINE, Type, Value};
use windows_sys::Win32::System::Registry::{
    KEY_READ, REG_LINK, REG_OPTION_OPEN_LINK, RegOpenKeyExW,
};

pub(crate) const MAX_VALUE_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Hive {
    User,
    Machine64,
    Machine32,
}

impl Hive {
    pub(crate) fn key(self, path: &str) -> io::Result<Key> {
        match self {
            Self::User => CURRENT_USER.open(path),
            Self::Machine64 => LOCAL_MACHINE.options().read().wow64_64().open(path),
            Self::Machine32 => LOCAL_MACHINE.options().read().wow64_32().open(path),
        }
        .map_err(io::Error::other)
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::User => "HKCU",
            Self::Machine64 => "HKLM (64)",
            Self::Machine32 => "HKLM (32)",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct RawValue {
    pub(crate) name: String,
    pub(crate) kind: u32,
    pub(crate) data: Vec<u8>,
}

impl RawValue {
    pub(crate) fn new(name: String, data: Value) -> Self {
        Self {
            name,
            kind: data.ty().into(),
            data: data.to_vec(),
        }
    }

    pub(crate) fn write(&self, key: &Key) -> io::Result<()> {
        let mut value = Value::from(self.data.as_slice());
        value.set_ty(Type::from(self.kind));
        key.set_value(&self.name, &value).map_err(io::Error::other)
    }
}

pub const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

pub(crate) fn registered(previous: &[App]) -> io::Result<Vec<App>> {
    use windows_registry::{CURRENT_USER, LOCAL_MACHINE};

    let views = [
        (
            "HKLM64",
            LOCAL_MACHINE.options().read().wow64_64().open(UNINSTALL),
        ),
        (
            "HKLM32",
            LOCAL_MACHINE.options().read().wow64_32().open(UNINSTALL),
        ),
        ("HKCU", CURRENT_USER.open(UNINSTALL)),
    ];
    let mut apps = Vec::new();
    let cache: HashMap<_, _> = previous
        .iter()
        .filter(|app| app.source_stamp != 0)
        .map(|app| (app.id.as_str(), app))
        .collect();
    for (scope, result) in views {
        let parent = match result {
            Ok(key) => key,
            Err(error) if error.code().0 == 0x8007_0002_u32 as i32 => continue,
            Err(error) => return Err(io::Error::other(error)),
        };
        for subkey in parent.keys().map_err(io::Error::other)? {
            let key = match parent.open(&subkey) {
                Ok(key) => key,
                Err(error) if error.code().0 == 0x8007_0002_u32 as i32 => continue,
                Err(error) => return Err(io::Error::other(error)),
            };
            let stamp = stamp(&key);
            let id = format!("{scope}\\{}", subkey.to_lowercase());
            if let Some(cached) = cache.get(id.as_str())
                && cached.source_stamp == stamp
            {
                apps.push((*cached).clone());
                continue;
            }
            let Some(mut app) = read_app(&key, scope, &subkey) else {
                continue;
            };
            app.source_stamp = stamp;
            apps.push(app);
        }
    }
    Ok(apps)
}

pub(crate) fn stamp(key: &windows_registry::Key) -> u64 {
    use windows_sys::Win32::{Foundation::FILETIME, System::Registry::RegQueryInfoKeyW};
    let mut time = FILETIME::default();
    let result = unsafe {
        RegQueryInfoKeyW(
            key.as_raw(),
            null_mut(),
            null_mut(),
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut time,
        )
    };
    if result == 0 {
        u64::from(time.dwHighDateTime) << 32 | u64::from(time.dwLowDateTime)
    } else {
        0
    }
}

pub fn read_app(key: &windows_registry::Key, scope: &str, subkey: &str) -> Option<App> {
    let string = |field| {
        key.get_string(field)
            .ok()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty() && !value.contains('\0'))
    };
    let flag = |field| key.get_u32(field).unwrap_or_default() != 0;
    if flag("SystemComponent") || string("ParentKeyName").is_some() {
        return None;
    }
    let release = string("ReleaseType")
        .unwrap_or_default()
        .to_ascii_lowercase();
    if [
        "update",
        "security update",
        "hotfix",
        "service pack",
        "update rollup",
    ]
    .contains(&release.as_str())
    {
        return None;
    }
    let command = string("UninstallString");
    let installer = if flag("WindowsInstaller") {
        "MSI"
    } else if string(inno_setup::SETUP_VERSION).is_some() || string(inno_setup::APP_PATH).is_some()
    {
        "Inno Setup"
    } else if string(nsis::VERSION).is_some() {
        "NSIS"
    } else if command.as_ref().is_some_and(|value| {
        let lower = value.to_ascii_lowercase();
        lower.contains("update.exe") && lower.contains("--uninstall")
    }) {
        "Squirrel / Electron"
    } else {
        ""
    }
    .to_owned();
    Some(App {
        id: format!("{scope}\\{}", subkey.to_lowercase()),
        name: string("DisplayName")?,
        publisher: string("Publisher").unwrap_or_default(),
        version: string("DisplayVersion").unwrap_or_default(),
        install_date: string("InstallDate").and_then(|value| install_date(&value)),
        size_kib: key.get_u64("EstimatedSize").ok().filter(|size| *size != 0),
        location: string("InstallLocation")
            .or_else(|| string(inno_setup::APP_PATH))
            .and_then(|path| expand(path.trim_matches('"')).ok())
            .map(PathBuf::from)
            .filter(|path| path.is_absolute()),
        icon: string("DisplayIcon").and_then(|value| icon_path(&value)),
        command,
        quiet_command: string("QuietUninstallString"),
        msi: (flag("WindowsInstaller") && is_product_code(subkey)).then(|| subkey.to_owned()),
        package: None,
        protected: flag("NoRemove"),
        portable: false,
        installer,
        source_stamp: 0,
    })
}

fn icon_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    let candidate = trimmed
        .rsplit_once(',')
        .filter(|(_, index)| index.trim().chars().all(|digit| digit.is_ascii_digit()))
        .map_or(trimmed, |(path, _)| path);
    expand(candidate.trim().trim_matches('"'))
        .ok()
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
}

fn install_date(value: &str) -> Option<String> {
    let [year, month, day] = if value.len() == 8 && value.bytes().all(|byte| byte.is_ascii_digit())
    {
        [&value[..4], &value[4..6], &value[6..]]
    } else {
        return None;
    };
    let y: u32 = year.parse().ok()?;
    let last_day = match month {
        "01" | "03" | "05" | "07" | "08" | "10" | "12" => 31,
        "04" | "06" | "09" | "11" => 30,
        "02" if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) => 29,
        "02" => 28,
        _ => return None,
    };
    (y > 0 && (1..=last_day).contains(&day.parse::<u32>().ok()?))
        .then(|| format!("{year}-{month}-{day}"))
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

fn raw_user_key(parent: &Key, name: &str, access: u32) -> io::Result<Key> {
    let mut handle = null_mut();
    let name = wide(name);
    let result = unsafe {
        RegOpenKeyExW(
            parent.as_raw(),
            name.as_ptr(),
            REG_OPTION_OPEN_LINK,
            access,
            &mut handle,
        )
    };
    if result != 0 {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let key = unsafe { Key::from_raw(handle) };
    if key.get_type("SymbolicLinkValue").ok() == Some(Type::Other(REG_LINK)) {
        return Err(io::Error::other("Registry symbolically linked key"));
    }
    Ok(key)
}

fn values(key: &Key) -> Option<Vec<RawValue>> {
    if key.keys().ok()?.next().is_some() {
        return None;
    }
    let mut snapshot = Vec::new();
    let mut size = 0;
    for (name, value) in key.values().ok()? {
        size += name.len() + value.len();
        if size > MAX_VALUE_BYTES || snapshot.len() >= 128 || value.ty() == Type::Other(REG_LINK) {
            return None;
        }
        snapshot.push(RawValue::new(name, value));
    }
    snapshot.sort_by(|a, b| a.name.cmp(&b.name));
    Some(snapshot)
}

pub(crate) fn user_path(path: &str, access: u32) -> io::Result<Key> {
    let mut components = path.split('\\');
    let mut key = raw_user_key(
        CURRENT_USER,
        components.next().unwrap_or_default(),
        KEY_READ,
    )?;
    let mut parts = components.peekable();
    while let Some(part) = parts.next() {
        key = raw_user_key(
            &key,
            part,
            if parts.peek().is_none() {
                access
            } else {
                KEY_READ
            },
        )?;
    }
    Ok(key)
}

pub(crate) fn vendor_key(key: &str) -> bool {
    let parts: Vec<_> = key.split('\\').collect();
    parts.len() == 3
        && parts[0].eq_ignore_ascii_case("Software")
        && parts[1..].iter().all(|name| {
            component(name) == Some(*name)
                && ![
                    "microsoft",
                    "windows",
                    "policies",
                    "classes",
                    "wow6432node",
                    "sakurauninstaller",
                ]
                .contains(&name.to_ascii_lowercase().as_str())
        })
}

pub(crate) fn registry_key(hive: Hive, path: &str) -> Option<Integration> {
    let key = if hive == Hive::User {
        user_path(path, KEY_READ)
    } else {
        hive.key(path)
    }
    .ok()?;
    Some(Integration::Registry {
        hive,
        key: path.into(),
        values: values(&key)?,
    })
}

pub(crate) fn scan_vendor(
    app: &App,
    installed: &[App],
    profile: Profile,
    findings: &mut Vec<Integration>,
) {
    if let (Some(vendor), Some(product)) = (component(&app.publisher), component(&app.name)) {
        let path = format!(r"Software\{vendor}\{product}");
        let shared = installed.iter().any(|other| {
            other.id != app.id
                && other.publisher.eq_ignore_ascii_case(vendor)
                && other.name.eq_ignore_ascii_case(product)
        });
        if vendor_key(&path) && !shared {
            let hives: &[_] = if profile == Profile::Deep {
                &[Hive::User, Hive::Machine32, Hive::Machine64]
            } else {
                &[Hive::User]
            };
            findings.extend(hives.iter().filter_map(|hive| registry_key(*hive, &path)));
        }
    }
}

#[cfg(test)]
#[path = "../../tests/integration/registry_inventory.rs"]
mod tests;
