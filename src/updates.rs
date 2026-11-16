use std::{
    fs::File,
    io::{self, Write},
    path::Path,
};

use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{uninstall::msi::launch_msi, utils::http};

const REPO: &str = "/5aether/SakuraUninstaller/releases";

#[derive(Deserialize)]
struct Envelope {
    payload: String,
    signature: String,
}

fn hex<const N: usize>(text: &str) -> io::Result<[u8; N]> {
    if text.len() != 2 * N || !text.as_bytes().iter().all(u8::is_ascii_hexdigit) {
        return Err(io::Error::other("Invalid signature encoding"));
    }
    let mut bytes = [0; N];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte =
            u8::from_str_radix(&text[index * 2..index * 2 + 2], 16).map_err(io::Error::other)?;
    }
    Ok(bytes)
}

pub(crate) fn key() -> io::Result<VerifyingKey> {
    let key = option_env!("SAKURA_RELEASE_PUBLIC_KEY")
        .unwrap_or_default()
        .trim();
    VerifyingKey::from_bytes(&hex(key)?).map_err(io::Error::other)
}

pub fn configured() -> bool {
    key().is_ok()
}

pub(crate) fn verify<T: DeserializeOwned>(bytes: &[u8], key: &VerifyingKey) -> io::Result<T> {
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(io::Error::other)?;
    let signature = Signature::from_bytes(&hex(&envelope.signature)?);
    key.verify_strict(envelope.payload.as_bytes(), &signature)
        .map_err(io::Error::other)?;
    serde_json::from_str(&envelope.payload).map_err(io::Error::other)
}

#[derive(Clone, Deserialize)]
pub struct Update {
    pub version: String,
    sha256: String,
}

fn version(text: &str) -> Option<[u32; 3]> {
    let mut parts = text.split('.');
    let [major, minor, patch] = [parts.next()?, parts.next()?, parts.next()?];
    if parts.next().is_some()
        || [major, minor, patch]
            .iter()
            .any(|part| part.is_empty() || !part.as_bytes().iter().all(u8::is_ascii_digit))
    {
        return None;
    }
    Some([
        major.parse().ok()?,
        minor.parse().ok()?,
        patch.parse().ok()?,
    ])
}

impl Update {
    fn is_newer(&self) -> bool {
        matches!((version(&self.version), version(env!("CARGO_PKG_VERSION"))), (Some(new), Some(current)) if new > current)
    }
}

pub fn check() -> io::Result<Option<Update>> {
    let verifier = key()?;
    let payload = http::get(
        "github.com",
        &format!("{REPO}/latest/download/update.json"),
        64 * 1024,
    )?;
    let update: Update = verify(&payload, &verifier)?;
    if version(&update.version).is_none() || hex::<32>(&update.sha256).is_err() {
        return Err(io::Error::other("Invalid update manifest"));
    }
    Ok(update.is_newer().then_some(update))
}

pub fn install(update: &Update, folder: &Path) -> io::Result<()> {
    if !update.is_newer() {
        return Err(io::Error::other("Update version is not newer"));
    }
    let relative = format!(
        "{REPO}/download/v{}/SakuraUninstaller-x64.msi",
        update.version
    );
    let payload = http::get("github.com", &relative, 64 * 1024 * 1024)?;
    let actual = Sha256::digest(&payload);
    if actual[..] != hex::<32>(&update.sha256)? {
        return Err(io::Error::other(
            "MSI hash does not match the signed manifest",
        ));
    }
    let target = folder.join(format!("update-{}.msi", Uuid::new_v4()));
    let mut file = File::create_new(&target)?;
    file.write_all(&payload)?;
    file.sync_all()?;
    drop(file);
    launch_msi(&target)
}

#[cfg(test)]
#[path = "../tests/unit/update_signatures.rs"]
mod tests;
