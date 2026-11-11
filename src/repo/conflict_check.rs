//! Conflict warnings: whether a merge, rebase or cherry-pick offered in a menu would conflict,
//! shown beside it before it is clicked, and a toolbar badge when the checked-out branch would
//! conflict with its target. Worked out with [`git::simulate`] in the background when a menu
//! first asks, a few at a time, and kept by the commits involved: they never change, so a menu
//! opened again answers at once.

#[path = "core_seams/conflict_badges.rs"]
mod conflict_badges;
#[allow(unused_imports)]
pub(crate) use conflict_badges::*;

#[path = "core_seams/conflict_labels.rs"]
mod conflict_labels;
#[allow(unused_imports)]
pub(crate) use conflict_labels::*;

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

#[cfg(test)]
#[path = "direct_tests/conflict_check_tests.rs"]
mod tests;
