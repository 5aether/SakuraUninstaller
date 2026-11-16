use crate::{
    scanner::leftovers::Evidence,
    storage::Locations,
    utils::paths::{comparable, is_local_absolute},
};
use std::path::Path;

impl Locations {
    pub(crate) fn allows(&self, path: &Path, evidence: Evidence) -> bool {
        if !is_local_absolute(path) || self.rules.protects_path(path) {
            return false;
        }
        let path = comparable(path);
        let store = comparable(&self.store);
        if path.starts_with(&store) || store.starts_with(&path) {
            return false;
        }
        let roots = match evidence {
            Evidence::InstallLocation => &self.programs,
            Evidence::UserData => &self.data,
            Evidence::WindowsIntegration => return false,
        };
        roots.iter().any(|root| {
            path.strip_prefix(comparable(root))
                .ok()
                .is_some_and(|suffix| {
                    let segments: Vec<_> = suffix.iter().collect();
                    (1..=2).contains(&segments.len())
                        && segments.iter().all(|part| {
                            let part = part.to_string_lossy();
                            ![
                                "microsoft",
                                "windows",
                                "windowsapps",
                                "modifiablewindowsapps",
                                "common files",
                                "packages",
                                "programs",
                                "temp",
                                "downloads",
                                "assembly",
                                "virtualstore",
                                "sakurauninstaller",
                            ]
                            .contains(&part.as_ref())
                        })
                })
        })
    }
}
