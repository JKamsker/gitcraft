//! Central pane for one file: its history (every commit that changed it, following renames),
//! its blame (who last changed each line) or the whole file as it is at a commit. Opened from a
//! file's right-click menu and from the list of all the files of a commit.

mod blame;
mod history;
mod line_history;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

// Shared across repository tabs, and never reused when a view is closed or replaced.
fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let mut current = NEXT.load(Ordering::Relaxed);
    loop {
        let next = current
            .checked_add(1)
            .unwrap_or_else(|| std::process::abort());
        match NEXT.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(previous) => return previous,
            Err(observed) => current = observed,
        }
    }
}

use egui::{Stroke, Ui};

use super::file_text::{self, FileText};
use super::find::Source;
use super::widgets;
use super::{Action, RepoTab};
use crate::forge::links::{Lines, Target};
use crate::git::{self, FileCommit};
use crate::icons;
use crate::theme::{self, Tokens};

pub(crate) use blame::{BlameData, load_blame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileTab {
    History,
    Blame,
    /// The whole file.
    File,
}

pub(crate) struct FileView {
    pub generation: u64,
    pub path: String,
    /// The commit history, blame and the file are at; HEAD and the working tree when `None`.
    pub rev: Option<String>,
    pub tab: FileTab,
    pub history: Option<Result<Vec<FileCommit>, String>>,
    pub blame: Option<Result<BlameData, String>>,
    /// Blame also depends on configuration, even when file content has a fixed OID.
    pub blame_generation: u64,
    /// A successful previous result remains visible until its replacement arrives.
    pub blame_needs_refresh: bool,
    pub text: Option<Result<FileText, String>>,
    /// The history of only these lines (where their numbers count from, and the lines), not
    /// of the whole file.
    pub lines: Option<(git::LinesAt, git::LineRange)>,
    pub line_inputs: Option<(String, String, git::LineRange)>,
    pub line_input_check: u64,
    history_input: Option<String>,
    text_input: Option<String>,
    observed_inputs: Option<FileInputs>,
    input_check: u64,
    probe: ProbeState,
    pending: [Option<u64>; 3],
    retry_load: Option<Instant>,
    line_observed: Option<(String, String, git::LineRange)>,
    blame_input: Option<String>,
    observed_blame: Option<String>,
    /// A line to scroll to once the blame is read, and to mark (1 is the first).
    pub goto_line: std::cell::Cell<Option<usize>>,
    pub marked_line: Option<usize>,
}

#[derive(Default)]
struct ProbeState {
    active: Option<u64>,
    dirty: bool,
    retry: Option<Instant>,
}

enum ProbeInputs {
    File(FileInputs),
    Lines(Result<(String, String, git::LineRange), String>),
}

#[derive(Clone, Default)]
pub(super) struct FileInputs {
    history: Option<String>,
    text: Option<String>,
    blame: Option<String>,
}

pub(super) fn content_key(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn file_inputs(repo: &std::path::Path, rev: Option<&str>, path: &str, blame: bool) -> FileInputs {
    FileInputs {
        blame: blame.then(|| blame_input(repo, rev, path)).flatten(),
        history: git::resolve(repo, rev.filter(|rev| !rev.is_empty()).unwrap_or("HEAD")).ok(),
        text: git::file_at(repo, rev, path)
            .ok()
            .map(|bytes| content_key(&bytes)),
    }
}

/// Blame depends on attribution settings even at a pinned revision. Observe effective
/// configuration and the content/existence of its repository-side file inputs. Explicit
/// refresh always invalidates, including external textconv programs with mutable state.
pub(super) fn blame_input(repo: &std::path::Path, rev: Option<&str>, path: &str) -> Option<String> {
    let mut bytes = git::run(Some(repo), &["config", "--null", "--list", "--show-origin"])
        .ok()?
        .into_bytes();
    let mut add = |label: &str, value: Result<Vec<u8>, String>| {
        bytes.extend_from_slice(&(label.len() as u64).to_le_bytes());
        bytes.extend_from_slice(label.as_bytes());
        let value = match value {
            Ok(value) => [vec![0], value].concat(),
            Err(error) => [vec![1], error.into_bytes()].concat(),
        };
        bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
        bytes.extend(value);
    };
    add(
        "revision",
        git::resolve(repo, rev.filter(|r| !r.is_empty()).unwrap_or("HEAD")).map(String::into_bytes),
    );
    if rev.is_none() {
        add("worktree", git::file_at(repo, None, path));
        add("index", git::file_at(repo, Some(""), path));
    }
    // Ask Git to resolve effective attributes, including system/default-global
    // sources and index fallback. Hashing only worktree .gitattributes misses both.
    add(
        "attributes",
        git::run(Some(repo), &["check-attr", "-z", "--all", "--", path]).map(String::into_bytes),
    );
    let mut files = vec![".mailmap".to_owned()];
    for key in ["blame.ignoreRevsFile", "mailmap.file"] {
        let values = git::run_ok_codes(
            Some(repo),
            &["config", "--null", "--path", "--get-all", key],
            &[0, 1],
        )
        .ok()?;
        files.extend(
            values
                .split('\0')
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        );
    }
    for file in files {
        add(&file, attribution_file_input(repo, &file));
    }
    // A configured ref:path is mutable even when the viewed commit is fixed.
    // HEAD:.mailmap also covers Git's bare-repository default conservatively.
    let blobs = git::run_ok_codes(
        Some(repo),
        &["config", "--null", "--get-all", "mailmap.blob"],
        &[0, 1],
    )
    .ok()?;
    for blob in blobs
        .split('\0')
        .filter(|s| !s.is_empty())
        .chain(std::iter::once("HEAD:.mailmap"))
    {
        add(
            blob,
            git::run(Some(repo), &["cat-file", "blob", blob]).map(String::into_bytes),
        );
    }
    Some(content_key(&bytes))
}

/// Configuration dependencies are read by Git with ordinary symlink-following
/// file semantics. Keep absolute paths on the repository's machine: joining an
/// absolute dependency to an ssh:// path would silently discard the remote host.
fn attribution_file_input(repo: &std::path::Path, file: &str) -> Result<Vec<u8>, String> {
    attribution_file_input_with(repo, file, git::run_shell)
}

fn attribution_file_input_with(
    repo: &std::path::Path,
    file: &str,
    run: impl FnOnce(&std::path::Path, &str) -> Result<git::ShellRun, String>,
) -> Result<Vec<u8>, String> {
    let file = crate::remote::sh_quote(file);
    let script = format!(
        "if test -f {file}; then printf 'file:'; git hash-object --stdin < {file}; elif test -e {file} || test -L {file}; then printf 'unreadable-or-nonregular'; else printf 'absent'; fi"
    );
    let result = run(repo, &script)?;
    if result.code == 0 {
        Ok(result.stdout)
    } else {
        Err(String::from_utf8_lossy(&result.stderr).into_owned())
    }
}

fn pane_index(pane: FileTab) -> usize {
    match pane {
        FileTab::History => 0,
        FileTab::Blame => 1,
        FileTab::File => 2,
    }
}

impl FileView {
    pub fn new(path: &str, rev: Option<String>, tab: FileTab) -> Self {
        Self {
            generation: next_generation(),
            path: path.to_owned(),
            rev,
            tab,
            history: None,
            blame: None,
            blame_generation: next_generation(),
            blame_needs_refresh: false,
            text: None,
            lines: None,
            line_inputs: None,
            line_input_check: 0,
            history_input: None,
            text_input: None,
            observed_inputs: None,
            input_check: 0,
            probe: ProbeState::default(),
            pending: [None; 3],
            retry_load: None,
            line_observed: None,
            blame_input: None,
            observed_blame: None,
            goto_line: Default::default(),
            marked_line: None,
        }
    }

    /// Invalidate references that can move, including cached failures. A full object ID
    /// identifies immutable content; abbreviations and revision expressions do not.
    pub fn invalidate_mutable(&mut self) -> bool {
        if self.rev.as_deref().is_some_and(git::investigations::oid) {
            return false;
        }
        self.generation = next_generation();
        self.history_input = None;
        self.text_input = None;
        self.observed_inputs = None;
        self.line_observed = None;
        self.input_check = next_generation();
        self.history = None;
        self.invalidate_blame();
        self.text = None;
        true
    }

    fn invalidate_blame(&mut self) {
        self.blame_generation = next_generation();
        self.blame_input = None;
        self.observed_blame = None;
        self.blame_needs_refresh = true;
        if !matches!(self.blame, Some(Ok(_))) {
            self.blame = None;
        }
    }

    fn reuses_line_inputs(
        &self,
        inputs: &Result<(String, String, git::LineRange), String>,
    ) -> bool {
        (matches!(self.history, Some(Ok(_)))
            || (self.history.is_none() && self.pending[0].is_some()))
            && inputs.is_ok()
            && inputs.as_ref().ok() == self.line_inputs.as_ref()
    }

    /// Whether `tab` still has to be loaded.
    pub fn needs(&self, tab: FileTab) -> bool {
        match tab {
            FileTab::History => self.history.is_none(),
            FileTab::Blame => self.blame.is_none() || self.blame_needs_refresh,
            FileTab::File => self.text.is_none(),
        }
    }
}

impl RepoTab {
    /// One active probe per view. Further status observations request one follow-up,
    /// rather than queueing work that will only be discarded later.
    pub(super) fn status_changed_file_view(&mut self, ctx: &egui::Context) {
        if let Some(view) = &mut self.file_view {
            view.probe.dirty = true;
        }
        self.poll_file_view(ctx);
    }

    pub(super) fn poll_file_view(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        let Some(view) = &mut self.file_view else {
            return;
        };
        if let Some(due) = view.retry_load {
            if now >= due {
                view.retry_load = None;
                self.load_file_view(ctx);
            } else {
                ctx.request_repaint_after(due - now);
            }
        }
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.probe.active.is_some() || !view.probe.dirty {
            return;
        }
        if let Some(due) = view.probe.retry
            && now < due
        {
            ctx.request_repaint_after(due - now);
            return;
        }
        view.probe.retry = None;
        view.probe.dirty = false;
        let check = next_generation();
        view.probe.active = Some(check);
        view.input_check = check;
        view.line_input_check = check;
        let (generation, blame_generation, path, rev, lines) = (
            view.generation,
            view.blame_generation,
            view.path.clone(),
            view.rev.clone(),
            view.lines.clone(),
        );
        let blame = view.blame.is_some() || view.pending[1].is_some() || view.tab == FileTab::Blame;
        let repo = self.path.clone();
        self.then_or(
            ctx,
            false,
            Err,
            move || {
                Ok(match lines {
                    Some((at, range)) => {
                        ProbeInputs::Lines(git::line_history_key(&repo, &at, &path, range))
                    }
                    None => ProbeInputs::File(file_inputs(&repo, rev.as_deref(), &path, blame)),
                })
            },
            move |tab, result, ctx| {
                tab.finish_file_probe(generation, blame_generation, check, result, ctx)
            },
        );
    }

    fn finish_file_probe(
        &mut self,
        generation: u64,
        blame_generation: u64,
        check: u64,
        result: Result<ProbeInputs, String>,
        ctx: &egui::Context,
    ) {
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.probe.active != Some(check) {
            return;
        }
        view.probe.active = None;
        match result {
            Err(error) => {
                if error == crate::tasks::BUSY {
                    view.probe.dirty = true;
                    view.probe.retry = Some(Instant::now() + Duration::from_millis(100));
                } else {
                    view.probe.dirty = false;
                } // No automatic cancellation/panic/error loop.
            }
            Ok(_) if view.generation != generation || view.blame_generation != blame_generation => {
                view.probe.dirty = true
            }
            Ok(ProbeInputs::File(inputs)) => {
                self.file_status_inputs(generation, check, inputs, ctx)
            }
            Ok(ProbeInputs::Lines(inputs)) => {
                let Some(view) = &mut self.file_view else {
                    return;
                };
                if let Ok(inputs) = &inputs {
                    view.line_observed = Some(inputs.clone());
                }
                // A pending history read supplies its own captured key on completion.
                // Until then, a successful observation cannot establish a mismatch.
                if view.history.is_none() && view.pending[0].is_some() && inputs.is_ok() {
                } else if !view.reuses_line_inputs(&inputs) {
                    self.invalidate_mutable_file_view(ctx);
                }
            }
        }
        self.poll_file_view(ctx);
    }

    pub(super) fn begin_file_load(&mut self, pane: FileTab) -> Option<u64> {
        let view = self.file_view.as_mut()?;
        let slot = &mut view.pending[pane_index(pane)];
        if slot.is_some() {
            return None;
        }
        let token = next_generation();
        *slot = Some(token);
        Some(token)
    }

    pub(super) fn finish_file_load(
        &mut self,
        pane: FileTab,
        token: u64,
        generation: u64,
        failure: Option<&str>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        let slot = &mut view.pending[pane_index(pane)];
        if *slot != Some(token) {
            return false;
        }
        *slot = None;
        let current = if pane == FileTab::Blame {
            view.blame_generation
        } else {
            view.generation
        };
        if current != generation {
            self.load_file_view(ctx);
            return false;
        }
        if failure == Some(crate::tasks::BUSY) {
            view.retry_load = Some(Instant::now() + Duration::from_millis(100));
            ctx.request_repaint_after(Duration::from_millis(100));
            return false;
        }
        true
    }

    pub(super) fn accept_blame_input(
        &mut self,
        input: Option<String>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        if view
            .observed_blame
            .as_ref()
            .is_some_and(|observed| Some(observed) != input.as_ref())
        {
            view.invalidate_blame();
            self.load_file_view(ctx);
            return false;
        }
        view.blame_input = input;
        true
    }

    fn file_status_inputs(
        &mut self,
        generation: u64,
        check: u64,
        inputs: FileInputs,
        ctx: &egui::Context,
    ) {
        let Some(view) = &mut self.file_view else {
            return;
        };
        if view.generation != generation || view.input_check != check {
            return;
        }
        let history_changed = view
            .history_input
            .as_ref()
            .is_some_and(|old| inputs.history.as_ref() != Some(old))
            || view
                .history
                .as_ref()
                .is_some_and(|result| result.is_err() || view.history_input.is_none());
        let text_changed = view
            .text_input
            .as_ref()
            .is_some_and(|old| inputs.text.as_ref() != Some(old))
            || view
                .text
                .as_ref()
                .is_some_and(|result| result.is_err() || view.text_input.is_none());
        if history_changed || text_changed {
            self.invalidate_mutable_file_view(ctx);
            return;
        }
        // A request that has not replied yet compares its own captured inputs with
        // this observation upon arrival. Unrelated probes do not supersede it.
        let blame_changed = view
            .blame_input
            .as_ref()
            .is_some_and(|old| inputs.blame.as_ref() != Some(old))
            || matches!(view.blame, Some(Err(_)));
        view.observed_blame = inputs.blame.clone();
        view.observed_inputs = Some(inputs);
        if blame_changed {
            view.invalidate_blame();
            if view.tab == FileTab::Blame {
                self.load_file_view(ctx);
            }
        }
    }

    pub(super) fn accept_file_inputs(
        &mut self,
        generation: u64,
        path: &str,
        rev: Option<&str>,
        tab: FileTab,
        input: Option<String>,
        ctx: &egui::Context,
    ) -> bool {
        let Some(view) = &mut self.file_view else {
            return false;
        };
        if view.generation != generation || view.path != path || view.rev.as_deref() != rev {
            return false;
        }
        let observed = view.observed_inputs.as_ref().map(|inputs| match tab {
            FileTab::History => &inputs.history,
            FileTab::File => &inputs.text,
            FileTab::Blame => unreachable!("blame uses its independent epoch"),
        });
        if observed.is_some_and(|expected| expected != &input) {
            self.invalidate_mutable_file_view(ctx);
            return false;
        }
        match tab {
            FileTab::History => view.history_input = input,
            FileTab::File => view.text_input = input,
            FileTab::Blame => unreachable!("blame uses its independent epoch"),
        }
        true
    }

    pub(super) fn invalidate_mutable_file_view(&mut self, ctx: &egui::Context) {
        if let Some(view) = &mut self.file_view {
            if !view.invalidate_mutable() {
                // Full OIDs pin content, but blame.ignoreRevsFile can change attribution.
                view.invalidate_blame();
            }
            self.load_file_view(ctx);
        }
    }

    pub(super) fn file_view(&mut self, ui: &mut Ui, acts: &mut Vec<Action>) {
        let t = Tokens::get(ui.ctx());
        let Some(view) = &self.file_view else { return };
        if view.lines.is_some() {
            self.line_history_view(ui, acts);
            return;
        }

        self.intent_badge(ui, &view.path);
        egui::Frame::new()
            .fill(t.chrome)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .stroke(Stroke::new(1.0, t.separator))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    icons::inline(ui, "history", 16.0, t.icon);
                    ui.label(egui::RichText::new(&view.path).font(theme::medium(13.0)));
                    if let Some(rev) = &view.rev {
                        widgets::chip(
                            ui,
                            &crate::i18n::message(
                                "repo-core-2494",
                                &[("rev", rev.get(..8).unwrap_or(rev).to_string().into())],
                            ),
                            t.text_dim,
                        );
                    }
                    ui.add_space(12.0);
                    let tabs = [
                        (FileTab::History, crate::i18n::text("repo-core-2497")),
                        (FileTab::Blame, crate::i18n::text("repo-core-2498")),
                        (FileTab::File, crate::i18n::text("repo-core-2368")),
                    ];
                    for (tab, label) in tabs {
                        let on = view.tab == tab;
                        let b = ui.add(
                            egui::Button::new(egui::RichText::new(label).color(if on {
                                t.text
                            } else {
                                t.text_dim
                            }))
                            .frame_when_inactive(on),
                        );
                        if on {
                            widgets::active_underline(ui, b.rect, t.accent);
                        }
                        if b.clicked() && !on {
                            acts.push(Action::FileView {
                                path: view.path.clone(),
                                rev: view.rev.clone(),
                                tab,
                            });
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::small_button(ui, "x", crate::i18n::text("repo-core-0711"))
                            .clicked()
                        {
                            acts.push(Action::CloseFileView);
                        }
                    });
                });
            });

        // A diff beside the history has its own bar.
        if self.diff.is_none()
            && let Some(what) = self.find_in()
        {
            self.find.bar(ui, what.name());
        }
        let Some(view) = &self.file_view else { return };
        match view.tab {
            FileTab::History => self.file_history(ui, acts),
            FileTab::Blame => self.blame_lines(ui, acts),
            FileTab::File => {
                // Lines link to the commit the file is at; on disk, they may be in none.
                let rev = view.rev.clone();
                let menu = self.link_menu(None);
                let mut link = |ui: &mut Ui, lines: Lines| {
                    if let Some(rev) = &rev
                        && crate::git::investigations::oid(rev)
                        && ui.button(crate::i18n::text("repo-core-2325")).clicked()
                    {
                        acts.push(Action::OpenLineDossier(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: rev.clone(),
                                path: view.path.clone(),
                                line: lines.first as usize,
                            },
                        )));
                    }
                    if let Some(rev) = &rev
                        && crate::git::investigations::oid(rev)
                        && ui.button(crate::i18n::text("repo-core-2326")).clicked()
                    {
                        acts.push(Action::OpenSymbolBiography(Some(
                            crate::git::investigations::dossier::Seed {
                                commit: rev.clone(),
                                path: view.path.clone(),
                                line: lines.first as usize,
                            },
                        )));
                    }
                    if let (Some(rev), Some(menu)) = (&rev, menu.clone()) {
                        let target = Target::File {
                            rev: rev.clone(),
                            path: view.path.clone(),
                            lines: Some(lines),
                        };
                        menu.items(ui, &lines.label(), &target, acts);
                    }
                };
                // Lines are picked only when there's something to link them to.
                let links = view
                    .rev
                    .is_some()
                    .then_some(&mut link as file_text::LineMenu<'_>);
                let picked = file_text::show(
                    ui,
                    ("file_view", &view.path, &view.rev),
                    view.text.as_ref(),
                    &mut self.find,
                    || Source::File {
                        path: view.path.clone(),
                        rev: view.rev.clone(),
                    },
                    Ok(()),
                    links,
                    Some((&view.goto_line, view.marked_line)),
                );
                if let Some(range) = picked {
                    let at = view
                        .rev
                        .clone()
                        .map_or(git::LinesAt::WorkTree, git::LinesAt::Rev);
                    acts.push(Action::LineHistory {
                        path: view.path.clone(),
                        at,
                        range,
                    });
                }
            }
        }
    }
}

fn error(ui: &mut Ui, e: &str) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.colored_label(
            t.danger,
            crate::git::display_error(&crate::forge::inbox::display_error(e)),
        );
    });
}
