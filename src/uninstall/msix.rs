use std::{
    io,
    path::{Path, PathBuf},
};

use windows_api::{
    ApplicationModel::{Package, PackageSignatureKind},
    Management::Deployment::{PackageManager, PackageTypes},
    Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    core::HSTRING,
};

use crate::scanner::installed_apps::App;

struct Apartment;

impl Apartment {
    fn open() -> io::Result<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(io::Error::other)?;
        Ok(Self)
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { RoUninitialize() };
    }
}

pub fn list() -> io::Result<Vec<App>> {
    let _apartment = Apartment::open()?;
    let manager = PackageManager::new().map_err(io::Error::other)?;
    let packages = manager
        .FindPackagesByUserSecurityIdWithPackageTypes(&HSTRING::new(), PackageTypes::Main)
        .map_err(io::Error::other)?;
    Ok(packages
        .into_iter()
        .filter_map(|package| entry(&package))
        .collect())
}

fn entry(package: &Package) -> Option<App> {
    let id = package.Id().ok()?;
    let name = id.Name().ok()?.to_string_lossy();
    let full = id.FullName().ok()?.to_string_lossy();
    let version = id.Version().ok()?;
    let display = package
        .DisplayName()
        .ok()
        .map(|name| name.to_string_lossy());
    let signature = package.SignatureKind().ok();
    let core = [
        "Microsoft.WindowsStore",
        "Microsoft.DesktopAppInstaller",
        "Microsoft.StorePurchaseApp",
        "Microsoft.SecHealthUI",
    ]
    .iter()
    .any(|essential| name.eq_ignore_ascii_case(essential));
    Some(App {
        id: format!("MSIX\\{}", full.to_lowercase()),
        name: display
            .filter(|label| !label.is_empty() && !label.starts_with("ms-resource:"))
            .unwrap_or(name),
        publisher: package
            .PublisherDisplayName()
            .map(|value| value.to_string_lossy())
            .unwrap_or_default(),
        version: format!(
            "{}.{}.{}.{}",
            version.Major, version.Minor, version.Build, version.Revision
        ),
        install_date: None,
        size_kib: None,
        location: None,
        icon: logo(package),
        command: None,
        quiet_command: None,
        msi: None,
        package: Some(full),
        protected: core || signature.is_none() || signature == Some(PackageSignatureKind::System),
        portable: false,
        installer: "MSIX".into(),
        source_stamp: 0,
    })
}

/// Resolves the package icon to the best real image on disk.
///
/// The manifest points at plain names (`Assets\Logo.png`) while the disk only holds
/// resource-qualified variants, and several inbox packages ship 1x1 placeholders, so
/// every candidate is checked for its real pixel size.
fn logo(package: &Package) -> Option<PathBuf> {
    let root = package
        .InstalledPath()
        .ok()
        .map(|path| PathBuf::from(path.to_string_lossy()));
    let mut declared: Vec<PathBuf> = Vec::new();
    if let Some(root) = &root {
        for name in manifest_logos(&root.join("AppxManifest.xml")) {
            declared.push(resolve(&name, root));
        }
    }
    let mut located = None;
    if let Ok(uri) = package.Logo()
        && let Ok(raw) = uri.RawUri()
    {
        let raw = raw.to_string_lossy();
        let absolute = PathBuf::from(&raw);
        declared.push(match &root {
            Some(root) => resolve(&raw, root),
            None => absolute.clone(),
        });
        located = absolute.parent().map(Path::to_path_buf);
    }

    let mut best: Option<((u32, u32), PathBuf)> = None;
    for target in declared {
        for candidate in variants(&target) {
            let Some(size) = usable(&candidate) else {
                continue;
            };
            let area = size.0.saturating_mul(size.1);
            if best
                .as_ref()
                .is_none_or(|(known, _)| area > known.0.saturating_mul(known.1))
            {
                best = Some((size, candidate));
            }
        }
    }
    best.map(|(_, path)| path)
        .or_else(|| root.as_deref().and_then(scan))
        .or_else(|| located.as_deref().and_then(executable))
}

/// Sparse packages keep their payload outside `WindowsApps`: fall back to the launcher
/// next to the declared logo, whose shell icon is the real app icon.
fn executable(folder: &Path) -> Option<PathBuf> {
    let mut current = Some(folder);
    for _ in 0..3 {
        let folder = current?;
        let mut best: Option<(u64, PathBuf)> = None;
        if let Ok(entries) = std::fs::read_dir(folder) {
            for item in entries.flatten() {
                let path = item.path();
                if !path
                    .extension()
                    .map(|value| value.to_string_lossy().eq_ignore_ascii_case("exe"))
                    .unwrap_or(false)
                {
                    continue;
                }
                let name = path
                    .file_name()
                    .map(|value| value.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                if ["unins", "uninstall", "setup", "update", "helper", "crash"]
                    .iter()
                    .any(|skip| name.contains(skip))
                {
                    continue;
                }
                let size = item.metadata().map(|meta| meta.len()).unwrap_or(0);
                if best.as_ref().is_none_or(|(known, _)| size > *known) {
                    best = Some((size, path));
                }
            }
        }
        if let Some((_, path)) = best {
            return Some(path);
        }
        current = folder.parent();
    }
    None
}

/// Turns a manifest or WinRT reference into a path, resolving `ms-appx://` style URIs
/// and relative asset names against the installed package folder.
fn resolve(reference: &str, root: &Path) -> PathBuf {
    if let Some(relative) = reference
        .strip_prefix("ms-appx://")
        .or_else(|| reference.strip_prefix("ms-appdata://"))
    {
        let asset = relative
            .split_once('/')
            .map(|(_, asset)| asset)
            .unwrap_or(relative);
        return root.join(asset.replace('/', "\\"));
    }
    let path = PathBuf::from(reference);
    if path.is_absolute() {
        return path;
    }
    root.join(reference.replace('/', "\\"))
}

/// Logo files declared in `AppxManifest.xml`, most specific tile first.
fn manifest_logos(manifest: &Path) -> Vec<String> {
    let Ok(bytes) = std::fs::read(manifest) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut logos = Vec::new();
    for attribute in [
        "Square44x44Logo",
        "Square150x150Logo",
        "Square71x71Logo",
        "SmallLogo",
        "Square30x30Logo",
        "Logo",
    ] {
        let needle = format!("{attribute}=\"");
        if let Some(start) = text.find(&needle) {
            let rest = &text[start + needle.len()..];
            if let Some(end) = rest.find('"') {
                logos.push(rest[..end].to_string());
            }
        }
    }
    if let Some(start) = text.find("<Logo>") {
        let rest = &text[start + "<Logo>".len()..];
        if let Some(end) = rest.find("</Logo>") {
            logos.push(rest[..end].to_string());
        }
    }
    logos
}

/// The declared file plus its resource-qualified siblings (`.scale-200`, `.targetsize-48`).
fn variants(target: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if target.is_file() {
        found.push(target.to_path_buf());
    }
    let (stem, extension, parent) = match (target.file_stem(), target.extension(), target.parent())
    {
        (Some(stem), Some(extension), Some(parent)) => (stem, extension, parent),
        _ => return found,
    };
    let stem = stem.to_string_lossy().to_ascii_lowercase();
    let extension = extension.to_string_lossy().to_ascii_lowercase();
    let mut siblings: Vec<PathBuf> = std::fs::read_dir(parent)
        .map(|entries| {
            entries
                .flatten()
                .map(|item| item.path())
                .collect::<Vec<PathBuf>>()
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|candidate| {
            candidate
                .extension()
                .map(|value| value.to_string_lossy().to_ascii_lowercase() == extension)
                .unwrap_or(false)
                && candidate
                    .file_stem()
                    .map(|value| {
                        value
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .starts_with(&stem)
                    })
                    .unwrap_or(false)
        })
        .collect();
    siblings.sort();
    for sibling in siblings {
        if !found.contains(&sibling) {
            found.push(sibling);
        }
    }
    found
}

/// Real pixel size of an icon file, rejecting the 1x1 placeholders inbox packages ship.
fn usable(path: &Path) -> Option<(u32, u32)> {
    let extension = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if !path.is_file() {
        return None;
    }
    if extension == "png" {
        use std::io::Read;
        let mut header = [0u8; 24];
        std::fs::File::open(path)
            .ok()?
            .read_exact(&mut header)
            .ok()?;
        if &header[..8] != b"\x89PNG\r\n\x1a\n" {
            return None;
        }
        let width = u32::from_be_bytes([header[16], header[17], header[18], header[19]]);
        let height = u32::from_be_bytes([header[20], header[21], header[22], header[23]]);
        return (width >= 32 && height >= 32).then_some((width, height));
    }
    if matches!(extension.as_str(), "jpg" | "jpeg" | "bmp") {
        let bytes = std::fs::metadata(path).ok()?.len();
        return (bytes > 3000).then_some((1, 1));
    }
    None
}

/// Last resort: the largest app-looking image anywhere inside the package.
fn scan(root: &Path) -> Option<PathBuf> {
    let mut best: Option<((u32, u32), PathBuf)> = None;
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((folder, depth)) = pending.pop() {
        if depth > 2 || visited > 250 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for item in entries.flatten() {
            visited += 1;
            let path = item.path();
            if path.is_dir() {
                pending.push((path, depth + 1));
                continue;
            }
            let name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !name.contains("logo") && !name.contains("tile") {
                continue;
            }
            let Some(size) = usable(&path) else {
                continue;
            };
            let area = size.0.saturating_mul(size.1);
            if best
                .as_ref()
                .is_none_or(|(known, _)| area > known.0.saturating_mul(known.1))
            {
                best = Some((size, path));
            }
        }
    }
    best.map(|(_, path)| path)
}

pub fn remove(full_name: &str) -> io::Result<u32> {
    let _apartment = Apartment::open()?;
    let manager = PackageManager::new().map_err(io::Error::other)?;
    manager
        .RemovePackageAsync(&HSTRING::from(full_name))
        .map_err(io::Error::other)?
        .join()
        .map_err(io::Error::other)?;
    Ok(0)
}
