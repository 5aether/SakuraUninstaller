use crate::{
    updates::{key, verify},
    utils::paths::{comparable, is_local_absolute, regular_path},
};
use serde::Deserialize;
use std::{
    io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Rules {
    protected_paths: Vec<PathBuf>,
    protected_integrations: Vec<String>,
}

impl Rules {
    pub fn load(path: &Path) -> io::Result<Self> {
        let metadata = regular_path(path)?;
        if !metadata.is_file() || metadata.len() > 128 * 1024 {
            return Err(io::Error::other("Invalid signed rules size"));
        }
        let rules: Self = verify(&std::fs::read(path)?, &key()?)?;
        if rules
            .protected_paths
            .iter()
            .any(|path| !is_local_absolute(path))
        {
            return Err(io::Error::other("Invalid protected path"));
        }
        Ok(rules)
    }

    pub fn protects_path(&self, path: &Path) -> bool {
        let path = comparable(path);
        self.protected_paths
            .iter()
            .map(|other| comparable(other))
            .any(|other| path.starts_with(&other) || other.starts_with(&path))
    }

    pub fn protects_integration(&self, label: &str) -> bool {
        let label = label.to_lowercase();
        self.protected_integrations
            .iter()
            .map(|name| name.to_lowercase())
            .any(|name| {
                !name.is_empty()
                    && (label == name
                        || label
                            .strip_prefix(&name)
                            .is_some_and(|suffix| suffix.starts_with('\\')))
            })
    }
}
