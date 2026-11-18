use crate::{
    reporting::report::Message,
    scanner::{
        installed_apps::App,
        leftovers::{Candidate, Integration},
    },
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Category {
    Registry = 0,
    Files = 1,
    Other = 2,
}

pub(crate) fn category(item: &Candidate) -> Category {
    match &item.integration {
        Some(Integration::Registry { .. } | Integration::Startup { .. }) => Category::Registry,
        None | Some(Integration::Shortcut { .. }) => Category::Files,
        Some(Integration::Service { .. } | Integration::Task { .. }) => Category::Other,
    }
}

pub(crate) fn item_key(item: &Candidate) -> String {
    format!(
        "{}:{}{}",
        item.app_id.len(),
        item.app_id,
        item.path.display()
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ItemState {
    Pending = 0,
    Cleaning = 1,
    Quarantined = 2,
    Failed = 3,
}

pub(crate) struct Progress {
    pub message: Message,
    pub app_id: Option<String>,
    pub item: Option<(String, ItemState)>,
}

impl Progress {
    pub(crate) fn for_app(message: Message, app: &App) -> Self {
        Self {
            message,
            app_id: Some(app.id.clone()),
            item: None,
        }
    }

    pub(crate) fn for_item(message: Message, item: &Candidate, state: ItemState) -> Self {
        Self {
            message,
            app_id: None,
            item: Some((item_key(item), state)),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Phase {
    Uninstalling = 0,
    Review = 1,
    Cleaning = 2,
    Summary = 3,
}

pub(crate) struct ReviewItem {
    pub candidate: Candidate,
    pub selected: bool,
    pub state: ItemState,
    pub error: Option<Message>,
}

impl ReviewItem {
    pub(crate) fn selectable(&self) -> bool {
        self.candidate.writable() && self.state != ItemState::Quarantined
    }
}

#[derive(Default)]
pub(crate) struct Totals {
    pub registry: usize,
    pub files: usize,
    pub other: usize,
    pub bytes: u64,
}

impl Totals {
    pub(crate) fn count(&self) -> usize {
        self.registry + self.files + self.other
    }

    fn add(&mut self, item: &Candidate) {
        match category(item) {
            Category::Registry => self.registry += 1,
            Category::Files => {
                self.files += 1;
                self.bytes = self.bytes.saturating_add(item.bytes);
            }
            Category::Other => self.other += 1,
        }
    }
}

pub(crate) struct ReviewSession {
    pub phase: Phase,
    pub app_ids: HashSet<String>,
    pub removed: usize,
    pub cleanup_total: usize,
    pub active_category: Category,
    pub items: Vec<ReviewItem>,
    pub failed: bool,
    pub cancelled: bool,
    pub restart_required: bool,
}

impl ReviewSession {
    pub(crate) fn new(apps: &[App]) -> Self {
        Self {
            phase: Phase::Uninstalling,
            app_ids: apps.iter().map(|app| app.id.clone()).collect(),
            removed: 0,
            cleanup_total: 0,
            active_category: Category::Registry,
            items: Vec::new(),
            failed: false,
            cancelled: false,
            restart_required: false,
        }
    }

    pub(crate) fn finish_uninstall(
        &mut self,
        candidates: &[Candidate],
        removed: &[String],
        failed: bool,
        cancelled: bool,
        restart_required: bool,
    ) {
        self.removed = removed
            .iter()
            .filter(|id| self.app_ids.contains(*id))
            .collect::<HashSet<_>>()
            .len();
        let mut seen = HashSet::new();
        self.items = candidates
            .iter()
            .filter(|item| self.app_ids.contains(&item.app_id) && seen.insert(item_key(item)))
            .cloned()
            .map(|candidate| ReviewItem {
                candidate,
                selected: false,
                state: ItemState::Pending,
                error: None,
            })
            .collect();
        self.items.sort_by(|a, b| {
            (
                category(&a.candidate) as i32,
                &a.candidate.app,
                &a.candidate.path,
            )
                .cmp(&(
                    category(&b.candidate) as i32,
                    &b.candidate.app,
                    &b.candidate.path,
                ))
        });
        self.failed = failed;
        self.cancelled = cancelled;
        self.restart_required = restart_required;
        self.phase = if self.items.is_empty() {
            Phase::Summary
        } else {
            Phase::Review
        };
    }

    pub(crate) fn select(&mut self, id: &str, selected: bool) {
        if self.phase != Phase::Review {
            return;
        }
        for item in &mut self.items {
            if item.selectable() && item_key(&item.candidate) == id {
                item.selected = selected;
            }
        }
    }

    pub(crate) fn select_category(&mut self, group: i32, selected: bool) {
        if self.phase != Phase::Review {
            return;
        }
        for item in &mut self.items {
            if item.selectable() && category(&item.candidate) as i32 == group {
                item.selected = selected;
            }
        }
    }

    pub(crate) fn selected_candidates(&self) -> Vec<Candidate> {
        self.items
            .iter()
            .filter(|item| item.selected && item.selectable())
            .map(|item| item.candidate.clone())
            .collect()
    }

    pub(crate) fn totals(&self, filter: impl Fn(&ReviewItem) -> bool) -> Totals {
        let mut totals = Totals::default();
        for item in self.items.iter().filter(|item| filter(item)) {
            totals.add(&item.candidate);
        }
        totals
    }

    pub(crate) fn begin_cleanup(&mut self) {
        self.cleanup_total = self.selected_candidates().len();
        self.phase = Phase::Cleaning;
        for item in &mut self.items {
            if item.selected && item.selectable() {
                item.state = ItemState::Pending;
                item.error = None;
            }
        }
    }

    pub(crate) fn update(&mut self, progress: &Progress) {
        if let Some((id, state)) = &progress.item {
            for item in &mut self.items {
                if item_key(&item.candidate) == *id {
                    self.active_category = category(&item.candidate);
                    item.state = *state;
                    if *state == ItemState::Failed {
                        item.error = Some(progress.message.clone());
                    }
                }
            }
        }
    }

    pub(crate) fn finish_cleanup(&mut self, cleaned: &[Candidate], failed: bool, cancelled: bool) {
        for item in &mut self.items {
            if let Some(verified) = cleaned
                .iter()
                .find(|cleaned| item_key(cleaned) == item_key(&item.candidate))
            {
                item.candidate = verified.clone();
                item.state = ItemState::Quarantined;
                item.selected = false;
                item.error = None;
            } else if item.state == ItemState::Cleaning {
                item.state = ItemState::Pending;
            }
        }
        self.failed |= failed;
        self.cancelled |= cancelled;
        self.phase = Phase::Summary;
    }
}

#[cfg(test)]
#[path = "../../tests/unit/uninstall_review.rs"]
mod tests;
