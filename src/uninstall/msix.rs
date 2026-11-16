use std::io;

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
        icon: None,
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
