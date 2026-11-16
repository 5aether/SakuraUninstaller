use crate::{storage::Locations, utils::http};
use std::io;

pub(crate) fn destination() -> Option<(&'static str, &'static str)> {
    let host = option_env!("SAKURA_TELEMETRY_HOST")?;
    let path = option_env!("SAKURA_TELEMETRY_PATH")?;
    valid_destination(host, path).then_some((host, path))
}

fn valid_destination(host: &str, path: &str) -> bool {
    host.len() <= 253
        && host.contains('.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
        && path.starts_with('/')
        && path.len() <= 1024
        && path[1..].split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
        })
}

fn payload([completed, errors]: [u64; 2]) -> serde_json::Value {
    serde_json::json!({"schema": 1, "completed": completed, "errors": errors})
}

pub(crate) fn send(locations: &Locations) -> io::Result<bool> {
    let (host, path) = destination()
        .filter(|_| locations.diagnostics())
        .ok_or_else(|| io::Error::other("Diagnostic sharing is unavailable"))?;
    let counts = locations.diagnostic_counts()?;
    if counts == [0, 0] {
        return Ok(false);
    }
    let bytes = serde_json::to_vec(&payload(counts)).map_err(io::Error::other)?;
    http::post_json(host, path, &bytes)?;
    Ok(true)
}

#[cfg(test)]
#[path = "../../tests/unit/diagnostics.rs"]
mod tests;
