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
}

impl Key {
    pub fn new(op: Op, head: &str, other: &str) -> Self {
        Self {
            op,
            head: head.to_owned(),
            other: other.to_owned(),
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
    /// A text conflict, and Mergiraf isn't installed (or couldn't be asked).
    NotInstalled,
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
    /// What the git of the repository's machine can do, once asked.
    features: Option<Result<Features, String>>,
    /// Each check, with when it was last asked for.
    entries: HashMap<Key, (u64, Check)>,
    /// Checks waiting for a worker; the latest asked for is taken first.
    queue: Vec<Key>,
    workers: usize,
    /// Counts the questions, for [`Inner::entries`]' ages.
    asked: u64,
}

impl Inner {
    /// Drops the checks asked for longest ago, beyond [`CAPACITY`], but none still running.
    fn evict(&mut self) {
        let over = self.entries.len().saturating_sub(CAPACITY);
        if over == 0 {
            return;
        }
        let mut done: Vec<(u64, Key)> = self
            .entries
            .iter()
            .filter(|(_, (_, check))| !matches!(check, Check::Checking))
            .map(|(key, (asked, _))| (*asked, key.clone()))
            .collect();
        done.sort_unstable_by_key(|(asked, _)| *asked);
        for (_, key) in done.into_iter().take(over) {
            self.entries.remove(&key);
        }
    }
}

/// The checks of one repository, shared with the threads working them out.
#[derive(Clone, Default)]
pub(super) struct ConflictChecks {
    inner: Arc<Mutex<Inner>>,
}

impl ConflictChecks {
    /// Where the check `key` of `repo` is; one not asked for before starts in the background,
    /// `ctx` repainted when it is done.
    pub fn get(&self, ctx: &egui::Context, repo: &Path, key: Key) -> Check {
        let Ok(mut inner) = self.inner.lock() else {
            return Check::Failed("the conflict check stopped".to_owned());
        };
        inner.asked += 1;
        let asked = inner.asked;
        if let Some((when, check)) = inner.entries.get_mut(&key) {
            *when = asked;
            return check.clone();
        }
        inner.entries.insert(key.clone(), (asked, Check::Checking));
        inner.queue.push(key);
        inner.evict();
        if inner.workers < WORKERS {
            inner.workers += 1;
            let (shared, ctx, repo) = (self.inner.clone(), ctx.clone(), repo.to_path_buf());
            std::thread::spawn(move || work(&shared, &ctx, &repo));
        }
        Check::Checking
    }
}

/// A worker: takes checks from the queue until it is empty.
fn work(shared: &Mutex<Inner>, ctx: &egui::Context, repo: &Path) {
    loop {
        let (key, features) = {
            let Ok(mut inner) = shared.lock() else { return };
            let Some(key) = inner.queue.pop() else {
                inner.workers -= 1;
                return;
            };
            (key, inner.features.clone())
        };
        let features = match features {
            Some(features) => features,
            None => {
                let found = simulate::features(repo);
                if let Ok(mut inner) = shared.lock() {
                    inner.features = Some(found.clone());
                }
                found
            }
        };
        let check = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_check(ctx, repo, features, &key)
        }))
        .unwrap_or_else(|_| Check::Failed("internal error".to_owned()));
        if let Ok(mut inner) = shared.lock()
            && let Some((_, entry)) = inner.entries.get_mut(&key)
        {
            *entry = check;
        }
        ctx.request_repaint();
    }
}

/// Works out the check `key`.
fn run_check(
    ctx: &egui::Context,
    repo: &Path,
    features: Result<Features, String>,
    key: &Key,
) -> Check {
    let features = match features {
        Ok(features) => features,
        Err(e) => return Check::Failed(e),
    };
    let unavailable = match key.op {
        Op::Merge => features.merge_unavailable(),
        Op::Rebase | Op::CherryPick => features.replay_unavailable(),
    };
    if let Some(why) = unavailable {
        return Check::Unavailable(why);
    }
    let predicted = match key.op {
        Op::Merge => simulate::merge(repo, &key.head, &key.other),
        Op::Rebase => simulate::rebase(repo, features, &key.head, &key.other),
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
    let patterns = crate::mergiraf::found(ctx).and_then(|f| crate::mergiraf::patterns(&f).ok());
    files
        .iter()
        .map(|file| mergiraf_hint(file, patterns.as_deref()))
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
            "contents" => "both sides changed it",
            "binary" => "binary file",
            "modify/delete" => "changed on one side, deleted on the other",
            "rename/delete" => "renamed on one side, deleted on the other",
            "rename/rename" => "renamed differently on each side",
            "file/directory" | "directory/file" => "a file on one side, a folder on the other",
            "add/add" => "added on both sides",
            "submodule" => "submodule",
            other => other,
        })
        .collect();
    if words.is_empty() {
        "not a text conflict".to_owned()
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
        Check::Checking => Some(("checking…".to_owned(), Tone::Faint)),
        Check::Unavailable(_) => None,
        Check::Failed(_) => Some(("check failed".to_owned(), Tone::Faint)),
        Check::Done(verdict) => Some(match &verdict.prediction.outcome {
            Outcome::Clean { changes: false } if op == Op::CherryPick => {
                ("already applied".to_owned(), Tone::Faint)
            }
            Outcome::Clean { changes: false } if op == Op::Merge => {
                ("already merged".to_owned(), Tone::Faint)
            }
            Outcome::Clean { .. } => ("applies cleanly".to_owned(), Tone::Good),
            Outcome::Conflicts { files, .. } => (
                format!("conflicts in {}", files_count(files.len())),
                Tone::Warn,
            ),
            Outcome::Unsure(_) => ("may conflict".to_owned(), Tone::Warn),
        }),
    }
}

/// `1 file`, `3 files`.
fn files_count(n: usize) -> String {
    if n == 1 {
        "1 file".to_owned()
    } else {
        format!("{n} files")
    }
}

/// How many conflicted files a tooltip lists; the rest are counted.
const LISTED: usize = 15;

/// What hovering a checked entry says: the outcome, the conflicted files with what Mergiraf
/// could do with each, and that uncommitted changes (`dirty`) are left out.
pub(super) fn tooltip(op: Op, check: &Check, dirty: bool) -> String {
    let mut text = match check {
        Check::Checking => {
            "Checking for conflicts; nothing in the repository is changed".to_owned()
        }
        Check::Unavailable(why) => return why.clone(),
        Check::Failed(e) => format!("Couldn't check for conflicts: {e}"),
        Check::Done(verdict) => verdict_text(op, verdict),
    };
    if dirty && !matches!(check, Check::Unavailable(_)) {
        text.push_str(
            "\n\nUncommitted changes aren't part of the check; git may refuse to start if they \
             touch these files",
        );
    }
    text
}

fn verdict_text(op: Op, verdict: &Verdict) -> String {
    let commits = verdict.prediction.commits;
    match &verdict.prediction.outcome {
        Outcome::Clean { changes: false } if op == Op::CherryPick => {
            "Its changes are already on the current branch: git would stop with an empty \
             cherry-pick"
                .to_owned()
        }
        Outcome::Clean { changes: false } if op == Op::Merge => {
            "Already merged: there is nothing to bring in".to_owned()
        }
        Outcome::Clean { .. } if op == Op::Rebase && commits > 0 => {
            let all = if commits == 1 {
                "The commit applies".to_owned()
            } else {
                format!("All {commits} commits apply")
            };
            format!("{all} without conflicts")
        }
        Outcome::Clean { .. } => "No conflicts".to_owned(),
        Outcome::Unsure(why) => format!("No conflicts found, but this may conflict: {why}"),
        Outcome::Conflicts { files, stop } => {
            let mut text = match stop {
                Some(stop) if op == Op::Rebase || commits > 1 => format!(
                    "Commit {} of {commits}, {} “{}”, conflicts in {}:",
                    stop.index,
                    super::short(&stop.commit.hash),
                    stop.commit.summary,
                    files_count(files.len())
                ),
                _ => format!("Conflicts in {}:", files_count(files.len())),
            };
            text.push_str(&file_lines(files, &verdict.mergiraf));
            if let Some(stop) = stop
                && stop.index < commits
            {
                text.push_str(
                    "\n\nThe commits after it aren't checked: they depend on how this is solved.",
                );
            }
            if verdict.mergiraf.contains(&Mergiraf::NotInstalled) {
                text.push_str(
                    "\n\nMergiraf can solve many text conflicts: Settings → Git → Mergiraf \
                     installs it.",
                );
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
            Some(Mergiraf::Likely) => "Mergiraf can likely solve it".to_owned(),
            Some(Mergiraf::UnknownLanguage) => "a file type Mergiraf doesn't know".to_owned(),
            Some(Mergiraf::NotText(kinds)) => format!("{kinds}: solve by hand"),
            Some(Mergiraf::NotInstalled) | None => "text conflict".to_owned(),
        };
        text.push_str(&format!("\n• {} — {hint}", file.path));
    }
    if files.len() > LISTED {
        text.push_str(&format!("\n…and {} more", files.len() - LISTED));
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
    /// A menu entry for `op` with `other`, marked with whether it would conflict.
    pub(super) fn conflict_button(
        &self,
        ui: &mut Ui,
        text: String,
        enabled: bool,
        op: Op,
        other: &str,
    ) -> Response {
        if !enabled || self.refs.head.is_empty() || other == self.refs.head {
            return ui.add_enabled(enabled, egui::Button::new(text));
        }
        let key = Key::new(op, &self.refs.head, other);
        let check = self.conflict_checks.get(ui.ctx(), &self.path, key);
        // The entry and its mark on one line: the menu widens for them.
        let mut button = egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend);
        if let Some((words, tone)) = label(op, &check) {
            let t = Tokens::get(ui.ctx());
            let color = match tone {
                Tone::Faint => t.text_faint,
                Tone::Good => t.success,
                Tone::Warn => t.warning,
            };
            button = button.right_text(RichText::new(words).color(color).small());
        }
        ui.add(button)
            .on_hover_text(tooltip(op, &check, !self.work.is_clean()))
    }

    /// The branch the checked-out one is compared with for the toolbar's badge, and its
    /// commit: its upstream, or the default branch when it has none (or it is gone).
    fn conflict_target(&self) -> Option<(String, String, bool)> {
        let current = self.refs.current.as_deref()?;
        let branch = self.refs.local.iter().find(|b| b.name == current)?;
        let upstream = (!branch.upstream.is_empty() && !branch.upstream_gone())
            .then(|| branch.upstream.clone());
        let is_upstream = upstream.is_some();
        let name = upstream.or_else(|| self.base_ref().map(|b| b.name))?;
        if name == current {
            return None;
        }
        let target = self
            .refs
            .local
            .iter()
            .chain(self.refs.remotes.iter().flat_map(|(_, list)| list))
            .find(|b| b.name == name)?
            .target
            .clone();
        Some((name, target, is_upstream))
    }

    /// The toolbar's badge, when merging the checked-out branch's target into it would
    /// conflict. Checked once the refs have settled, and not while an operation runs.
    pub(super) fn target_conflict_badge(&mut self, ui: &mut Ui) {
        let target = self.conflict_target();
        let key = target
            .as_ref()
            .filter(|(_, commit, _)| !self.refs.head.is_empty() && *commit != self.refs.head)
            .map(|(_, commit, _)| Key::new(Op::Merge, &self.refs.head, commit));
        let watch = &mut self.target_watch;
        if watch.key != key {
            watch.key = key;
            watch.upstream = target.as_ref().is_some_and(|(_, _, upstream)| *upstream);
            watch.target = target.map(|(name, _, _)| name).unwrap_or_default();
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
        let check = self.conflict_checks.get(ui.ctx(), &self.path, key);
        let Check::Done(verdict) = &check else {
            return;
        };
        let Outcome::Conflicts { .. } = &verdict.prediction.outcome else {
            return;
        };
        let t = Tokens::get(ui.ctx());
        let target = self.target_watch.target.clone();
        let current = self.refs.current.clone().unwrap_or_default();
        ui.add_space(6.0);
        let why = if self.target_watch.upstream {
            "its upstream"
        } else {
            "the default branch; it has no upstream"
        };
        super::widgets::chip(ui, &format!("Conflicts with {target}"), t.warning).on_hover_text(
            format!(
                "Merging {target} ({why}) into {current} would conflict.\n{}",
                tooltip(Op::Merge, &check, !self.work.is_clean())
            ),
        );
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

    /// The cache answers again without asking git, keeps what is asked for often, and drops
    /// the oldest beyond its size, but never a check still running.
    #[test]
    fn the_cache_keeps_recent_checks_and_drops_old_ones() {
        let mut inner = Inner::default();
        let key = |n: usize| Key::new(Op::Merge, "head", &format!("c{n}"));
        let clean = done(Outcome::Clean { changes: true }, 0, Vec::new());
        inner.entries.insert(key(0), (0, Check::Checking));
        for n in 1..=CAPACITY + 10 {
            inner.entries.insert(key(n), (n as u64, clean.clone()));
        }
        // The first finished one was asked for again, late.
        if let Some(entry) = inner.entries.get_mut(&key(1)) {
            entry.0 = 10_000;
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
            &["commit", "-q", "--allow-empty", "-m", "base"],
            &["commit", "-q", "--allow-empty", "-m", "two"],
        ] {
            git(args)?;
        }
        let head = git(&["rev-parse", "HEAD"])?;
        let base = git(&["rev-parse", "HEAD~1"])?;
        let ctx = egui::Context::default();
        let checks = ConflictChecks::default();
        let key = Key::new(Op::Merge, &head, &base);
        let first = checks.get(&ctx, &dir, key.clone());
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
