//! Bisecting: what `git bisect` has going on in a worktree (its marks, the commit being tested,
//! how far it has come, how it ended), read from git's own files and refs, so that a bisect
//! started in a terminal is the same as one started here; and the commands that move it on.
//! Everything goes through git on the repository's machine.

mod parse;
#[cfg(test)]
mod tests;

use std::path::Path;

use super::{WorkStatus, not_an_option, run};

pub use parse::steps_left;

/// The words a bisect calls the two kinds of commits (`git bisect start --term-new=…`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terms {
    /// For the commits that have the change looked for (`bad`, or `new`…).
    pub bad: String,
    /// For those that don't have it yet (`good`, or `old`…).
    pub good: String,
}

impl Default for Terms {
    fn default() -> Self {
        Self {
            bad: "bad".to_owned(),
            good: "good".to_owned(),
        }
    }
}

/// What a commit was marked in the bisect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Good,
    Bad,
    Skip,
}

/// The commits marked so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub bad: Option<String>,
    pub good: Vec<String>,
    pub skipped: Vec<String>,
}

/// How far a bisect with a bad and a good commit has come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    /// Commits still to test after the one checked out.
    pub left: usize,
    /// About how many more tests it takes (see [`steps_left`]).
    pub steps: u32,
}

/// How a bisect ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The commit that brought the change in.
    FirstBad(String),
    /// Only skipped commits were left: it is one of these.
    OnlySkipped(Vec<String>),
}

/// A bisect going on in a worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bisect {
    pub terms: Terms,
    pub marks: Marks,
    /// The commit being tested: the one checked out (or `BISECT_HEAD` for a bisect started
    /// with `--no-checkout`).
    pub current: String,
    /// Where `git bisect reset` goes back to: the branch (or commit) checked out when it began.
    pub start: String,
    /// Once there are a bad and a good commit, and it hasn't ended.
    pub progress: Option<Progress>,
    pub outcome: Option<Outcome>,
}

impl Bisect {
    /// What `hash` was marked, if anything.
    pub fn mark_of(&self, hash: &str) -> Option<Mark> {
        if self.marks.bad.as_deref() == Some(hash) {
            Some(Mark::Bad)
        } else if self.marks.good.iter().any(|h| h == hash) {
            Some(Mark::Good)
        } else if self.marks.skipped.iter().any(|h| h == hash) {
            Some(Mark::Skip)
        } else {
            None
        }
    }

    /// The word git takes for `mark` in this bisect (`git bisect <word>`).
    pub fn word(&self, mark: Mark) -> &str {
        match mark {
            Mark::Good => &self.terms.good,
            Mark::Bad => &self.terms.bad,
            Mark::Skip => "skip",
        }
    }

    /// The branch it goes back to at the end, when it began on one.
    pub fn start_branch(&self) -> Option<&str> {
        let looks_like_hash =
            self.start.len() >= 40 && self.start.chars().all(|c| c.is_ascii_hexdigit());
        (!self.start.is_empty() && !looks_like_hash).then_some(self.start.as_str())
    }
}

/// The bisect going on in the worktree `repo`, if any. Asked of git (its log, refs and files,
/// which are this worktree's own), so it works the same for one started in a terminal.
pub fn bisect_state(repo: &Path) -> Result<Option<Bisect>, String> {
    let log = match run(Some(repo), &["bisect", "log"]) {
        Ok(log) => log,
        Err(e) if parse::not_bisecting(&e) => return Ok(None),
        Err(e) => return Err(format!("Couldn't read the bisect: {e}")),
    };
    // Side by side: for a repository elsewhere, each is a round trip or more.
    let (files, refs, current) = std::thread::scope(|scope| {
        let files = scope.spawn(crate::connections::inherit(|| Files::read(repo)));
        let refs = scope.spawn(crate::connections::inherit(|| bisect_refs(repo)));
        let current = current_commit(repo);
        let join = |_| "could not read the bisect".to_owned();
        Ok::<_, String>((
            files.join().map_err(join)??,
            refs.join().map_err(join)??,
            current?,
        ))
    })?;
    let terms = parse::terms(files.terms.as_deref());
    let marks = parse::marks(&refs, &terms);
    let mut outcome = parse::outcome(&log, &terms.bad);
    let progress = match (&outcome, &marks.bad) {
        // Marks git took but can't bisect between (a good commit after the bad one) leave
        // nothing to count: no progress, the rest of the state still shown.
        (None, Some(bad)) if !marks.good.is_empty() => range_vars(repo, bad, &marks.good, &files)
            .ok()
            .filter(|vars| vars.candidates > 0)
            .map(|vars| Progress {
                left: vars.left,
                steps: steps_left(vars.candidates),
            }),
        _ => None,
    };
    // Only the bad commit left: it is the first bad one, however a git version words its log.
    if outcome.is_none()
        && progress.is_some_and(|p| p.steps == 0 && p.left == 0)
        && let Some(bad) = &marks.bad
        && range_is_only(repo, bad, &marks.good, &files)
    {
        outcome = Some(Outcome::FirstBad(bad.clone()));
    }
    let progress = progress.filter(|_| outcome.is_none());
    Ok(Some(Bisect {
        terms,
        marks,
        current,
        start: files.start.unwrap_or_default().trim().to_owned(),
        progress,
        outcome,
    }))
}

/// The files of the bisect in the worktree's git folder that say how it was started.
struct Files {
    terms: Option<String>,
    start: Option<String>,
    /// The pathspec it is limited to.
    names: Vec<String>,
    first_parent: bool,
}

impl Files {
    fn read(repo: &Path) -> Result<Self, String> {
        let names = [
            "BISECT_TERMS",
            "BISECT_START",
            "BISECT_NAMES",
            "BISECT_FIRST_PARENT",
        ];
        let mut args = vec!["rev-parse"];
        for name in names {
            args.extend(["--git-path", name]);
        }
        let paths = run(Some(repo), &args)?;
        let paths: Vec<&str> = paths.lines().map(str::trim).collect();
        let [terms, start, names, first_parent] = paths[..] else {
            return Err(format!("git named no bisect files: {}", paths.join(" ")));
        };
        // Side by side: for a repository elsewhere, each is a round trip.
        std::thread::scope(|scope| {
            let terms = scope.spawn(crate::connections::inherit(|| optional_file(repo, terms)));
            let start = scope.spawn(crate::connections::inherit(|| optional_file(repo, start)));
            let names = scope.spawn(crate::connections::inherit(|| optional_file(repo, names)));
            let first_parent = optional_file(repo, first_parent)?.is_some();
            let join = |_| "could not read the bisect's files".to_owned();
            Ok(Self {
                terms: terms.join().map_err(join)??,
                start: start.join().map_err(join)??,
                names: parse::names(&names.join().map_err(join)??.unwrap_or_default()),
                first_parent,
            })
        })
    }
}

/// The file git named `printed` for `repo`, `None` when there is none.
fn optional_file(repo: &Path, printed: &str) -> Result<Option<String>, String> {
    match crate::remote::read_to_string(&crate::remote::resolve(repo, printed)) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("could not read the bisect's {printed}: {e}")),
    }
}

/// The bisect's refs, as `<hash> <name>` lines.
fn bisect_refs(repo: &Path) -> Result<String, String> {
    run(
        Some(repo),
        &[
            "for-each-ref",
            "--format=%(objectname) %(refname)",
            "refs/bisect/",
        ],
    )
}

/// The commit being tested.
fn current_commit(repo: &Path) -> Result<String, String> {
    let verify = |name| run(Some(repo), &["rev-parse", "--verify", "-q", name]);
    verify("BISECT_HEAD")
        .or_else(|_| verify("HEAD"))
        .map(|h| h.trim().to_owned())
        .map_err(|e| format!("could not read the commit being tested: {e}"))
}

/// The counts of what is left between `bad` and the `good` ones, as git bisects it.
fn range_vars(
    repo: &Path,
    bad: &str,
    good: &[String],
    files: &Files,
) -> Result<parse::Vars, String> {
    let mut args = vec!["rev-list", "--bisect-vars"];
    if files.first_parent {
        args.push("--first-parent");
    }
    args.push(bad);
    args.push("--not");
    args.extend(good.iter().map(String::as_str));
    args.push("--");
    args.extend(files.names.iter().map(String::as_str));
    let out = run(Some(repo), &args)?;
    parse::vars(&out).ok_or_else(|| format!("git gave no counts for the bisect: {}", out.trim()))
}

/// Whether `bad` is the only commit left between it and the `good` ones.
fn range_is_only(repo: &Path, bad: &str, good: &[String], files: &Files) -> bool {
    let mut args = vec!["rev-list", "--count"];
    if files.first_parent {
        args.push("--first-parent");
    }
    args.push(bad);
    args.push("--not");
    args.extend(good.iter().map(String::as_str));
    args.push("--");
    args.extend(files.names.iter().map(String::as_str));
    run(Some(repo), &args).is_ok_and(|n| n.trim() == "1")
}

/// Why a bisect can't start in this working tree, if it can't: it checks out commit after
/// commit, which changes in the working tree would be carried into (or refused by), and an
/// operation left half done would be in the way.
pub fn start_blocker(work: &WorkStatus, bisecting: bool) -> Option<String> {
    if bisecting {
        return Some("A bisect is already going on: finish or stop it first.".to_owned());
    }
    if let Some(state) = work.in_progress {
        return Some(format!(
            "A {} is in progress: finish or abort it first.",
            state.label().to_lowercase()
        ));
    }
    if !work.conflicted.is_empty() {
        return Some("There are conflicts: resolve them first.".to_owned());
    }
    let changed = !work.staged.is_empty()
        || work
            .unstaged
            .iter()
            .any(|f| f.area != super::Area::Untracked);
    changed.then(|| {
        "There are uncommitted changes, which every commit checked out would carry along: \
         commit or stash them first."
            .to_owned()
    })
}

/// The commit `rev` names, or why it names none.
fn commit_of(repo: &Path, rev: &str) -> Result<String, String> {
    let rev = not_an_option(rev.trim())?;
    let spec = format!("{rev}^{{commit}}");
    run(Some(repo), &["rev-parse", "--verify", "-q", &spec])
        .map(|h| h.trim().to_owned())
        .map_err(|_| format!("{rev} is not a commit here"))
}

/// Starts a bisect between the commit `bad` and, when given, the commit `good` (else one is
/// marked afterwards); both may be any revision (`HEAD`, a tag…). Gives what git said.
pub fn bisect_start(repo: &Path, bad: &str, good: Option<&str>) -> Result<String, String> {
    let bad = commit_of(repo, bad)?;
    let good = good.map(|g| commit_of(repo, g)).transpose()?;
    if good.as_deref() == Some(bad.as_str()) {
        return Err("The good and the bad commit are the same one".to_owned());
    }
    let mut args = vec!["bisect", "start", bad.as_str()];
    args.extend(good.as_deref());
    // No paths: what follows the revisions would be read as some.
    args.push("--");
    run(Some(repo), &args).map(|out| out.trim().to_owned())
}

/// Marks `rev` (the commit being tested when `None`) as `mark`, with the bisect's own word
/// for it. Gives what git said: the next commit to test, or the end. Only skipped commits
/// left is an end too, not a failure.
pub fn bisect_mark(
    repo: &Path,
    bisect: &Bisect,
    mark: Mark,
    rev: Option<&str>,
) -> Result<String, String> {
    let hash = rev.map(|r| commit_of(repo, r)).transpose()?;
    let mut args = vec!["bisect", bisect.word(mark)];
    args.extend(hash.as_deref());
    match run(Some(repo), &args) {
        Ok(out) => Ok(out.trim().to_owned()),
        Err(e) if parse::only_skipped_left(&e) => Ok(e),
        Err(e) => Err(e),
    }
}

/// Ends the bisect and checks out again what was checked out when it began. When that can't
/// be checked out any more (the branch was deleted), it ends it where it is, and says so.
pub fn bisect_reset(repo: &Path) -> Result<String, String> {
    match run(Some(repo), &["bisect", "reset"]) {
        Ok(out) => Ok(out.trim().to_owned()),
        Err(e) if e.contains("could not check out original HEAD") => {
            run(Some(repo), &["bisect", "reset", "HEAD"])?;
            Err(format!(
                "{} Ended the bisect where it was instead.",
                e.lines().next().unwrap_or_default().trim()
            ))
        }
        Err(e) => Err(e),
    }
}

/// The arguments of `git bisect run` for a command typed as it would be in a shell (`cargo
/// test`, `make && ./check.sh`): given to the shell git runs it with, by `eval`, a builtin of
/// every `sh` (and so there whatever the PATH, even on Windows).
pub fn bisect_run_args(command: &str) -> Vec<String> {
    ["bisect", "run", "eval", command.trim()]
        .map(str::to_owned)
        .to_vec()
}
