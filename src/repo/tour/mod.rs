//! Background first-day tour with actions using the repository's ordinary guarded routes.
use super::{Action, Msg, Refresh, RepoTab, messages::spawn_or, ops};
use crate::{
    git::activity::History,
    tour::{self, Tour},
};
use std::sync::{
    Arc,
    atomic::{AtomicU64, AtomicUsize, Ordering},
};

static REQUESTS: AtomicU64 = AtomicU64::new(0);
pub(super) struct TourView {
    generation: Arc<AtomicU64>,
    report: Option<Tour>,
    loading: bool,
    error: Option<String>,
    name: String,
    email: String,
    show_help: bool,
}
impl TourView {
    pub(super) fn history(&self) -> Option<Arc<History>> {
        self.report.as_ref().map(|r| r.history.clone())
    }
}
impl RepoTab {
    pub(super) fn open_tour(&mut self, refresh: bool, ctx: &egui::Context) {
        if self.conflict.is_some() || self.rebase.is_some() {
            self.status = Some(("Finish or close the editor first".into(), true));
            return;
        }
        let cached = if refresh {
            None
        } else {
            self.activity
                .as_ref()
                .and_then(|v| v.history())
                .or_else(|| self.tour.as_ref().and_then(TourView::history))
        };
        self.activity = None;
        self.diff = None;
        self.file_view = None;
        self.ci_view = None;
        self.close_pr();
        let generation = REQUESTS.fetch_add(1, Ordering::Relaxed) + 1;
        let token = Arc::new(AtomicU64::new(generation));
        let alive = Arc::downgrade(&token);
        self.tour = Some(TourView {
            generation: token,
            report: None,
            loading: true,
            error: None,
            name: String::new(),
            email: String::new(),
            show_help: false,
        });
        let path = self.path.clone();
        let original = path.clone();
        let failed_path = path.clone();
        spawn_or(
            &self.tx,
            ctx,
            move |reason| {
                Msg::Then(Box::new(move |tab, _| {
                    if tab.path == failed_path {
                        tab.tour_arrived(generation, Err(reason));
                    }
                }))
            },
            move || {
                let active = || {
                    alive
                        .upgrade()
                        .is_some_and(|t| t.load(Ordering::Relaxed) == generation)
                };
                let days = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| (d.as_secs() / 86400) as i64)
                    .unwrap_or(0);
                let report = tour::read(&path, cached, &active, &AtomicUsize::new(0), days);
                Msg::Then(Box::new(move |tab, _| {
                    if tab.path == original {
                        tab.tour_arrived(generation, report);
                    }
                }))
            },
        );
    }
    fn tour_arrived(&mut self, generation: u64, result: Result<Tour, String>) {
        if let Some(view) = &mut self.tour
            && view.generation.load(Ordering::Relaxed) == generation
        {
            view.loading = false;
            match result {
                Ok(report) => {
                    view.name = report.identity.0.clone();
                    view.email = report.identity.1.clone();
                    view.report = Some(report);
                }
                Err(e) => view.error = Some(e),
            }
        }
    }
    pub(super) fn tour_view(&mut self, ui: &mut egui::Ui, acts: &mut Vec<Action>) {
        let Some(view) = &mut self.tour else {
            return;
        };
        let mut close = false;
        let mut refresh = false;
        ui.horizontal(|ui| {
            ui.heading("A tour of this repository");
            refresh = ui.button("Refresh tour").clicked();
            close = ui
                .button(if view.loading {
                    "Cancel tour"
                } else {
                    "Close tour"
                })
                .clicked();
        });
        ui.label("History suggests habits, not rules. Written contribution instructions are the policy; signatures and sign-offs below are observed, not verified.");
        if view.loading {
            ui.add(crate::spinner::Spinner::new());
            ui.label("Reading the repository in the background…");
        }
        if let Some(e) = &view.error {
            ui.colored_label(crate::theme::Tokens::get(ui.ctx()).danger, e);
        }
        egui::ScrollArea::vertical().id_salt("repository_tour").show(ui,|ui|{
            let Some(report)=&view.report else{return;};let h=&report.history;
            ui.label(format!("{} unique commits scanned{}{} · documents at {}",h.commits.len(),if h.more{" (partial history)"}else{" (all reachable history)"},if h.shallow{" · shallow clone"}else{""},report.head.get(..12).unwrap_or(&report.head)));
            ui.label("Branches/remotes/tags/HEAD counted once; app/stash-only refs excluded. Documents and metadata never trigger a lazy fetch. Refresh after repository or mailmap changes.");
            if h.bytes_limited{ui.label("History reached the 64 MiB scan limit.");}
            ui.separator();ui.strong("README and contribution rules");
            for document in &report.documents{
                let readme=document.path.to_ascii_lowercase().starts_with("readme");
                egui::CollapsingHeader::new(&document.path).default_open(readme||view.show_help).show(ui,|ui|{ui.label(&document.text);if document.truncated{ui.label("Excerpt limited to 6,000 characters.");}});
            }
            for unavailable in &report.documents_unavailable{ui.label(unavailable);}
            if report.documents.is_empty(){ui.label("No README or contribution document was available in the recorded HEAD tree.");}
            ui.separator();ui.strong("Observed history workflow");let w=&report.workflow;
            ui.label(format!("{} commits inspected: {} merges, {} Conventional Commit-like subjects, {} signature headers, {} Signed-off-by trailers",w.inspected,w.merges,w.conventional,w.signature_headers,w.signoffs));
            ui.label(if w.merges==0{"No merge commits in this sample. This alone cannot distinguish squash, rebase or linear development."}else{"Merge commits occur in the sample; this does not establish a merge policy."});
            if let Some(e)=&w.incomplete{ui.label(e);}
            ui.strong("Branch and tag names");ui.label(format!("{} branches/remotes; {} tags (up to 2,000 names)",report.branches.len(),report.tags.len()));
            ui.label(format!("Branches: {}",report.branches.iter().take(30).cloned().collect::<Vec<_>>().join(", ")));ui.label(format!("Tags: {}",report.tags.iter().take(30).cloned().collect::<Vec<_>>().join(", ")));
            ui.label(format!("Observed prefixes: {} release/* branches, {} v* tags",report.branches.iter().filter(|s|s.contains("/release/")).count(),report.tags.iter().filter(|s|s.starts_with('v')).count()));
            ui.separator();ui.strong(format!("Recent activity: {} through {}",report.since,report.through));ui.label(format!("{} sampled commits by committer date, using each recorded timezone. Counts may be incomplete when history is partial or shallow. Contributors are frequent contributors, not folder owners.",report.recent_commits));
            for folder in &report.folders{ui.label(format!("{}: {}",String::from_utf8_lossy(&folder.path),folder.authors.iter().map(|(n,e,c)|format!("{n} <{e}> ({c} commits)")).collect::<Vec<_>>().join(", ")));}
            ui.strong("Files changed most often");for (path,n) in &report.hot_files{ui.label(format!("{n} commits · {}",String::from_utf8_lossy(path)));}
            ui.separator();ui.strong("Setup checklist");ui.label(format!("Detected: {}",if report.needs.is_empty(){"No listed tool markers".into()}else{report.needs.iter().cloned().collect::<Vec<_>>().join(", ")}));
            ui.label("Set this repository's author (public identity only):");ui.horizontal(|ui|{ui.label("Name");ui.text_edit_singleline(&mut view.name);ui.label("Email");ui.text_edit_singleline(&mut view.email);});
            let valid=!view.name.trim().is_empty()&&!view.email.trim().is_empty()&&!view.name.chars().any(char::is_control)&&!view.email.chars().any(char::is_control)&&self.busy_op.is_none();
            if ui.add_enabled(valid,egui::Button::new("Set identity for this repository")).clicked(){acts.push(Action::SetIdentity(view.name.clone(),view.email.clone()));}
            if report.needs.contains("Git LFS"){
                ui.hyperlink_to("Get git-lfs for this machine","https://git-lfs.com/");
                if ui.add_enabled(self.busy_op.is_none(),egui::Button::new("Install LFS hooks locally")).clicked(){acts.push(Action::Ops{label:"Install local Git LFS hooks".into(),ops:ops(&[&["lfs","install","--local"]]),refresh:Refresh::All});}
            }
            if report.needs.contains("Submodules")&&ui.button("Review submodule initialization…").clicked(){acts.push(Action::Dialog(super::Dialog::confirm("Initialize submodules","Download and check out the submodules named by this repository? Review .gitmodules before continuing.".into(),"Initialize submodules",ops(&[&["submodule","update","--init","--recursive"]]),None)));}
            if ui.button("Review hook, EditorConfig and signing setup").clicked(){view.show_help=!view.show_help;}
            if view.show_help{ui.label("Review the contribution documents above before installing Husky, Lefthook or pre-commit. This tour never runs project scripts. Enable EditorConfig support in your editor. For signing, use Settings → Git → Commit signing and choose a local key; a signature header does not prove trust or validity.");}
            if valid&&ui.button("Copy a Signed-off-by line").clicked(){ui.ctx().copy_text(format!("Signed-off-by: {} <{}>",view.name,view.email));}
        });
        if close {
            self.tour = None;
        } else if refresh {
            self.open_tour(true, ui.ctx());
        }
    }
}
