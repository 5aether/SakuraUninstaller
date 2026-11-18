use crate::{
    reporting::report::Message,
    safety::quarantine::Saved,
    scanner::{installed_apps::App, leftovers::Candidate},
    storage::Locations,
    uninstall::{review::ReviewSession, runner::Job, session::BatchControl},
    updates,
};
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex},
};

pub(crate) struct State {
    pub(crate) locations: Locations,
    pub(crate) apps: Vec<(App, String)>,
    pub(crate) reviews: Vec<Candidate>,
    pub(crate) saved: Vec<Saved>,
    pub(crate) selection: [HashSet<String>; 3],
    pub(crate) visible: Vec<String>,
    pub(crate) pending: Option<Job>,
    pub(crate) batch_control: Option<BatchControl>,
    pub(crate) uninstall_review: Option<ReviewSession>,
    pub(crate) status: Option<Message>,
    pub(crate) report: Vec<Message>,
    pub(crate) failed: bool,
    pub(crate) measured: HashMap<String, u64>,
    pub(crate) update: Option<updates::Update>,
}

pub(crate) type Shared = Arc<Mutex<State>>;
