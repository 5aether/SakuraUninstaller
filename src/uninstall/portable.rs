use crate::{
    scanner::installed_apps::App,
    storage::Locations,
    utils::{
        paths::{comparable, known_folder, regular_path},
        strings::component,
    },
};
use std::{io, path::Path};
use windows_sys::Win32::UI::Shell::FOLDERID_Windows;

impl Locations {
    pub fn portables(&self) -> io::Result<Vec<App>> {
        Ok(self.read_state("portable.json")?.unwrap_or_default())
    }

    pub fn add_portable(&self, path: &Path) -> io::Result<()> {
        let metadata = regular_path(path)?;
        if !metadata.is_file()
            || !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
        {
            return Err(io::Error::other("Select a local portable .exe"));
        }
        let executable = path.canonicalize()?;
        let parent = executable
            .parent()
            .ok_or_else(|| io::Error::other("Portable folder unavailable"))?;
        let name = parent
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(component)
            .ok_or_else(|| io::Error::other("Portable software needs an individual folder"))?
            .to_owned();
        let root = comparable(parent);
        let windows = known_folder(&FOLDERID_Windows)?;
        if root.starts_with(comparable(&windows))
            || root == comparable(&self.store)
            || root.parent().and_then(Path::parent).is_none()
            || self
                .programs
                .iter()
                .chain(&self.data)
                .any(|base| comparable(base) == root)
            || [
                "downloads",
                "desktop",
                "documents",
                "system32",
                "windowsapps",
                "temp",
                "programs",
            ]
            .contains(&name.to_ascii_lowercase().as_str())
        {
            return Err(io::Error::other(
                "Select an executable in a dedicated portable application folder",
            ));
        }
        let id = format!("portable:{}", root.display());
        let mut apps = self.portables()?;
        apps.retain(|app| app.id != id);
        apps.push(App {
            id,
            name,
            publisher: String::new(),
            version: String::new(),
            install_date: None,
            size_kib: None,
            location: Some(parent.to_owned()),
            icon: Some(executable),
            command: None,
            quiet_command: None,
            msi: None,
            package: None,
            protected: false,
            portable: true,
            installer: "Portable".into(),
            source_stamp: 0,
        });
        self.write_state("portable.json", &apps)
    }
}
