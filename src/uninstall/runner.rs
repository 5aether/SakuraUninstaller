use super::{
    elevation, inno_setup, msi, msix, nsis, processes,
    review::{ItemState, Progress},
    session::{BatchControl, ExitStatus, verify_removal},
};
use crate::{
    reporting::report::Message,
    safety::quarantine::Saved,
    scanner::{
        filesystem_scanner,
        installed_apps::{self as inventory, App},
        leftovers::{self, Candidate, Profile},
    },
    storage::Locations,
    updates,
    utils::{
        paths::{comparable, is_local_absolute, system_directory},
        strings::expand,
    },
};
use std::{ffi::OsStr, io, path::PathBuf, time::Duration};
use uuid::Uuid;

pub(crate) fn split(command: &str) -> Option<(&str, &str)> {
    if command.contains('\0') {
        return None;
    }
    let command = command.trim();
    if let Some(quoted) = command.strip_prefix('"') {
        let (exe, args) = quoted.split_once('"')?;
        if !args.is_empty() && !args.starts_with(char::is_whitespace) {
            return None;
        }
        return Some((exe, args.trim_start()));
    }
    let lower = command.to_ascii_lowercase();
    let end = lower.match_indices(".exe").find_map(|(start, _)| {
        let end = start + 4;
        (end == lower.len() || lower[end..].starts_with(char::is_whitespace)).then_some(end)
    })?;
    Some((&command[..end], command[end..].trim_start()))
}

#[derive(Debug)]
pub struct Launch {
    pub executable: PathBuf,
    pub arguments: String,
}

pub fn prepare(app: &App, quiet: bool) -> io::Result<Launch> {
    if !app.removable() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Protected or unregistered application",
        ));
    }
    if let Some(code) = app.msi.as_deref() {
        return msi::removal(code, quiet);
    }
    let command = registered_command(app, quiet)?;
    let command = expand(command)?;
    let (exe, arguments) =
        split(&command).ok_or_else(|| io::Error::other("Invalid uninstall command"))?;
    let executable = PathBuf::from(exe);
    let filename = executable
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if !is_local_absolute(&executable)
        || !filename.ends_with(".exe")
        || [
            "cmd.exe",
            "powershell.exe",
            "pwsh.exe",
            "wscript.exe",
            "cscript.exe",
            "mshta.exe",
            "rundll32.exe",
            "regsvr32.exe",
            "msiexec.exe",
            "explorer.exe",
        ]
        .contains(&filename.as_str())
    {
        return Err(io::Error::other("Unsupported uninstall executable"));
    }
    // Refuse paths that resolve to network locations or to the Windows directory.
    let resolved = executable.canonicalize()?;
    let system = system_directory()?
        .parent()
        .ok_or_else(|| io::Error::other("Windows path unavailable"))?
        .canonicalize()?;
    if !is_local_absolute(&resolved)
        || comparable(&resolved).starts_with(comparable(&system))
        || !resolved.is_file()
    {
        return Err(io::Error::other("Unsupported uninstall location"));
    }
    let arguments = if quiet && app.quiet_command.is_none() {
        match app.installer.as_str() {
            "Inno Setup" => inno_setup::quiet_arguments(arguments),
            "NSIS" => nsis::quiet_arguments(arguments),
            _ => return Err(io::Error::other("No registered quiet command")),
        }
    } else {
        arguments.to_owned()
    };
    Ok(Launch {
        executable: resolved,
        arguments,
    })
}

fn registered_command(app: &App, quiet: bool) -> io::Result<&str> {
    let command = if quiet {
        app.quiet_command.as_deref().or_else(|| {
            matches!(app.installer.as_str(), "Inno Setup" | "NSIS")
                .then_some(app.command.as_deref())
                .flatten()
        })
    } else {
        app.command.as_deref()
    }
    .ok_or_else(|| io::Error::other("No registered command for the selected mode"))?;
    if command.trim().is_empty() || command.contains('\0') {
        return Err(io::Error::other("Invalid uninstall command"));
    }
    Ok(command)
}

pub enum Job {
    Refresh,
    Preview(Vec<App>, bool, Profile),
    Uninstall(Vec<App>, bool, Profile, bool),
    Measure(Vec<App>),
    CloseApps(Vec<App>),
    AddPortable(std::path::PathBuf),
    CheckUpdate,
    InstallUpdate(updates::Update),
    SendDiagnostics,
    Quarantine(Vec<Candidate>),
    Restore(Vec<Uuid>),
}

impl Job {
    pub fn starting(&self) -> Message {
        match self {
            Self::Refresh => Message::new(
                "Aggiornamento elenco software…",
                "Refreshing software inventory…",
            ),
            Self::Preview(_, _, _) => Message::new("Preparazione anteprima…", "Preparing dry run…"),
            Self::Uninstall(_, _, _, _) => Message::new(
                "Attendo i disinstallatori ufficiali…",
                "Waiting for the official uninstallers…",
            ),
            Self::Quarantine(_) => Message::new(
                "Verifica e spostamento dei residui…",
                "Verifying and moving leftovers…",
            ),
            Self::Restore(_) => Message::new("Ripristino in corso…", "Restoring…"),
            Self::Measure(_) => Message::new("Calcolo delle dimensioni…", "Measuring disk usage…"),
            Self::CloseApps(_) => Message::new(
                "Richiesta di chiusura delle finestre…",
                "Requesting window closure…",
            ),
            Self::AddPortable(_) => {
                Message::new("Registrazione app portatile…", "Registering portable app…")
            }
            Self::CheckUpdate => Message::new(
                "Ricerca aggiornamenti firmati…",
                "Checking for signed updates…",
            ),
            Self::InstallUpdate(_) => Message::new(
                "Download e verifica dell’aggiornamento…",
                "Downloading and verifying the update…",
            ),
            Self::SendDiagnostics => {
                Message::new("Invio contatori diagnostici…", "Sending diagnostic counts…")
            }
        }
    }
}

#[derive(Default)]
pub struct Outcome {
    pub inventory: Option<Vec<App>>,
    pub saved: Option<Vec<Saved>>,
    pub reviewed: Option<Vec<Candidate>>,
    pub report: Vec<Message>,
    pub status: Option<Message>,
    pub failed: bool,
    pub measured: Option<std::collections::HashMap<String, u64>>,
    pub update: Option<Option<updates::Update>>,
    pub installer_started: bool,
    pub cancelled: bool,
    pub restart_required: bool,
    pub removed_app_ids: Vec<String>,
    pub quarantined: Vec<Candidate>,
}

impl Outcome {
    fn error(&mut self, subject: &str, error: impl std::fmt::Display) {
        self.failed = true;
        let details = format!("{subject}: {error}");
        self.report.push(Message::new(&details, &details));
    }
}

pub fn execute(
    locations: &Locations,
    job: Job,
    previous: Vec<Candidate>,
    control: &BatchControl,
    progress: &mut dyn FnMut(Progress),
) -> Outcome {
    let mut outcome = Outcome::default();
    let previous = match locations.reviews() {
        Ok(saved) => saved,
        Err(error) => {
            outcome.error("reviews.json", error);
            previous
        }
    };
    let mut reviews = previous.clone();
    let record_report = !matches!(&job, Job::Refresh | Job::CheckUpdate);
    let result = match job {
        Job::Refresh => {
            if locations.auto_update() && updates::configured() {
                check_update(&mut outcome, true);
            }
            Ok(())
        }
        Job::CheckUpdate => {
            check_update(&mut outcome, false);
            Ok(())
        }
        Job::SendDiagnostics => {
            let (status, failed) = match crate::reporting::diagnostics::send(locations) {
                Ok(true) => (
                    Message::new("Contatori diagnostici inviati.", "Diagnostic counts sent."),
                    false,
                ),
                Ok(false) => (
                    Message::new(
                        "Nessun contatore da inviare.",
                        "No diagnostic counts to send.",
                    ),
                    false,
                ),
                Err(error) => (
                    Message::new(
                        format!("Invio dei contatori non riuscito. {error}"),
                        format!("Could not send diagnostic counts. {error}"),
                    ),
                    true,
                ),
            };
            return Outcome {
                status: Some(status),
                failed,
                ..Outcome::default()
            };
        }
        Job::InstallUpdate(update) => locations.data_folder().and_then(|folder| {
            updates::install(&update, &folder)?;
            outcome.installer_started = true;
            Ok(())
        }),
        Job::Preview(apps, quiet, profile) => {
            preview(locations, &apps, quiet, profile, &mut outcome)
        }
        Job::Uninstall(apps, quiet, profile, review_leftovers) => uninstall(
            locations,
            &apps,
            UninstallOptions {
                quiet,
                profile,
                review_leftovers,
            },
            &mut reviews,
            &mut outcome,
            control,
            progress,
        ),
        Job::Measure(apps) => {
            use crate::storage::cache::measurement_fingerprint;
            use rayon::prelude::*;
            let results: Vec<_> = apps
                .par_iter()
                .map(|app| {
                    let result = (|| {
                        let fingerprint = measurement_fingerprint(app)?;
                        let bytes = filesystem_scanner::measured_size(app)?;
                        if measurement_fingerprint(app)? != fingerprint {
                            return Err(io::Error::other(
                                "Installation changed during measurement",
                            ));
                        }
                        Ok((fingerprint, bytes))
                    })();
                    (app, result)
                })
                .collect();
            for (app, result) in results {
                match result {
                    Ok((fingerprint, bytes)) => {
                        if let Err(error) = locations.save_measurement(app, &fingerprint, bytes) {
                            outcome.error("sakura.sqlite3", error);
                        }
                        outcome.report.push(Message::new(
                            format!("{} — {bytes} byte misurati", app.name),
                            format!("{} — {bytes} bytes measured", app.name),
                        ));
                    }
                    Err(error) => outcome.error(&app.name, error),
                }
            }
            Ok(())
        }
        Job::AddPortable(path) => locations.add_portable(&path),
        Job::CloseApps(apps) => close_apps(locations, &apps, &mut outcome),
        Job::Quarantine(items) => {
            quarantine(
                locations,
                &items,
                &mut outcome,
                control,
                progress,
                inventory::scan,
            );
            Ok(())
        }
        Job::Restore(ids) => {
            for id in ids {
                match locations.restore(id) {
                    Ok(()) => outcome.report.push(Message::new(
                        format!("{id}: ripristinato"),
                        format!("{id}: restored"),
                    )),
                    Err(error) => outcome.error(&id.to_string(), error),
                }
            }
            Ok(())
        }
    };
    if let Err(error) = result {
        outcome.error("Sakura Uninstaller", error);
    }
    if outcome.installer_started {
        return outcome;
    }
    let cached = locations.inventory_cache();
    match inventory::full_cached(&cached) {
        Ok((mut inventory, warning)) => {
            if let Some(error) = warning {
                outcome.error("Windows MSIX", error);
            }
            let reviewed = leftovers::remaining(locations, &reviews, &inventory);
            if reviewed != previous
                && let Err(error) = locations.save_reviews(&reviewed)
            {
                outcome.error("reviews.json", error);
            }
            outcome.reviewed = Some(reviewed);
            match locations.portables() {
                Ok(portables) => {
                    inventory.extend(portables);
                    inventory.sort_by_cached_key(|app| app.name.to_lowercase());
                }
                Err(error) => outcome.error("Portable", error),
            }
            if let Err(error) = locations.set_inventory_cache(&inventory) {
                outcome.error("sakura.sqlite3", error);
            }
            match locations.measurements(&inventory) {
                Ok(measured) => outcome.measured = Some(measured),
                Err(error) => outcome.error("sakura.sqlite3", error),
            }
            outcome.inventory = Some(inventory);
        }
        Err(error) => {
            outcome.reviewed = Some(Vec::new());
            outcome.error("Windows Registry", error);
        }
    }
    match locations.saved() {
        Ok(saved) => outcome.saved = Some(saved),
        Err(error) => outcome.error("Sakura data", error),
    }
    if record_report && !outcome.report.is_empty() {
        let text = |italian| {
            outcome
                .report
                .iter()
                .map(|line| line.text(italian))
                .collect::<Vec<_>>()
                .join("\n")
        };
        if let Err(error) = locations.save_report(&text(true), &text(false)) {
            outcome.error("report", error);
        }
    }
    if record_report
        && (!outcome.cancelled || outcome.failed)
        && let Err(error) = locations.measure_event(!outcome.failed)
    {
        outcome.error("diagnostic-counts.json", error);
    }
    outcome.finish_status();
    outcome
}

impl Outcome {
    fn finish_status(&mut self) {
        if self.failed {
            self.status = Some(Message::new(
                if self.restart_required {
                    "Alcune operazioni richiedono attenzione ed è richiesto un riavvio. Apri Rapporti."
                } else {
                    "Alcune operazioni richiedono attenzione. Apri Rapporti."
                },
                if self.restart_required {
                    "Some operations need attention and a restart is required. Open Reports."
                } else {
                    "Some operations need attention. Open Reports."
                },
            ));
        } else if self.cancelled {
            self.status = Some(Message::new(
                if self.restart_required {
                    "Coda interrotta. È richiesto un riavvio. Apri Rapporti."
                } else {
                    "Coda interrotta. Apri Rapporti per gli esiti."
                },
                if self.restart_required {
                    "Queue stopped. A restart is required. Open Reports."
                } else {
                    "Queue stopped. Open Reports for the results."
                },
            ));
        } else if self.restart_required {
            self.status = Some(Message::new(
                "È richiesto un riavvio per completare la disinstallazione. Apri Rapporti.",
                "A restart is required to complete uninstallation. Open Reports.",
            ));
        } else if self.status.is_none() && !self.report.is_empty() {
            self.status = Some(Message::new(
                "Operazione completata. Il rapporto è disponibile.",
                "Operation completed. A report is available.",
            ));
        }
    }
}

fn close_apps(locations: &Locations, apps: &[App], outcome: &mut Outcome) -> io::Result<()> {
    let installed = inventory::scan()?;
    let portable = locations.portables()?;
    for selected in apps {
        let fresh = if selected.portable {
            portable.iter().find(|app| *app == selected)
        } else {
            current(&installed, selected).ok()
        };
        let Some(root) =
            fresh.and_then(|app| crate::scanner::leftovers::associated_root(app, &installed))
        else {
            outcome.error(&selected.name, "Installation directory not confirmed");
            continue;
        };
        locations.log("close-app", &selected.name, "pending")?;
        let count = processes::close_visible_windows(&root);
        locations.log("close-app", &selected.name, &count.to_string())?;
        outcome.report.push(Message::new(
            format!(
                "{} — richieste di chiusura inviate a {count} finestre.",
                selected.name
            ),
            format!("{} — close requested for {count} windows.", selected.name),
        ));
    }
    Ok(())
}

fn check_update(outcome: &mut Outcome, automatic: bool) {
    match updates::check() {
        Ok(update) => {
            if let Some(update) = &update {
                outcome.status = Some(Message::new(
                    format!(
                        "Versione {} disponibile. Apri le impostazioni per installarla.",
                        update.version
                    ),
                    format!(
                        "Version {} available. Open Settings to install it.",
                        update.version
                    ),
                ));
            } else if !automatic {
                outcome.status = Some(Message::new(
                    "Sakura è aggiornato.",
                    "Sakura is up to date.",
                ));
            }
            outcome.update = Some(update);
        }
        Err(error) if !automatic => {
            outcome.update = Some(None);
            outcome.error("Update", error);
        }
        Err(_) => (),
    }
}

fn current<'a>(inventory: &'a [App], selected: &App) -> io::Result<&'a App> {
    inventory
        .iter()
        .find(|app| {
            app.id == selected.id
                && app.name == selected.name
                && app.publisher == selected.publisher
                && app.version == selected.version
                && app.command == selected.command
                && app.quiet_command == selected.quiet_command
                && app.msi == selected.msi
                && app.package == selected.package
                && app.location == selected.location
                && app.installer == selected.installer
                && app.removable()
        })
        .ok_or_else(|| io::Error::other("Registration changed. Refresh and review the selection."))
}

enum Operation {
    Exe(Launch),
    Package(String),
}

impl Operation {
    fn prepare(app: &App, quiet: bool) -> io::Result<Self> {
        if !app.removable() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Protected application",
            ));
        }
        match &app.package {
            Some(full_name) => Ok(Self::Package(full_name.clone())),
            None => prepare(app, quiet).map(Self::Exe),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Exe(launch) => {
                format!("\"{}\" {}", launch.executable.display(), launch.arguments)
                    .trim_end()
                    .to_owned()
            }
            Self::Package(name) => format!("MSIX · {name}"),
        }
    }

    fn run(&self) -> io::Result<u32> {
        match self {
            Self::Exe(launch) => elevation::run(launch),
            Self::Package(name) => msix::remove(name),
        }
    }
}

fn preview(
    locations: &Locations,
    apps: &[App],
    quiet: bool,
    profile: Profile,
    outcome: &mut Outcome,
) -> io::Result<()> {
    let installed = inventory::scan()?;
    let portable = locations.portables()?;
    for requested in apps {
        if requested.portable {
            if let Some(app) = portable.iter().find(|app| *app == requested) {
                for item in leftovers::snapshot(locations, app, &installed, profile) {
                    let path = item.path.display();
                    outcome.report.push(Message::new(
                        format!("{} — {path}", app.name),
                        format!("{} — {path}", app.name),
                    ));
                }
                outcome.report.push(Message::new(
                    "App portatile: verranno esaminati solo i residui attribuibili.",
                    "Portable app: only attributable leftovers will be reviewed.",
                ));
            }
            continue;
        }
        let plan = current(&installed, requested)
            .and_then(|app| Operation::prepare(app, quiet).map(|operation| (app, operation)));
        let (app, launch) = match plan {
            Ok(plan) => plan,
            Err(error) => {
                outcome.error(&requested.name, error);
                continue;
            }
        };
        let exe = launch.label();
        outcome.report.push(Message::new(
            format!("{} — disinstallatore: {exe}", app.name),
            format!("{} — uninstaller: {exe}", app.name),
        ));
        let active = processes::running_under(app.location.as_deref());
        if !active.is_empty() {
            outcome.report.push(Message::new(
                format!("Processi da chiudere: {active:?}"),
                format!("Processes to close: {active:?}"),
            ));
        }
        let candidates = leftovers::snapshot(locations, app, &installed, profile);
        if candidates.is_empty() {
            outcome.report.push(Message::new(
                "Nessun elemento idoneo al confronto.",
                "No items eligible for comparison.",
            ));
        }
        for item in candidates {
            let path = item.path.display();
            outcome.report.push(Message::new(
                format!("Elemento da ricontrollare: {path}"),
                format!("Item to recheck: {path}"),
            ));
        }
    }
    outcome.status = Some(Message::new(
        "Anteprima pronta. Apri Rapporti.",
        "Dry run ready. Open Reports.",
    ));
    Ok(())
}

struct UninstallOptions {
    quiet: bool,
    profile: Profile,
    review_leftovers: bool,
}

fn report_unstarted(outcome: &mut Outcome, count: usize) {
    if count != 0 {
        outcome.report.push(Message::new(
            format!("Programmi non avviati: {count}."),
            format!("Programs not started: {count}."),
        ));
    }
}

fn batch_progress(
    progress: &mut dyn FnMut(Progress),
    index: usize,
    total: usize,
    app: &App,
    stage: Message,
) {
    progress(Progress::for_app(
        Message::new(
            format!(
                "{}/{total} · {} — {}",
                index + 1,
                app.name,
                stage.text(true)
            ),
            format!(
                "{}/{total} · {} — {}",
                index + 1,
                app.name,
                stage.text(false)
            ),
        ),
        app,
    ));
}

fn inventory_for(app: &App) -> io::Result<Vec<App>> {
    let (installed, warning) = inventory::full()?;
    if app.package.is_some()
        && let Some(error) = warning
    {
        return Err(io::Error::other(format!(
            "MSIX inventory unavailable: {error}"
        )));
    }
    Ok(installed)
}

fn uninstall(
    locations: &Locations,
    apps: &[App],
    options: UninstallOptions,
    reviews: &mut Vec<Candidate>,
    outcome: &mut Outcome,
    control: &BatchControl,
    progress: &mut dyn FnMut(Progress),
) -> io::Result<()> {
    let UninstallOptions {
        quiet,
        profile,
        review_leftovers,
    } = options;
    let portable = locations.portables()?;
    for (index, requested) in apps.iter().enumerate() {
        if control.stop_requested() {
            outcome.cancelled = true;
            report_unstarted(outcome, apps.len() - index);
            break;
        }
        batch_progress(
            progress,
            index,
            apps.len(),
            requested,
            Message::new("verifica e backup…", "checking and backing up…"),
        );
        let installed = match inventory_for(requested) {
            Ok(installed) => installed,
            Err(error) => {
                outcome.error(&requested.name, error);
                report_unstarted(outcome, apps.len() - index);
                break;
            }
        };
        if requested.portable {
            let Some(app) = portable.iter().find(|app| *app == requested) else {
                outcome.error(
                    &requested.name,
                    "Portable registration changed. Refresh and review the selection.",
                );
                continue;
            };
            if !review_leftovers {
                outcome.report.push(Message::new(
                    format!("{} — app portatile senza disinstallatore.", requested.name),
                    format!("{} — portable app with no uninstaller.", requested.name),
                ));
                continue;
            }
            let candidates = leftovers::snapshot(locations, app, &installed, profile);
            let count = candidates.len();
            reviews.retain(|item| item.app_id != requested.id);
            reviews.extend(candidates);
            outcome.report.push(Message::new(
                format!(
                    "{} — app portatile, {count} elementi da esaminare.",
                    app.name
                ),
                format!("{} — portable app, {count} items to review.", app.name),
            ));
            continue;
        }
        let result = current(&installed, requested).and_then(|app| {
            let launch = Operation::prepare(app, quiet)?;
            let active = processes::running_under(app.location.as_deref());
            if !active.is_empty() {
                outcome.failed = true;
                outcome.report.push(Message::new(
                    format!("{} — chiudi i processi {active:?} e riprova.", app.name),
                    format!("{} — close processes {active:?} and retry.", app.name),
                ));
                return Ok(None);
            }
            let before = leftovers::snapshot(locations, app, &installed, profile);
            locations.backup_before_uninstall(&before)?;
            if control.stop_requested() {
                return Ok(None);
            }
            locations.log("uninstall", &app.name, "pending")?;
            batch_progress(
                progress,
                index,
                apps.len(),
                app,
                Message::new("disinstallazione in corso…", "uninstalling…"),
            );
            let exit = launch.run();
            let record = match &exit {
                Ok(code) => format!("exit {code}"),
                Err(error) => error.to_string(),
            };
            if let Err(error) = locations.log("uninstall", &app.name, &record) {
                outcome.error(&app.name, error);
            }
            exit.map(|code| Some((code, before)))
        });
        match result {
            Ok(Some((code, before))) => match ExitStatus::classify(code, requested.msi.is_some()) {
                ExitStatus::Completed | ExitStatus::RestartRequired => {
                    let restart = code == 3010;
                    outcome.restart_required |= restart;
                    if restart {
                        outcome.report.push(Message::new(
                            format!(
                                "{} — Windows Installer richiede il riavvio (codice {code}).",
                                requested.name
                            ),
                            format!(
                                "{} — Windows Installer requires a restart (code {code}).",
                                requested.name
                            ),
                        ));
                    }
                    batch_progress(
                        progress,
                        index,
                        apps.len(),
                        requested,
                        Message::new("verifica della rimozione…", "verifying removal…"),
                    );
                    let after = match verify_removal(
                        requested,
                        || inventory::full_cached(&installed),
                        if restart { 1 } else { 31 },
                        Duration::from_millis(500),
                    ) {
                        Ok(after) => after,
                        Err(error) => {
                            outcome.error(&requested.name, error);
                            continue;
                        }
                    };
                    let Some(after) = after else {
                        if restart {
                            outcome.report.push(Message::new(
                            format!("{} — rimozione da verificare dopo il riavvio; residui non proposti.", requested.name),
                            format!("{} — verify removal after restarting; no leftovers proposed.", requested.name),
                        ));
                            continue;
                        }
                        outcome.failed = true;
                        outcome.report.push(Message::new(
                        format!(
                            "{} — il disinstallatore ha terminato, ma la voce è ancora registrata. Aggiorna l’elenco o riprova; residui non proposti.",
                            requested.name
                        ),
                        format!(
                            "{} — the uninstaller exited, but the entry is still registered. Refresh or retry; no leftovers proposed.",
                            requested.name
                        ),
                    ));
                        continue;
                    };
                    outcome.removed_app_ids.push(requested.id.clone());
                    if review_leftovers {
                        batch_progress(
                            progress,
                            index,
                            apps.len(),
                            requested,
                            Message::new("ricerca dei residui…", "scanning leftovers…"),
                        );
                        let remaining = leftovers::remaining(locations, &before, &after);
                        let count = remaining.len();
                        reviews.retain(|item| item.app_id != requested.id);
                        reviews.extend(remaining);
                        outcome.report.push(Message::new(
                            format!(
                                "{} — voce rimossa, {count} elementi da esaminare.",
                                requested.name
                            ),
                            format!(
                                "{} — entry removed, {count} items to review.",
                                requested.name
                            ),
                        ));
                    } else {
                        outcome.report.push(Message::new(
                            format!("{} — voce rimossa.", requested.name),
                            format!("{} — entry removed.", requested.name),
                        ));
                    }
                }
                ExitStatus::RestartInitiated => {
                    outcome.restart_required = true;
                    outcome.report.push(Message::new(
                        format!(
                            "{} — Windows Installer ha avviato il riavvio. Coda interrotta.",
                            requested.name
                        ),
                        format!(
                            "{} — Windows Installer initiated a restart. Queue stopped.",
                            requested.name
                        ),
                    ));
                    report_unstarted(outcome, apps.len() - index - 1);
                    break;
                }
                ExitStatus::Cancelled => {
                    outcome.cancelled = true;
                    outcome.report.push(Message::new(
                        format!("{} — disinstallazione annullata.", requested.name),
                        format!("{} — uninstall cancelled.", requested.name),
                    ));
                    report_unstarted(outcome, apps.len() - index - 1);
                    break;
                }
                ExitStatus::Failed(code) => {
                    outcome.failed = true;
                    outcome.report.push(Message::new(
                        format!(
                            "{} — il disinstallatore ha restituito il codice {code}.",
                            requested.name
                        ),
                        format!(
                            "{} — the uninstaller returned exit code {code}.",
                            requested.name
                        ),
                    ));
                    if requested.msi.is_some() && code == 1618 {
                        outcome.report.push(Message::new(
                        "Un’altra installazione è in corso. Attendi che termini e riprova. Coda interrotta.",
                        "Another installation is in progress. Wait for it to finish and retry. Queue stopped.",
                    ));
                        report_unstarted(outcome, apps.len() - index - 1);
                        break;
                    }
                }
            },
            Ok(None) if control.stop_requested() => {
                outcome.cancelled = true;
                report_unstarted(outcome, apps.len() - index);
                break;
            }
            Ok(None) => (),
            Err(error) if error.raw_os_error() == Some(1223) => {
                outcome.cancelled = true;
                outcome.report.push(Message::new(
                    "Richiesta di Windows annullata.",
                    "Windows request cancelled.",
                ));
                report_unstarted(outcome, apps.len() - index - 1);
                break;
            }
            Err(error) => outcome.error(&requested.name, error),
        }
    }
    if outcome.cancelled {
        outcome.report.push(Message::new(
            "Coda di disinstallazione interrotta.",
            "Uninstall queue stopped.",
        ));
    }
    Ok(())
}

fn quarantine(
    locations: &Locations,
    items: &[Candidate],
    outcome: &mut Outcome,
    control: &BatchControl,
    progress: &mut dyn FnMut(Progress),
    mut scan: impl FnMut() -> io::Result<Vec<App>>,
) {
    for (index, item) in items.iter().enumerate() {
        if control.stop_requested() {
            outcome.cancelled = true;
            let count = items.len() - index;
            outcome.report.push(Message::new(
                format!("Pulizia interrotta. Elementi non elaborati: {count}."),
                format!("Cleanup stopped. Items not processed: {count}."),
            ));
            break;
        }
        let path = item.path.to_string_lossy();
        progress(Progress::for_item(
            Message::new(
                format!("{}/{} · {path}", index + 1, items.len()),
                format!("{}/{} · {path}", index + 1, items.len()),
            ),
            item,
            ItemState::Cleaning,
        ));
        let result = scan().and_then(|inventory| locations.quarantine(item, &inventory));
        match result {
            Ok(receipt) => {
                let summary = receipt.compression;
                let mut it = format!("{path} — spostato in quarantena");
                let mut en = format!("{path} — moved to quarantine");
                if summary.compressed != 0 {
                    it.push_str(&format!(" · file compressi: {}", summary.compressed));
                    en.push_str(&format!(" · compressed files: {}", summary.compressed));
                }
                if summary.skipped != 0 {
                    it.push_str(" · compressione parziale, file disponibili");
                    en.push_str(" · partial compression, files available");
                }
                let message = Message::new(it, en);
                progress(Progress::for_item(
                    message.clone(),
                    item,
                    ItemState::Quarantined,
                ));
                outcome.report.push(message);
                outcome.quarantined.push(receipt.item);
            }
            Err(error) => {
                let details = format!("{path}: {error}");
                progress(Progress::for_item(
                    Message::new(&details, &details),
                    item,
                    ItemState::Failed,
                ));
                outcome.error(&path, error);
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/command_lines.rs"]
mod tests;
