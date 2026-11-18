use crate::scanner::installed_apps::App;
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub(crate) struct BatchControl(Arc<AtomicBool>);

impl BatchControl {
    pub(crate) fn request_stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub(crate) fn stop_requested(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) enum ExitStatus {
    Completed,
    RestartRequired,
    RestartInitiated,
    Cancelled,
    Failed(u32),
}

impl ExitStatus {
    pub(crate) fn classify(code: u32, msi: bool) -> Self {
        match code {
            0 => Self::Completed,
            1223 => Self::Cancelled,
            1602 if msi => Self::Cancelled,
            1614 if msi => Self::Completed,
            1641 if msi => Self::RestartInitiated,
            3010 if msi => Self::RestartRequired,
            code => Self::Failed(code),
        }
    }
}

/// An absent MSIX entry is meaningful only if package enumeration succeeded.
/// Polling also accommodates EXE launchers that exit before their worker does.
pub(crate) fn verify_removal(
    selected: &App,
    mut scan: impl FnMut() -> io::Result<(Vec<App>, Option<io::Error>)>,
    attempts: usize,
    interval: Duration,
) -> io::Result<Option<Vec<App>>> {
    for attempt in 0..attempts.max(1) {
        let (installed, warning) = scan()?;
        if selected.package.is_some()
            && let Some(error) = warning
        {
            return Err(io::Error::other(format!(
                "Cannot verify MSIX removal: {error}"
            )));
        }
        if !installed.iter().any(|app| {
            app.id == selected.id
                || selected.msi.as_ref().is_some_and(|code| {
                    app.msi
                        .as_ref()
                        .is_some_and(|other| code.eq_ignore_ascii_case(other))
                })
        }) {
            return Ok(Some(installed));
        }
        if attempt + 1 < attempts {
            std::thread::sleep(interval);
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "../../tests/unit/uninstall_session.rs"]
mod tests;
