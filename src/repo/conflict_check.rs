//! Conflict warnings: whether a merge, rebase or cherry-pick offered in a menu would conflict,
//! shown beside it before it is clicked, and a toolbar badge when the checked-out branch would
//! conflict with its target. Worked out with [`git::simulate`] in the background when a menu
//! first asks, a few at a time, and kept by the commits involved: they never change, so a menu
//! opened again answers at once.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Response, RichText, Ui};

use super::RepoTab;
use crate::git::simulate::{self, ConflictFile, Features, Outcome, Prediction};
use crate::theme::Tokens;

/// What is checked.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Op {
    /// `other` merged into HEAD.
    Merge,
    /// HEAD's branch rebased onto `other`.
    Rebase,
    /// `other` cherry-picked onto HEAD.
    CherryPick,
}

/// One check, by the commits it is about.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Key {
    pub op: Op,
    pub head: String,
    pub other: String,
    /// For a rebase that a pull does: the upstream's name, whose fork point it starts from.
    pub upstream: Option<String>,
}

impl Key {
    pub fn new(op: Op, head: &str, other: &str) -> Self {
        Self {
            op,
            head: head.to_owned(),
            other: other.to_owned(),
            upstream: None,
        }
    }

    /// `git pull --rebase` from `upstream`, at commit `other`.
    pub fn pull_rebase(head: &str, other: &str, upstream: &str) -> Self {
        Self {
            upstream: Some(upstream.to_owned()),
            ..Self::new(Op::Rebase, head, other)
        }
    }
}

/// What Mergiraf could do with a conflicted file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Mergiraf {
    /// A text conflict in a language Mergiraf knows.
    Likely,
    /// A text conflict in a file Mergiraf has no language for.
    UnknownLanguage,
    /// Not a text conflict (binary, deleted or renamed on one side…): the kinds.
    NotText(String),
    /// A text conflict, and Mergiraf isn't installed.
    NotInstalled,
    /// A text conflict, and Mergiraf is installed but couldn't list its languages: why.
    Unchecked(String),
}

/// A finished check: the prediction, and what Mergiraf could do with each conflicted file.
#[derive(Debug)]
pub(super) struct Verdict {
    pub prediction: Prediction,
    pub mergiraf: Vec<Mergiraf>,
}

/// Where a check is.
#[derive(Clone, Debug)]
pub(super) enum Check {
    Checking,
    Done(Arc<Verdict>),
    /// git failed: what it said.
    Failed(String),
    /// This git can't check it: why.
    Unavailable(String),
}

/// How many checks are kept; the least recently asked for go first.
const CAPACITY: usize = 256;
/// How many checks run at once.
const WORKERS: usize = 2;

#[derive(Default)]
struct Inner {
    /// What the git of the repository's machine can do, and whether the repository is a
    /// partial clone, once found out (a failure to find out is asked again next time).
    repo_info: Option<RepoInfo>,
    /// Each check, with when it was last asked for, and when it failed if it did.
    entries: HashMap<Key, Entry>,
    /// Checks waiting for a worker; the latest asked for is taken first.
    queue: Vec<Key>,
    workers: usize,
    /// Counts the questions, for [`Inner::entries`]' ages.
    asked: u64,
    /// Counts [`ConflictChecks::forget`]s: what a worker started before one is not kept.
    generation: u64,
    /// [`crate::mergiraf::changes`] when the checks were made: git set up to merge with
    /// Mergiraf, or set back, changes what merges cleanly.
    mergiraf_changes: u32,
}

/// A check, as [`Inner::entries`] keeps it.
#[derive(Clone, Debug)]
struct Entry {
    asked: u64,
    check: Check,
    /// When it failed: a failure (a machine out of reach) is tried again after [`RETRY`].
    failed: Option<Instant>,
}

/// What a check needs to know about the repository's git.
#[derive(Clone, Copy, Debug)]
struct RepoInfo {
    features: Features,
    /// A partial clone, whose missing files git would download to merge.
    partial: bool,
}

/// The most checks waiting for a worker; beyond, the oldest asked for are dropped (their menus
/// are long closed).
const MAX_QUEUE: usize = 32;

/// How long a failed check stands before it is tried again.
const RETRY: Duration = Duration::from_secs(30);

impl Inner {
    /// Forgets every check, those running included (see [`ConflictChecks::forget`]).
    fn forget(&mut self) {
        self.entries.clear();
        self.queue.clear();
        self.repo_info = None;
        self.generation += 1;
    }

    /// Forgets the checks waiting for a worker that are about another HEAD than `head`: the
    /// branch moved on, and menus only ask about the current one.
    fn drop_stale(&mut self, head: &str) {
        let stale: Vec<Key> = self
            .queue
            .iter()
            .filter(|k| k.head != head)
            .cloned()
            .collect();
        self.queue.retain(|k| k.head == head);
        for key in stale {
            self.entries.remove(&key);
        }
    }

    /// Drops the oldest waiting checks beyond [`MAX_QUEUE`].
    fn bound_queue(&mut self) {
        let over = self.queue.len().saturating_sub(MAX_QUEUE);
        for key in self.queue.drain(..over).collect::<Vec<_>>() {
            self.entries.remove(&key);
        }
    }

    /// Drops the checks asked for longest ago, beyond [`CAPACITY`], but none still running.
    fn evict(&mut self) {
        let over = self.entries.len().saturating_sub(CAPACITY);
        if over == 0 {
            return;
        }
        let mut done: Vec<(u64, Key)> = self
            .entries
            .iter()
            .filter(|(_, e)| !matches!(e.check, Check::Checking))
            .map(|(key, e)| (e.asked, key.clone()))
            .collect();
        done.sort_unstable_by_key(|(asked, _)| *asked);
        for (_, key) in done.into_iter().take(over) {
            self.entries.remove(&key);
        }
    }
}

/// The checks of one repository, shared with the threads working them out.
#[derive(Default)]
pub(super) struct ConflictChecks {
    inner: Arc<Mutex<Inner>>,
}

impl ConflictChecks {
    /// Forgets every check, those running included (F5): what merges cleanly also depends
    /// on `.gitattributes` and git's config, which the commits don't capture.
    pub fn forget(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.forget();
        }
    }
}

impl Drop for ConflictChecks {
    /// The tab is closed: what is still waiting is not worked out.
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.queue.clear();
        }
    }
}

impl ConflictChecks {
    /// Where the check `key` of `repo` is; one not asked for before starts in the background,
    /// `ctx` repainted when it is done.
    pub fn get(&self, ctx: &egui::Context, repo: &Path, key: Key) -> Check {
        let Ok(mut inner) = self.inner.lock() else {
            return Check::Failed(crate::i18n::text("repo-core-0747").to_owned());
        };
        // Mergiraf's set-up changed (Settings → Git): every answer may have.
        let mergiraf_changes = crate::mergiraf::changes();
        if inner.mergiraf_changes != mergiraf_changes {
            inner.forget();
            inner.mergiraf_changes = mergiraf_changes;
        }
        inner.asked += 1;
        let asked = inner.asked;
        let retry = |e: &Entry| e.failed.is_some_and(|at| at.elapsed() >= RETRY);
        inner.drop_stale(&key.head);
        if let Some(entry) = inner.entries.get_mut(&key)
            && !retry(entry)
        {
            entry.asked = asked;
            return entry.check.clone();
        }
        let entry = Entry {
            asked,
            check: Check::Checking,
            failed: None,
        };
        inner.entries.insert(key.clone(), entry);
        inner.queue.push(key);
        inner.bound_queue();
        inner.evict();
        if inner.workers < WORKERS {
            inner.workers += 1;
            let (shared, ctx, repo) = (self.inner.clone(), ctx.clone(), repo.to_path_buf());
            crate::connections::spawn(move || work(&shared, &ctx, &repo));
        }
        Check::Checking
    }
}

/// A worker: takes checks from the queue until it is empty.
fn work(shared: &Mutex<Inner>, ctx: &egui::Context, repo: &Path) {
    loop {
        let (key, info, generation) = {
            let Ok(mut inner) = shared.lock() else { return };
            let Some(key) = inner.queue.pop() else {
                inner.workers -= 1;
                return;
            };
            (key, inner.repo_info, inner.generation)
        };
        // Nothing here may unwind past the loop, or this worker's slot is never given back.
        let check = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let info = match info {
                Some(info) => Ok(info),
                None => repo_info(repo).inspect(|found| {
                    if let Ok(mut inner) = shared.lock()
                        && inner.generation == generation
                    {
                        inner.repo_info = Some(*found);
                    }
                }),
            };
            run_check(ctx, repo, info, &key)
        }))
        .unwrap_or_else(|_| Check::Failed(crate::i18n::text("repo-core-0748").to_owned()));
        if let Ok(mut inner) = shared.lock()
            && inner.generation == generation
            && let Some(entry) = inner.entries.get_mut(&key)
        {
            entry.failed = matches!(check, Check::Failed(_)).then(Instant::now);
            entry.check = check;
        }
        ctx.request_repaint();
    }
}

/// What the git of `repo`'s machine can do, and whether `repo` is a partial clone.
fn repo_info(repo: &Path) -> Result<RepoInfo, String> {
    Ok(RepoInfo {
        features: simulate::features(repo)?,
        partial: simulate::partial_clone(repo)?,
    })
}

/// Works out the check `key`.
fn run_check(ctx: &egui::Context, repo: &Path, info: Result<RepoInfo, String>, key: &Key) -> Check {
    let RepoInfo { features, partial } = match info {
        Ok(info) => info,
        Err(e) => return Check::Failed(e),
    };
    if partial {
        return Check::Unavailable(crate::i18n::text("repo-core-0749").to_owned());
    }
    let unavailable = match key.op {
        Op::Merge => features.merge_unavailable(),
        Op::Rebase | Op::CherryPick => features.replay_unavailable(),
    };
    if let Some(why) = unavailable {
        return Check::Unavailable(why);
    }
    let predicted = match key.op {
        Op::Merge => simulate::merge(repo, &key.head, &key.other),
        Op::Rebase => match &key.upstream {
            Some(upstream) => {
                simulate::pull_rebase(repo, features, &key.head, upstream, &key.other)
            }
            None => simulate::rebase(repo, features, &key.head, &key.other),
        },
        Op::CherryPick => simulate::cherry_pick(repo, features, &key.head, &[&key.other]),
    };
    match predicted {
        Ok(prediction) => {
            let mergiraf = mergiraf_hints(ctx, conflicted(&prediction));
            Check::Done(Arc::new(Verdict {
                prediction,
                mergiraf,
            }))
        }
        Err(e) => Check::Failed(e),
    }
}

/// The files a prediction says conflict.
fn conflicted(prediction: &Prediction) -> &[ConflictFile] {
    match &prediction.outcome {
        Outcome::Conflicts { files, .. } => files,
        _ => &[],
    }
}

/// What Mergiraf could do with each of `files`. Runs Mergiraf (to list its languages): from a
/// background thread.
fn mergiraf_hints(ctx: &egui::Context, files: &[ConflictFile]) -> Vec<Mergiraf> {
    if files.is_empty() {
        return Vec::new();
    }
    let patterns = crate::mergiraf::found(ctx).map(|f| crate::mergiraf::patterns(&f));
    files
        .iter()
        .map(|file| match &patterns {
            Some(Err(e)) if file.text_only() => Mergiraf::Unchecked(e.clone()),
            Some(Ok(patterns)) => mergiraf_hint(file, Some(patterns)),
            _ => mergiraf_hint(file, None),
        })
        .collect()
}

/// What Mergiraf could do with `file`, knowing the `patterns` of its languages (none when it
/// isn't installed).
pub(super) fn mergiraf_hint(file: &ConflictFile, patterns: Option<&[String]>) -> Mergiraf {
    if !file.text_only() {
        let kinds = plain_kinds(&file.kinds);
        return Mergiraf::NotText(kinds);
    }
    match patterns {
        None => Mergiraf::NotInstalled,
        Some(patterns) if crate::mergiraf::knows(patterns, &file.path) => Mergiraf::Likely,
        Some(_) => Mergiraf::UnknownLanguage,
    }
}

/// git's names for what conflicts in a file, in plain words. `contents` is left out beside
/// another kind (a binary file conflicts in both).
fn plain_kinds(kinds: &[String]) -> String {
    let words: Vec<&str> = kinds
        .iter()
        .filter(|k| kinds.len() == 1 || *k != "contents")
        .map(|k| match k.as_str() {
            "contents" => crate::i18n::text("repo-core-0751"),
            "binary" => crate::i18n::text("repo-core-0752"),
            "modify/delete" => crate::i18n::text("repo-core-0754"),
            "rename/delete" => crate::i18n::text("repo-core-0756"),
            "rename/rename" => crate::i18n::text("repo-core-0758"),
            "file/directory" | "directory/file" => crate::i18n::text("repo-core-0761"),
            "add/add" => crate::i18n::text("repo-core-0763"),
            "submodule" => "submodule",
            other => other,
        })
        .collect();
    if words.is_empty() {
        crate::i18n::text("repo-core-0764").to_owned()
    } else {
        words.join("; ")
    }
}

/// How a label beside a menu entry is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Tone {
    Faint,
    Good,
    Warn,
}

/// The few words beside the menu entry of `op`, if any.
pub(super) fn label(op: Op, check: &Check) -> Option<(String, Tone)> {
    match check {
        Check::Checking => Some((crate::i18n::text("repo-core-0765").to_owned(), Tone::Faint)),
        Check::Unavailable(_) => None,
        Check::Failed(_) => Some((crate::i18n::text("repo-core-0766").to_owned(), Tone::Faint)),
        Check::Done(verdict) => Some(match &verdict.prediction.outcome {
            Outcome::Clean { changes: false } if op == Op::CherryPick => {
                (crate::i18n::text("repo-core-0767").to_owned(), Tone::Faint)
            }
            Outcome::Clean { changes: false } if op == Op::Merge => {
                (crate::i18n::text("repo-core-0768").to_owned(), Tone::Faint)
            }
            Outcome::Clean { .. } => (crate::i18n::text("repo-core-0769").to_owned(), Tone::Good),
            Outcome::Conflicts { files, .. } => (
                crate::i18n::message(
                    "repo-core-0770",
                    &[("len", format!("{}", files_count(files.len())).into())],
                ),
                Tone::Warn,
            ),
            Outcome::Unsure(_) => (crate::i18n::text("repo-core-0771").to_owned(), Tone::Warn),
        }),
    }
}

/// `1 file`, `3 files`.
fn files_count(n: usize) -> String {
    if n == 1 {
        crate::i18n::text("repo-core-0772").to_owned()
    } else {
        crate::i18n::message("repo-core-0773", &[("n", (n).into())])
    }
}

/// How many conflicted files a tooltip lists; the rest are counted.
const LISTED: usize = 15;

/// What hovering a checked entry says: the outcome, the conflicted files with what Mergiraf
/// could do with each, and that uncommitted changes (`dirty`) are left out.
pub(super) fn tooltip(op: Op, check: &Check, dirty: bool) -> String {
    let mut text = match check {
        Check::Checking => crate::i18n::text("repo-core-0774").to_owned(),
        Check::Unavailable(why) => return why.clone(),
        Check::Failed(e) => {
            crate::i18n::message("repo-core-0775", &[("e", format!("{}", e).into())])
        }
        Check::Done(verdict) => verdict_text(op, verdict),
    };
    if dirty && !matches!(check, Check::Unavailable(_)) {
        text.push_str(crate::i18n::text("repo-core-0776"));
    }
    text
}

fn verdict_text(op: Op, verdict: &Verdict) -> String {
    let commits = verdict.prediction.commits;
    match &verdict.prediction.outcome {
        Outcome::Clean { changes: false } if op == Op::CherryPick => {
            crate::i18n::text("repo-core-0777").to_owned()
        }
        Outcome::Clean { changes: false } if op == Op::Merge => {
            crate::i18n::text("repo-core-0778").to_owned()
        }
        Outcome::Clean { .. } if op == Op::Rebase && commits > 0 => {
            let all = if commits == 1 {
                crate::i18n::text("repo-core-0779").to_owned()
            } else {
                crate::i18n::message("repo-core-0780", &[("commits", (commits).into())])
            };
            crate::i18n::message("repo-core-0781", &[("all", format!("{}", all).into())])
        }
        Outcome::Clean { .. } => crate::i18n::text("repo-core-0782").to_owned(),
        Outcome::Unsure(why) => {
            crate::i18n::message("repo-core-0783", &[("why", format!("{}", why).into())])
        }
        Outcome::Conflicts { files, stop } => {
            let mut text = match stop {
                Some(stop) if op == Op::Rebase || commits > 1 => crate::i18n::message(
                    "repo-core-0784",
                    &[
                        ("index", format!("{}", stop.index).into()),
                        ("commits", format!("{}", commits).into()),
                        (
                            "hash",
                            format!("{}", super::short(&stop.commit.hash)).into(),
                        ),
                        ("summary", format!("{}", stop.commit.summary).into()),
                        ("len", format!("{}", files_count(files.len())).into()),
                    ],
                ),
                _ => crate::i18n::message(
                    "repo-core-0785",
                    &[("len", format!("{}", files_count(files.len())).into())],
                ),
            };
            text.push_str(&file_lines(files, &verdict.mergiraf));
            if let Some(stop) = stop
                && stop.index < commits
            {
                text.push_str(crate::i18n::text("repo-core-0786"));
            }
            let unchecked = verdict.mergiraf.iter().find_map(|m| match m {
                Mergiraf::Unchecked(e) => Some(e),
                _ => None,
            });
            if let Some(e) = unchecked {
                text.push_str(&crate::i18n::message(
                    "repo-core-0787",
                    &[("e", format!("{}", e).into())],
                ));
            }
            if verdict.mergiraf.contains(&Mergiraf::NotInstalled) {
                text.push_str(crate::i18n::text("repo-core-0788"));
            }
            text
        }
    }
}

/// One line for each conflicted file (up to [`LISTED`]), with what Mergiraf could do.
fn file_lines(files: &[ConflictFile], mergiraf: &[Mergiraf]) -> String {
    let mut text = String::new();
    for (n, file) in files.iter().enumerate().take(LISTED) {
        let hint = match mergiraf.get(n) {
            Some(Mergiraf::Likely) => crate::i18n::text("repo-core-0789").to_owned(),
            Some(Mergiraf::UnknownLanguage) => crate::i18n::text("repo-core-0790").to_owned(),
            Some(Mergiraf::NotText(kinds)) => {
                crate::i18n::message("repo-core-0791", &[("kinds", format!("{}", kinds).into())])
            }
            Some(Mergiraf::NotInstalled | Mergiraf::Unchecked(_)) | None => {
                crate::i18n::text("repo-core-0792").to_owned()
            }
        };
        text.push_str(&format!("\n• {} — {hint}", file.path));
    }
    if files.len() > LISTED {
        text.push_str(&crate::i18n::message(
            "repo-core-0794",
            &[("LISTED", format!("{}", files.len() - LISTED).into())],
        ));
    }
    text
}

/// How long the toolbar's target has to stay the same before it is checked: refs move a few
/// times in a row during a fetch, a pull or a checkout.
const SETTLE: Duration = Duration::from_millis(800);

/// What the toolbar's badge is about: the target the checked-out branch was last compared
/// with, and since when.
#[derive(Default)]
pub(super) struct TargetWatch {
    key: Option<Key>,
    target: String,
    /// `target` is the branch's upstream, not the default branch.
    upstream: bool,
    since: Option<Instant>,
}

impl RepoTab {
    /// Reads the repository again and forgets the conflict checks (F5, the toolbar's button,
    /// coming back to the window): `.gitattributes` or git's config may have changed.
    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.conflict_checks.forget();
        self.reload(ctx);
    }

    /// A menu entry for `op` with `other`, marked with whether it would conflict.
    pub(super) fn conflict_button(
        &self,
        ui: &mut Ui,
        text: String,
        enabled: bool,
        op: Op,
        other: &str,
    ) -> Response {
        let icon = match op {
            Op::Merge | Op::Rebase => "git-merge",
            Op::CherryPick => "git-branch",
        };
        let refused = (!enabled).then_some(if other == self.refs.head {
            crate::i18n::text("repo-core-0796")
        } else {
            crate::i18n::text("repo-core-0797")
        });
        if !enabled || self.refs.head.is_empty() || other == self.refs.head {
            return crate::menu_item::MenuItem::new(text)
                .icon(icon)
                .refused(refused)
                .show(ui);
        }
        let key = Key::new(op, &self.refs.head, other);
        let check = self.conflict_checks.get(ui.ctx(), &self.path, key);
        // The entry and its mark on one line: the menu widens for them.
        let mut button = crate::menu_item::MenuItem::new(text).icon(icon);
        if let Some((words, tone)) = label(op, &check) {
            let t = Tokens::get(ui.ctx());
            let color = match tone {
                Tone::Faint => t.text_faint,
                Tone::Good => t.success,
                Tone::Warn => t.warning,
            };
            button = button.annotation(RichText::new(words).color(color).small());
        }
        button
            .show(ui)
            .on_hover_text(tooltip(op, &check, !self.work.is_clean()))
    }

    /// The branch the checked-out one is compared with for the toolbar's badge, and its
    /// commit: its upstream, or the default branch when it has none (or it is gone).
    fn conflict_target(&self) -> Option<(String, String, bool)> {
        let current = self.refs.current.as_deref()?;
        let branch = self.refs.local.iter().find(|b| b.name == current)?;
        let local = || self.refs.local.iter();
        let remote = || self.refs.remotes.iter().flat_map(|(_, list)| list);
        if !branch.upstream.is_empty() && !branch.upstream_gone() {
            // `%(upstream:short)` names a remote branch as `origin/x`, a local one as `x`.
            let name = &branch.upstream;
            let found = remote()
                .find(|b| b.name == *name)
                .or_else(|| local().find(|b| b.name == *name))?;
            return Some((name.clone(), found.target.clone(), true));
        }
        let base = self.base_ref()?;
        if !base.remote && base.name == current {
            return None;
        }
        let found = if base.remote {
            remote().find(|b| b.name == base.name)
        } else {
            local().find(|b| b.name == base.name)
        }?;
        Some((base.name, found.target.clone(), false))
    }

    /// The toolbar's badge, when merging the checked-out branch's target into it would
    /// conflict. Checked once the refs have settled, and not while an operation runs.
    pub(super) fn target_conflict_badge(&mut self, ui: &mut Ui) {
        let target = self.conflict_target();
        // With Pull set to rebase, the upstream is rebased onto, not merged: a branch can
        // merge cleanly and still stop at one of its commits.
        let rebase =
            crate::settings::Settings::get(ui.ctx()).pull_mode == super::pull::PullMode::Rebase;
        let key = target
            .as_ref()
            .filter(|(_, commit, _)| !self.refs.head.is_empty() && *commit != self.refs.head)
            .map(|(name, commit, upstream)| {
                if rebase && *upstream {
                    Key::pull_rebase(&self.refs.head, commit, name)
                } else {
                    Key::new(Op::Merge, &self.refs.head, commit)
                }
            });
        let watch = &mut self.target_watch;
        // Named as it is now, even when another branch at the same commit took its place.
        watch.upstream = target.as_ref().is_some_and(|(_, _, upstream)| *upstream);
        watch.target = target.map(|(name, _, _)| name).unwrap_or_default();
        if watch.key != key {
            watch.key = key;
            watch.since = Some(Instant::now());
        }
        let Some(key) = watch.key.clone() else {
            return;
        };
        let settled = watch.since.map_or(Duration::ZERO, |s| s.elapsed());
        if settled < SETTLE {
            ui.ctx().request_repaint_after(SETTLE - settled);
            return;
        }
        if self.busy_op.is_some() || self.work.in_progress.is_some() {
            return;
        }
        let op = key.op;
        let check = self.conflict_checks.get(ui.ctx(), &self.path, key);
        let Check::Done(verdict) = &check else {
            return;
        };
        let what = match &verdict.prediction.outcome {
            Outcome::Conflicts { .. } => crate::i18n::text("repo-core-0798"),
            Outcome::Unsure(_) => crate::i18n::text("repo-core-0799"),
            Outcome::Clean { .. } => return,
        };
        let t = Tokens::get(ui.ctx());
        let target = self.target_watch.target.clone();
        let current = self.refs.current.clone().unwrap_or_default();
        ui.add_space(6.0);
        let why = if self.target_watch.upstream {
            crate::i18n::text("repo-core-0800")
        } else {
            crate::i18n::text("repo-core-0801")
        };
        let doing = match op {
            Op::Rebase => crate::i18n::message(
                "repo-core-0802",
                &[
                    ("current", format!("{}", current).into()),
                    ("target", format!("{}", target).into()),
                    ("why", format!("{}", why).into()),
                ],
            ),
            _ => crate::i18n::message(
                "repo-core-0803",
                &[
                    ("target", format!("{}", target).into()),
                    ("why", format!("{}", why).into()),
                    ("current", format!("{}", current).into()),
                ],
            ),
        };
        super::widgets::chip(ui, &format!("{what} {target}"), t.warning).on_hover_text(format!(
            "{doing}:\n{}",
            tooltip(op, &check, !self.work.is_clean())
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::simulate::Stop;

    fn file(path: &str, kinds: &[&str]) -> ConflictFile {
        ConflictFile {
            path: path.to_owned(),
            kinds: kinds.iter().map(|k| (*k).to_owned()).collect(),
        }
    }

    fn done(outcome: Outcome, commits: usize, mergiraf: Vec<Mergiraf>) -> Check {
        Check::Done(Arc::new(Verdict {
            prediction: Prediction { outcome, commits },
            mergiraf,
        }))
    }

    #[test]
    fn labels_say_how_it_would_go() {
        let clean = done(Outcome::Clean { changes: true }, 0, Vec::new());
        assert_eq!(
            label(Op::Merge, &clean),
            Some(("applies cleanly".to_owned(), Tone::Good))
        );
        let nothing = done(Outcome::Clean { changes: false }, 1, Vec::new());
        assert_eq!(
            label(Op::CherryPick, &nothing),
            Some(("already applied".to_owned(), Tone::Faint))
        );
        assert_eq!(label(Op::Rebase, &nothing).map(|l| l.1), Some(Tone::Good));
        assert_eq!(
            label(Op::Merge, &nothing),
            Some(("already merged".to_owned(), Tone::Faint))
        );
        let three = done(
            Outcome::Conflicts {
                files: vec![file("a", &[]), file("b", &[]), file("c", &[])],
                stop: None,
            },
            0,
            Vec::new(),
        );
        assert_eq!(
            label(Op::Merge, &three),
            Some(("conflicts in 3 files".to_owned(), Tone::Warn))
        );
        let unsure = done(Outcome::Unsure("x".into()), 200, Vec::new());
        assert_eq!(
            label(Op::Rebase, &unsure),
            Some(("may conflict".to_owned(), Tone::Warn))
        );
        assert_eq!(label(Op::Merge, &Check::Unavailable("old".into())), None);
        assert_eq!(
            label(Op::Merge, &Check::Checking).map(|l| l.1),
            Some(Tone::Faint)
        );
    }

    #[test]
    fn mergiraf_is_offered_for_text_conflicts_it_knows() {
        let patterns = vec!["*.rs".to_owned(), "*.json".to_owned()];
        let known = Some(patterns.as_slice());
        assert_eq!(
            mergiraf_hint(&file("src/a.rs", &["contents"]), known),
            Mergiraf::Likely
        );
        assert_eq!(
            mergiraf_hint(&file("notes.txt", &["contents"]), known),
            Mergiraf::UnknownLanguage
        );
        assert_eq!(
            mergiraf_hint(&file("img.rs", &["binary", "contents"]), known),
            Mergiraf::NotText("binary file".to_owned())
        );
        assert_eq!(
            mergiraf_hint(&file("gone.rs", &["modify/delete"]), known),
            Mergiraf::NotText("changed on one side, deleted on the other".to_owned())
        );
        assert_eq!(
            mergiraf_hint(&file("a.rs", &["contents"]), None),
            Mergiraf::NotInstalled
        );
    }

    /// A rebase that stops says at which commit, lists the files with Mergiraf's chances,
    /// and that what is after it wasn't checked.
    #[test]
    fn a_rebase_tooltip_names_the_commit_and_the_files() {
        let stop = Stop {
            index: 3,
            commit: simulate::Pick {
                hash: "0123456789abcdef".into(),
                parent: Some("p".into()),
                summary: "Third".into(),
            },
        };
        let check = done(
            Outcome::Conflicts {
                files: vec![file("src/a.rs", &["contents"]), file("b.png", &["binary"])],
                stop: Some(stop),
            },
            5,
            vec![Mergiraf::Likely, Mergiraf::NotText("binary".into())],
        );
        let text = tooltip(Op::Rebase, &check, true);
        assert!(
            text.starts_with("Commit 3 of 5, 01234567 “Third”, conflicts in 2 files:"),
            "{text}"
        );
        assert!(
            text.contains("• src/a.rs — Mergiraf can likely solve it"),
            "{text}"
        );
        assert!(text.contains("• b.png — binary: solve by hand"), "{text}");
        assert!(text.contains("aren't checked"), "{text}");
        assert!(text.contains("Uncommitted changes"), "{text}");
        assert!(!text.contains("Settings → Git"), "{text}");
    }

    #[test]
    fn long_lists_are_cut_and_a_missing_mergiraf_is_mentioned() {
        let files: Vec<ConflictFile> = (0..20)
            .map(|n| file(&format!("f{n}.rs"), &["contents"]))
            .collect();
        let check = done(
            Outcome::Conflicts { files, stop: None },
            0,
            vec![Mergiraf::NotInstalled; 20],
        );
        let text = tooltip(Op::Merge, &check, false);
        assert!(text.starts_with("Conflicts in 20 files:"), "{text}");
        assert_eq!(text.matches("\n• ").count(), LISTED);
        assert!(text.contains("…and 5 more"), "{text}");
        assert!(text.contains("Settings → Git → Mergiraf"), "{text}");
        assert!(!text.contains("Uncommitted"), "{text}");
    }

    /// Mergiraf found but unable to list its languages says so, with why, instead of
    /// suggesting to install it.
    #[test]
    fn a_failed_language_query_is_shown_with_its_error() {
        let check = done(
            Outcome::Conflicts {
                files: vec![file("a.rs", &["contents"])],
                stop: None,
            },
            0,
            vec![Mergiraf::Unchecked("Mergiraf: bad flag".into())],
        );
        let text = tooltip(Op::Merge, &check, false);
        assert!(text.contains("• a.rs — text conflict"), "{text}");
        assert!(
            text.contains("couldn't list the file types it knows: Mergiraf: bad flag"),
            "{text}"
        );
        assert!(!text.contains("installs it"), "{text}");
    }

    /// The cache answers again without asking git, keeps what is asked for often, and drops
    /// the oldest beyond its size, but never a check still running.
    #[test]
    fn the_cache_keeps_recent_checks_and_drops_old_ones() {
        let mut inner = Inner::default();
        let key = |n: usize| Key::new(Op::Merge, "head", &format!("c{n}"));
        let clean = done(Outcome::Clean { changes: true }, 0, Vec::new());
        let entry = |asked: u64, check: Check| Entry {
            asked,
            check,
            failed: None,
        };
        inner.entries.insert(key(0), entry(0, Check::Checking));
        for n in 1..=CAPACITY + 10 {
            inner.entries.insert(key(n), entry(n as u64, clean.clone()));
        }
        // The first finished one was asked for again, late.
        if let Some(entry) = inner.entries.get_mut(&key(1)) {
            entry.asked = 10_000;
        }
        inner.evict();
        assert_eq!(inner.entries.len(), CAPACITY);
        assert!(inner.entries.contains_key(&key(0)), "running");
        assert!(inner.entries.contains_key(&key(1)), "asked for again");
        for n in 2..=12 {
            assert!(!inner.entries.contains_key(&key(n)), "{n}");
        }
        assert!(inner.entries.contains_key(&key(13)));
    }

    /// A failed check (a machine out of reach) is asked again once [`RETRY`] has passed, and
    /// a check about a HEAD the branch has moved on from is dropped from the queue.
    #[test]
    fn failures_are_retried_and_stale_questions_dropped() {
        let checks = ConflictChecks::default();
        let ctx = egui::Context::default();
        let failed = Key::new(Op::Merge, "head", "x");
        let stale = Key::new(Op::Merge, "old-head", "y");
        let Some(long_ago) = Instant::now().checked_sub(RETRY * 2) else {
            return;
        };
        if let Ok(mut inner) = checks.inner.lock() {
            // No worker starts: the test looks at the queue.
            inner.workers = WORKERS;
            let entry = |check: Check, failed: Option<Instant>| Entry {
                asked: 0,
                check,
                failed,
            };
            let gone = Check::Failed("unreachable".into());
            inner
                .entries
                .insert(failed.clone(), entry(gone.clone(), Some(Instant::now())));
            inner
                .entries
                .insert(stale.clone(), entry(Check::Checking, None));
            inner.queue.push(stale.clone());
            // Just failed: the failure stands.
            drop(inner);
            assert!(matches!(
                checks.get(&ctx, Path::new("."), failed.clone()),
                Check::Failed(_)
            ));
            if let Ok(mut inner) = checks.inner.lock()
                && let Some(e) = inner.entries.get_mut(&failed)
            {
                e.failed = Some(long_ago);
            }
        }
        // Long enough ago: asked again; and the old HEAD's question is gone.
        assert!(matches!(
            checks.get(&ctx, Path::new("."), failed.clone()),
            Check::Checking
        ));
        let state = checks
            .inner
            .lock()
            .map(|inner| (inner.queue.clone(), inner.entries.contains_key(&stale)))
            .ok();
        assert_eq!(state, Some((vec![failed], false)));
        // A closed tab leaves its workers nothing to do.
        let inner = checks.inner.clone();
        drop(checks);
        assert_eq!(inner.lock().map(|i| i.queue.len()).ok(), Some(0));
    }

    /// The queue keeps the latest questions only, and a check that was running when the
    /// checks were forgotten (F5) doesn't come back with its old answer.
    #[test]
    fn the_queue_is_bounded_and_forgetting_drops_running_work() {
        let mut inner = Inner::default();
        let key = |n: usize| Key::new(Op::Merge, "head", &format!("c{n}"));
        for n in 0..MAX_QUEUE + 5 {
            inner.queue.push(key(n));
            let entry = Entry {
                asked: n as u64,
                check: Check::Checking,
                failed: None,
            };
            inner.entries.insert(key(n), entry);
        }
        inner.bound_queue();
        assert_eq!(inner.queue.len(), MAX_QUEUE);
        assert_eq!(inner.queue.first(), Some(&key(5)));
        assert!(!inner.entries.contains_key(&key(4)));
        let checks = ConflictChecks::default();
        if let Ok(mut shared) = checks.inner.lock() {
            *shared = inner;
        }
        let before = checks.inner.lock().map(|i| i.generation).ok();
        checks.forget();
        let after = checks
            .inner
            .lock()
            .map(|i| (i.generation, i.entries.len(), i.queue.len()))
            .ok();
        assert_eq!(after, before.map(|g| (g + 1, 0, 0)));
    }

    /// Through the real thing: a check starts, finishes in the background, and is answered
    /// from the cache after; a key it has is not checked again.
    #[test]
    fn checks_run_in_the_background_and_are_kept() -> Result<(), String> {
        let dir = std::env::temp_dir().join(format!("gitcraft-checks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let git = |args: &[&str]| crate::git::run(Some(&dir), args).map(|s| s.trim().to_owned());
        for args in [
            &["init", "-q", "-b", "main"][..],
            &["config", "user.name", "t"],
            &["config", "user.email", "t@t"],
            &["config", "commit.gpgSign", "false"],
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["commit", "-q", "--allow-empty", "-m", "two"],
        ] {
            git(args)?;
        }
        let head = git(&["rev-parse", "HEAD"])?;
        let base = git(&["rev-parse", "HEAD~1"])?;
        let ctx = egui::Context::default();
        let checks = ConflictChecks::default();
        // Asked about before a commit moved HEAD on, and never taken by a worker.
        let stale = Key::new(Op::Merge, "an-older-head", &base);
        if let Ok(mut inner) = checks.inner.lock() {
            inner.workers = WORKERS;
            inner.queue.push(stale.clone());
            let entry = Entry {
                asked: 0,
                check: Check::Checking,
                failed: None,
            };
            inner.entries.insert(stale.clone(), entry);
        }
        let key = Key::new(Op::Merge, &head, &base);
        let first = checks.get(&ctx, &dir, key.clone());
        let dropped = checks
            .inner
            .lock()
            .map(|mut inner| {
                inner.workers = 0;
                (inner.queue.clone(), inner.entries.contains_key(&stale))
            })
            .map_err(|e| e.to_string())?;
        assert_eq!(dropped, (vec![key.clone()], false));
        // The worker this test held back.
        if let Ok(mut inner) = checks.inner.lock() {
            inner.queue.clear();
            inner.entries.clear();
        }
        let first_again = checks.get(&ctx, &dir, key.clone());
        let started = Instant::now();
        let answer = loop {
            match checks.get(&ctx, &dir, key.clone()) {
                Check::Checking if started.elapsed() < Duration::from_secs(30) => {
                    std::thread::sleep(Duration::from_millis(20));
                }
                other => break other,
            }
        };
        let _ = std::fs::remove_dir_all(&dir);
        assert!(matches!(first, Check::Checking));
        assert!(matches!(first_again, Check::Checking));
        match answer {
            Check::Done(verdict) => {
                assert_eq!(
                    verdict.prediction.outcome,
                    Outcome::Clean { changes: false }
                );
            }
            // A git too old to merge-tree says so instead.
            Check::Unavailable(_) => {}
            other => return Err(format!("{other:?}")),
        }
        // Answered from the cache: the repository is gone, and still the same answer.
        assert!(!matches!(checks.get(&ctx, &dir, key), Check::Checking));
        Ok(())
    }
}
