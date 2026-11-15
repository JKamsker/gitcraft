//! Window-owned, transient GitHub discovery. No result or credential is serialized.
mod local;
mod requests;
mod view;
use super::App;
use crate::forge::discovery::{DiscoveryError, DiscoverySession, Owner, Page, Query, Repository};
use std::collections::HashSet;
use std::sync::mpsc::Receiver;

pub(super) struct Browser {
    owner: egui::ViewportId,
    local: local::Local,
    session: DiscoverySession,
    serial: u64,
    mode: usize,
    input: String,
    filter: String,
    query: Option<Query>,
    repositories: Vec<Repository>,
    owners: Vec<Owner>,
    next: Option<String>,
    seen_pages: HashSet<String>,
    pending: Option<Pending>,
    loaded: bool,
    incomplete: bool,
    total: Option<u64>,
    error: Option<DiscoveryError>,
    focus: bool,
}
#[derive(Clone, PartialEq, Eq)]
struct Tag {
    owner: egui::ViewportId,
    serial: u64,
    connection: u64,
    query: Query,
}
struct Pending {
    tag: Tag,
    rx: Receiver<(Tag, Result<Page, DiscoveryError>)>,
    _scope: crate::tasks::Scope,
}
pub(super) enum Intent {
    Clone { https: String, ssh: String },
    Open(std::path::PathBuf),
    Manual,
    Settings,
}
impl Browser {
    fn new(owner: egui::ViewportId) -> Self {
        let session = DiscoverySession::current();
        let mode = if session.account.selected() { 0 } else { 2 };
        Self {
            owner,
            local: Default::default(),
            session,
            serial: 0,
            mode,
            input: String::new(),
            filter: String::new(),
            query: None,
            repositories: vec![],
            owners: vec![],
            next: None,
            seen_pages: HashSet::new(),
            pending: None,
            loaded: false,
            incomplete: false,
            total: None,
            error: None,
            focus: true,
        }
    }
    fn reset(&mut self) {
        self.pending = None;
        self.local.retire_open();
        self.serial = self.serial.saturating_add(1);
        self.repositories.clear();
        self.owners.clear();
        self.next = None;
        self.seen_pages.clear();
        self.loaded = false;
        self.error = None;
        self.incomplete = false;
        self.total = None;
        self.filter.clear();
    }
    fn invalidate(&mut self) {
        self.reset();
        self.session = DiscoverySession::current();
        self.query = None;
    }
    fn query_from_controls(&self) -> Query {
        match self.mode {
            0 => Query::Mine,
            1 => Query::Starred,
            3 => Query::SearchOwners(self.input.clone()),
            _ => Query::SearchRepositories(self.input.clone()),
        }
    }
}
impl App {
    pub(super) fn start_repo_browser(&mut self) {
        let known = self
            .all_repo_paths()
            .into_iter()
            .chain(self.recent.iter().cloned())
            .chain(self.workspaces.all_repos())
            .chain(
                self.library
                    .saved()
                    .get(&self.settings.active_profile)
                    .into_iter()
                    .flat_map(|library| library.entries.iter().map(|entry| entry.path.clone())),
            )
            .collect();
        let mut browser = Browser::new(self.windows.current);
        browser.local = local::Local::new(known);
        self.repo_browser = Some(browser);
    }
    pub(super) fn poll_repo_browser(&mut self) {
        if let Some(browser) = &mut self.repo_browser {
            browser.poll();
        }
    }
    pub(super) fn repo_browser_dialog(&mut self, ctx: &egui::Context) {
        let Some(browser) = &mut self.repo_browser else {
            return;
        };
        browser.poll();
        let (closed, intent) = browser.show(ctx);
        if closed || intent.is_some() {
            self.repo_browser = None;
        }
        match intent {
            Some(Intent::Clone { https, ssh }) => self.start_discovered_clone(https, ssh),
            Some(Intent::Open(root)) => self.open_root(root, ctx),
            Some(Intent::Manual) => self.start_clone(),
            Some(Intent::Settings) => self.open_settings(),
            None => {}
        }
    }
}
#[cfg(test)]
mod tests;
