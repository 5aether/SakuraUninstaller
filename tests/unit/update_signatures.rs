use super::*;
use ed25519_dalek::{Signer, SigningKey};

#[test]
fn signed_updates_reject_changed_payload_and_downgrades() -> io::Result<()> {
    let secret = SigningKey::from_bytes(&[19; 32]);
    let payload = include_str!("../fixtures/update-payload.json").trim();
    let signature = secret
        .sign(payload.as_bytes())
        .to_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let feed = serde_json::json!({"payload": payload, "signature": signature});
    let update: Update = verify(&serde_json::to_vec(&feed)?, &secret.verifying_key())?;
    assert!(update.is_newer());
    let mut changed = feed;
    changed["payload"] = serde_json::Value::String(payload.replace("1.2.3", "1.2.9"));
    assert!(verify::<Update>(&serde_json::to_vec(&changed)?, &secret.verifying_key()).is_err());
    assert!(
        !Update {
            version: env!("CARGO_PKG_VERSION").into(),
            sha256: String::new()
        }
        .is_newer()
    );
    assert!(version(r"1.2.3/../").is_none());
    Ok(())
}
