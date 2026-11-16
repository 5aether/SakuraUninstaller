use crate::{
    AppWindow, Entry,
    reporting::report::Message,
    scanner::{
        installed_apps::App,
        leftovers::{Candidate, Evidence, Integration, Profile},
    },
    state::{Shared, State},
    storage::Locations,
    uninstall::runner::{self as jobs, Job, Outcome},
    updates,
    utils::{
        icons, paths,
        strings::{self, localized, matches, size},
        window_controls,
    },
};
use slint::{CloseRequestResponse, ComponentHandle, ModelRc, VecModel};
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
};

fn page(ui: &AppWindow) -> usize {
    usize::try_from(ui.get_page()).unwrap_or(0).min(2)
}

fn item_id(item: &Candidate) -> String {
    item.path.to_string_lossy().into_owned()
}

fn initial(name: &str) -> String {
    name.trim()
        .chars()
        .next()
        .map(|letter| letter.to_uppercase().to_string())
        .unwrap_or_default()
}

fn draw(ui: &AppWindow, state: &mut State) {
    let italian = ui.get_italian();
    let tab = page(ui);
    let selection = &state.selection[tab];
    let mut quiet_supported = false;
    let rows = match tab {
        0 => {
            let query = ui.get_search_text().to_lowercase();
            let selected = state
                .apps
                .iter()
                .filter(|(app, _)| selection.contains(&app.id));
            quiet_supported =
                !selection.is_empty() && selected.into_iter().all(|(app, _)| app.quiet_supported());
            ui.set_only_portable(
                !selection.is_empty()
                    && state
                        .apps
                        .iter()
                        .filter(|(app, _)| selection.contains(&app.id))
                        .all(|(app, _)| app.portable),
            );
            state
                .apps
                .iter()
                .filter(|(_, index)| matches(&query, index))
                .filter(|(app, _)| match ui.get_filter_index() {
                    1 => app.removable(),
                    2 => app.package.is_some(),
                    3 => app.portable,
                    _ => true,
                })
                .map(|(app, _)| {
                    let mut details = [app.publisher.as_str(), app.version.as_str()]
                        .into_iter()
                        .filter(|text| !text.is_empty())
                        .collect::<Vec<_>>();
                    if !app.selectable() {
                        details.push(localized(
                            italian,
                            "Rimozione non disponibile",
                            "Removal unavailable",
                        ));
                    }
                    if !app.installer.is_empty() {
                        details.push(&app.installer);
                    }
                    if let Some(date) = &app.install_date {
                        details.push(date);
                    }
                    Entry {
                        id: app.id.as_str().into(),
                        title: app.name.as_str().into(),
                        description: details.join(" · ").into(),
                        initial: initial(&app.name).into(),
                        icon: icons::image(app.icon_file().as_deref()),
                        size: state
                            .measured
                            .get(&app.id)
                            .map(|bytes| size(*bytes, italian))
                            .or_else(|| {
                                app.size_kib.map(|kib| {
                                    format!("≈ {}", size(kib.saturating_mul(1024), italian))
                                })
                            })
                            .unwrap_or_default()
                            .into(),
                        selectable: app.selectable(),
                        selected: selection.contains(&app.id),
                    }
                })
                .collect::<Vec<_>>()
        }
        1 => state
            .reviews
            .iter()
            .map(|item| {
                let source = match item.evidence {
                    Evidence::InstallLocation => localized(
                        italian,
                        "Posizione di installazione registrata",
                        "Registered install location",
                    ),
                    Evidence::UserData => localized(
                        italian,
                        "Nome esatto nei dati utente · confidenza media",
                        "Exact user-data match · medium confidence",
                    ),
                    Evidence::WindowsIntegration => match &item.integration {
                        Some(Integration::Registry { .. }) => localized(
                            italian,
                            "Registro · editore e nome esatti",
                            "Registry · exact publisher and product",
                        ),
                        Some(Integration::Startup { .. }) => localized(
                            italian,
                            "Avvio · percorso eseguibile",
                            "Startup · executable path",
                        ),
                        Some(Integration::Shortcut { .. }) => localized(
                            italian,
                            "Collegamento · destinazione verificata",
                            "Shortcut · matched target",
                        ),
                        Some(Integration::Service { .. }) => localized(
                            italian,
                            "Servizio · percorso eseguibile",
                            "Service · executable path",
                        ),
                        Some(Integration::Task { .. }) => localized(
                            italian,
                            "Attività pianificata · azione verificata",
                            "Scheduled task · matched action",
                        ),
                        None => localized(italian, "Elemento Windows", "Windows item"),
                    },
                };
                let id = item_id(item);
                let mode = if item.writable() {
                    ""
                } else {
                    localized(italian, " · solo rapporto", " · report only")
                };
                Entry {
                    selected: selection.contains(&id),
                    id: id.into(),
                    title: item.path.to_string_lossy().as_ref().into(),
                    description: format!("{} · {source}{mode}", item.app).into(),
                    initial: initial(&item.app).into(),
                    icon: icons::image(Some(&item.path)),
                    size: if item.integration.is_some() {
                        String::new()
                    } else {
                        size(item.bytes, italian)
                    }
                    .into(),
                    selectable: item.writable(),
                }
            })
            .collect(),
        _ => state
            .saved
            .iter()
            .map(|saved| {
                let id = saved.id.to_string();
                Entry {
                    selected: selection.contains(&id),
                    id: id.into(),
                    title: saved.item.path.to_string_lossy().as_ref().into(),
                    description: saved.item.app.as_str().into(),
                    initial: initial(&saved.item.app).into(),
                    icon: icons::image(Some(&saved.item.path)),
                    size: if saved.item.integration.is_some() {
                        String::new()
                    } else {
                        size(saved.item.bytes, italian)
                    }
                    .into(),
                    selectable: true,
                }
            })
            .collect(),
    };

    state.visible = rows
        .iter()
        .filter(|row| row.selectable)
        .map(|row| row.id.to_string())
        .collect();
    ui.set_available_count(rows.len() as i32);
    ui.set_selected_count(selection.len() as i32);
    ui.set_residue_count(state.reviews.len() as i32);
    ui.set_saved_count(state.saved.len() as i32);
    ui.set_entries(ModelRc::new(VecModel::from(rows)));
    ui.set_silent_supported(quiet_supported);
    if tab == 0 && !quiet_supported {
        ui.set_silent(false);
    }
    ui.set_status_error(state.failed);
    ui.set_status(
        state
            .status
            .as_ref()
            .map_or("", |status| status.text(italian))
            .into(),
    );
}

fn apply(ui: &AppWindow, shared: &Shared, outcome: Outcome) {
    if outcome.installer_started {
        ui.set_busy(false);
        let _ = ui.hide();
        return;
    }
    let mut state = shared.lock().unwrap();
    if let Some(update) = outcome.update {
        state.update = update;
        ui.set_update_version(
            state
                .update
                .as_ref()
                .map_or("", |update| update.version.as_str())
                .into(),
        );
    }
    if let Some(measured) = outcome.measured {
        state.measured = measured;
    }
    if let Some(apps) = outcome.inventory {
        let allowed: HashSet<_> = apps
            .iter()
            .filter(|app| app.selectable())
            .map(|app| &app.id)
            .collect();
        state.selection[0].retain(|id| allowed.contains(id));
        state.apps = apps
            .into_iter()
            .map(|app| {
                let index = app.search_text();
                (app, index)
            })
            .collect();
    }
    if let Some(reviews) = outcome.reviewed {
        let ids: HashSet<_> = reviews
            .iter()
            .filter(|item| item.writable())
            .map(item_id)
            .collect();
        state.selection[1].retain(|id| ids.contains(id));
        state.reviews = reviews;
    }
    if let Some(saved) = outcome.saved {
        let ids: HashSet<_> = saved.iter().map(|entry| entry.id.to_string()).collect();
        state.selection[2].retain(|id| ids.contains(id));
        state.saved = saved;
    }
    state.status = outcome.status;
    if !outcome.report.is_empty() {
        state.report = outcome.report;
        ui.set_has_report(true);
    }
    state.failed = outcome.failed;
    draw(ui, &mut state);
    ui.set_busy(false);
}

fn start(ui: &AppWindow, shared: &Shared, job: Job) {
    if ui.get_busy() {
        return;
    }
    let (locations, reviews) = {
        let mut state = shared.lock().unwrap();
        state.status = Some(job.starting());
        state.failed = false;
        ui.set_busy(true);
        draw(ui, &mut state);
        (state.locations.clone(), state.reviews.clone())
    };
    let weak = ui.as_weak();
    let shared = shared.clone();
    std::thread::spawn(move || {
        let outcome = jobs::execute(&locations, job, reviews);
        let _ = weak.upgrade_in_event_loop(move |ui| apply(&ui, &shared, outcome));
    });
}

fn selected_apps(state: &State) -> Vec<App> {
    state
        .apps
        .iter()
        .filter(|(app, _)| app.selectable() && state.selection[0].contains(&app.id))
        .map(|(app, _)| app.clone())
        .collect()
}

fn prepare(ui: &AppWindow, state: &mut State) {
    let index = page(ui);
    let italian = ui.get_italian();
    let chosen = &state.selection[index];
    let (job, summary, labels) = match index {
        0 => {
            let items = selected_apps(state);
            let labels = items.iter().map(|app| app.name.clone()).collect::<Vec<_>>();
            let summary = if items.iter().all(|app| app.portable) {
                localized(
                    italian,
                    "Verranno cercati dati utente e integrazioni attribuibili. La cartella del programma rimane disponibile.",
                    "Sakura will look for attributable user data and integrations. The application folder stays available.",
                )
            } else if ui.get_silent() {
                localized(
                    italian,
                    "Sakura avvierà i disinstallatori ufficiali in modalità silenziosa.",
                    "Sakura will run the official uninstallers in quiet mode.",
                )
            } else {
                localized(
                    italian,
                    "Sakura avvierà i disinstallatori ufficiali. Potrebbero chiederti una conferma.",
                    "Sakura will run the official uninstallers. They may ask you to confirm.",
                )
            };
            (
                Job::Uninstall(
                    items,
                    ui.get_silent(),
                    Profile::from_index(ui.get_profile_index()),
                    true,
                ),
                summary,
                labels,
            )
        }
        1 => {
            let items = state
                .reviews
                .iter()
                .filter(|item| item.writable() && chosen.contains(&item_id(item)))
                .cloned()
                .collect::<Vec<_>>();
            let labels = items.iter().map(item_id).collect();
            (
                Job::Quarantine(items),
                localized(
                    italian,
                    "Le cartelle selezionate verranno spostate in quarantena. Potrai ripristinarle.",
                    "Selected folders will move to quarantine. You can restore them.",
                ),
                labels,
            )
        }
        _ => {
            let items = state
                .saved
                .iter()
                .filter(|item| chosen.contains(&item.id.to_string()))
                .collect::<Vec<_>>();
            let labels = items.iter().map(|item| item_id(&item.item)).collect();
            let ids = items.into_iter().map(|item| item.id).collect();
            (
                Job::Restore(ids),
                localized(
                    italian,
                    "Le cartelle verranno ripristinate nella posizione originale se è libera.",
                    "Folders will return to the original location if it is free.",
                ),
                labels,
            )
        }
    };
    let includes_package = matches!(&job, Job::Uninstall(apps, _, _, _) if apps.iter().any(|app| app.package.is_some()));
    ui.set_offer_leftover_review(
        matches!(&job, Job::Uninstall(apps, _, _, _) if apps.iter().any(|app| !app.portable)),
    );
    let mut detail = summary.to_owned();
    if includes_package {
        detail.push_str(localized(
            italian,
            "\n\nPer le app MSIX, Windows può rimuovere anche i dati gestiti dal pacchetto.",
            "\n\nFor MSIX apps, Windows may also remove data managed by the package.",
        ));
    }
    for label in &labels {
        detail.push_str(&format!("\n\n• {label}"));
    }
    ui.set_confirmation_detail(detail.into());
    state.pending = (!labels.is_empty()).then_some(job);
}

pub fn connect(ui: &AppWindow, locations: Locations) {
    ui.set_italian(locations.language().unwrap_or_else(strings::italian));
    ui.set_profile_index(locations.profile().index());
    ui.set_updates_configured(updates::configured());
    ui.set_auto_update(locations.auto_update());
    ui.set_diagnostics(locations.diagnostics());
    ui.set_diagnostics_host(
        crate::reporting::diagnostics::destination()
            .map_or("", |(host, _)| host)
            .into(),
    );
    ui.set_has_report(locations.read_report(0).ok().flatten().is_some());
    let cached = locations.inventory_cache();
    let measured = locations.measurements(&cached).unwrap_or_default();
    let apps = cached
        .into_iter()
        .map(|app| {
            let index = app.search_text();
            (app, index)
        })
        .collect();
    let weak = ui.as_weak();
    ui.window().on_close_requested(move || {
        if weak.upgrade().is_some_and(|window| window.get_busy()) {
            CloseRequestResponse::KeepWindowShown
        } else {
            CloseRequestResponse::HideWindow
        }
    });
    let shared = Arc::new(Mutex::new(State {
        locations,
        apps,
        reviews: Vec::new(),
        saved: Vec::new(),
        selection: Default::default(),
        visible: Vec::new(),
        pending: None,
        status: None,
        report: Vec::new(),
        failed: false,
        measured,
        update: None,
    }));

    macro_rules! bind {
        ($method:ident, |$window:ident, $state:ident $(, $arg:ident)*| $handler:block) => {{
            let weak = ui.as_weak();
            let $state = shared.clone();
            ui.$method(move |$($arg),*| {
                if let Some($window) = weak.upgrade() {
                    $handler
                }
            });
        }};
    }

    bind!(on_load_page, |ui, shared, tab| {
        ui.set_page(tab);
        draw(&ui, &mut shared.lock().unwrap());
    });
    bind!(on_browse_report, |ui, shared, direction| {
        let state = shared.lock().unwrap();
        let offset = match direction {
            -1 | 1 => (ui.get_report_offset() + direction).max(0),
            _ => 0,
        };
        let italian = ui.get_italian();
        let latest = || {
            state
                .report
                .iter()
                .map(|line| line.text(italian))
                .collect::<Vec<_>>()
                .join("\n")
        };
        match state.locations.read_report(offset) {
            Ok(Some((message, date, older))) => {
                let stored = message.text(italian);
                let newest = if offset == 0 { latest() } else { String::new() };
                let contents = if !newest.is_empty() && newest != stored {
                    format!("{newest}\n\n{date}\n{stored}")
                } else {
                    format!("{date}\n{stored}")
                };
                ui.set_report_offset(offset);
                ui.set_report_has_older(older);
                ui.set_report_text(contents.into());
            }
            Ok(None) | Err(_) if offset == 0 => {
                ui.set_report_offset(0);
                ui.set_report_has_older(false);
                ui.set_report_text(latest().into());
            }
            _ => (),
        }
    });
    bind!(on_language_changed, |ui, shared| {
        let italian = !ui.get_italian();
        ui.set_italian(italian);
        let mut state = shared.lock().unwrap();
        if let Err(error) = state.locations.set_language(italian) {
            state.status = Some(Message::new(
                format!("Impossibile salvare la lingua: {error}"),
                format!("Could not save language: {error}"),
            ));
            state.failed = true;
        }
        draw(&ui, &mut state);
    });
    bind!(on_filter_changed, |ui, shared| {
        draw(&ui, &mut shared.lock().unwrap());
    });
    bind!(on_profile_changed, |ui, shared| {
        let mut state = shared.lock().unwrap();
        if let Err(error) = state
            .locations
            .set_profile(Profile::from_index(ui.get_profile_index()))
        {
            state.status = Some(Message::new(
                format!("Impossibile salvare il profilo: {error}"),
                format!("Could not save scan profile: {error}"),
            ));
            state.failed = true;
            draw(&ui, &mut state);
        }
    });
    bind!(on_measure, |ui, shared| {
        let apps = {
            let state = shared.lock().unwrap();
            let selected = selected_apps(&state);
            if selected.is_empty() {
                state
                    .apps
                    .iter()
                    .filter(|(app, _)| app.selectable() && state.visible.contains(&app.id))
                    .map(|(app, _)| app.clone())
                    .collect()
            } else {
                selected
            }
        };
        if !apps.is_empty() {
            start(&ui, &shared, Job::Measure(apps));
        }
    });
    bind!(on_close_apps, |ui, shared| {
        let apps = selected_apps(&shared.lock().unwrap());
        if !apps.is_empty() {
            start(&ui, &shared, Job::CloseApps(apps));
        }
    });
    bind!(on_add_portable, |ui, shared, path| {
        let path = std::path::PathBuf::from(path.trim().trim_matches('"'));
        start(&ui, &shared, Job::AddPortable(path));
    });
    bind!(on_check_update, |ui, shared| {
        if updates::configured() {
            start(&ui, &shared, Job::CheckUpdate);
        }
    });
    bind!(on_install_update, |ui, shared| {
        let update = shared.lock().unwrap().update.clone();
        if let Some(update) = update {
            start(&ui, &shared, Job::InstallUpdate(update));
        }
    });
    bind!(on_update_preference, |ui, shared| {
        let mut state = shared.lock().unwrap();
        if let Err(error) = state.locations.set_auto_update(ui.get_auto_update()) {
            state.status = Some(Message::new(
                format!("Preferenza non salvata: {error}"),
                format!("Unable to save preference: {error}"),
            ));
            state.failed = true;
            draw(&ui, &mut state);
        }
    });
    bind!(on_diagnostics_preference, |ui, shared| {
        let mut state = shared.lock().unwrap();
        if let Err(error) = state.locations.set_diagnostics(ui.get_diagnostics()) {
            state.status = Some(Message::new(
                format!("Preferenza non salvata: {error}"),
                format!("Unable to save preference: {error}"),
            ));
            state.failed = true;
            draw(&ui, &mut state);
        }
    });
    bind!(on_send_diagnostics, |ui, shared| {
        start(&ui, &shared, Job::SendDiagnostics);
    });
    bind!(on_select, |ui, shared, id, choose| {
        if !ui.get_busy() {
            let mut state = shared.lock().unwrap();
            let id = id.to_string();
            if state.visible.contains(&id) {
                let selection = &mut state.selection[page(&ui)];
                if choose {
                    selection.insert(id);
                } else {
                    selection.remove(&id);
                }
                draw(&ui, &mut state);
            }
        }
    });
    bind!(on_select_visible, |ui, shared| {
        let mut state = shared.lock().unwrap();
        let visible = state.visible.clone();
        state.selection[page(&ui)].extend(visible);
        draw(&ui, &mut state);
    });
    bind!(on_clear_selection, |ui, shared| {
        let mut state = shared.lock().unwrap();
        state.selection[page(&ui)].clear();
        draw(&ui, &mut state);
    });
    bind!(on_refresh, |ui, shared| {
        start(&ui, &shared, Job::Refresh);
    });
    bind!(on_preview, |ui, shared| {
        let apps = selected_apps(&shared.lock().unwrap());
        if !apps.is_empty() {
            start(
                &ui,
                &shared,
                Job::Preview(
                    apps,
                    ui.get_silent(),
                    Profile::from_index(ui.get_profile_index()),
                ),
            );
        }
    });
    bind!(on_prepare_action, |ui, shared| {
        prepare(&ui, &mut shared.lock().unwrap());
    });
    bind!(on_confirm_action, |ui, shared, review| {
        let pending = shared.lock().unwrap().pending.take();
        if let Some(mut job) = pending {
            if ui.get_offer_leftover_review()
                && let Job::Uninstall(_, _, _, ref mut review_leftovers) = job
            {
                *review_leftovers = review;
            }
            start(&ui, &shared, job);
        }
    });
    {
        let weak = ui.as_weak();
        ui.on_close_window(move || {
            if let Some(window) = weak.upgrade()
                && !window.get_busy()
            {
                let _ = window.hide();
            }
        });
    }
    ui.on_minimize_window(|| {
        window_controls::minimize();
    });
    ui.on_toggle_maximize_window(window_controls::toggle_maximize);
    bind!(on_open_folder, |ui, shared| {
        let mut state = shared.lock().unwrap();
        if let Err(error) = state
            .locations
            .data_folder()
            .and_then(|path| paths::open_folder(&path))
        {
            state.status = Some(Message::new(
                format!("Impossibile aprire il registro: {error}"),
                format!("Could not open activity log: {error}"),
            ));
            state.failed = true;
            draw(&ui, &mut state);
        }
    });
    start(ui, &shared, Job::Refresh);
}
