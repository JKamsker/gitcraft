//! A repository at a glance, for the views that look over many at once (workspaces, "Where's
//! my work?"): its branch and how it stands with its upstream, its uncommitted files, when it
//! was last fetched, and the work that is only here: commits on no remote, stashes, worktrees
//! with changes, branches whose remote branch is gone.
//!
//! Elsewhere (WSL, SSH) all of it is asked in one script, one round trip; here, the same git
//! commands run one after the other. Both give their answers as the same sections (see
//! [`Section`]), read by one parser, so the two can't tell a repository apart differently.

use std::path::{Path, PathBuf};
use std::time::Duration;

use super::command::{GIT_ENV, GIT_ENV_REMOVED, run_coded};
use super::status::parse_status;
use super::worktree::parse_worktrees;
use super::{FS, UTF8};

/// Of the branches with no upstream, how many get their commits on no remote counted: each is
/// a `rev-list` of its own.
pub const MAX_COUNTED_BRANCHES: usize = 40;

/// Of the other worktrees, how many get their changes counted: each is a `status` of its own.
pub const MAX_COUNTED_WORKTREES: usize = 50;

/// What one look at a repository found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overview {
    /// The branch checked out; `None` when HEAD is detached.
    pub branch: Option<String>,
    /// HEAD's commit, short; empty before the first commit.
    pub head: String,
    /// The checked-out branch's upstream (`origin/main`), empty when it has none.
    pub upstream: String,
    /// Commits of the checked-out branch its upstream doesn't have, and the other way round.
    pub ahead: u32,
    pub behind: u32,
    /// Files with uncommitted changes in this working tree.
    pub changed: usize,
    /// When the repository last fetched (Unix seconds), `None` when it never did.
    pub last_fetch: Option<i64>,
    /// Whether it has a remote at all (with none, nothing can be pushed).
    pub has_remote: bool,
    /// Local branches with commits no remote has.
    pub unpushed: Vec<Unpushed>,
    /// The stashes' messages, newest first.
    pub stashes: Vec<String>,
    /// The repository's other worktrees with uncommitted changes.
    pub dirty_worktrees: Vec<DirtyWorktree>,
    /// Local branches whose upstream was deleted on the remote.
    pub gone: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unpushed {
    pub branch: String,
    /// Its upstream, when it has one; without, the commits counted are those on no remote.
    pub upstream: Option<String>,
    pub commits: u32,
    pub tracks: Tracks,
}

/// What a branch with unpushed commits tracks, which says where a push of it would go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tracks {
    /// A remote's branch of the same name.
    SameName,
    /// A remote's branch of another name (`feature` made from `origin/main` tracks it).
    OtherName,
    /// Nothing.
    Nothing,
    /// A remote's branch that was deleted there.
    Gone,
    /// A branch of this repository.
    Local,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirtyWorktree {
    /// In the form the app gives paths (one elsewhere as `ssh://…`).
    pub path: PathBuf,
    pub changes: usize,
}

/// One answer: a key saying what was asked, the exit code, and the output (stdout when it
/// succeeded, stderr when it didn't).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Section {
    key: String,
    code: i32,
    out: Vec<u8>,
}

/// Looks at `repo`. One elsewhere is given at most `limit` to answer (it is stopped there
/// after that); one here runs to the end: the caller decides how long to wait for it.
pub fn overview(repo: &Path, limit: Duration) -> Result<Overview, String> {
    let sections = match crate::remote::parse(repo) {
        Some(target) => sections_there(&target, limit)?,
        None => sections_here(repo)?,
    };
    let host = crate::remote::parse(repo).map(|t| t.host);
    parse(&sections, |p| match &host {
        Some(host) => crate::remote::encode(host, p),
        None => PathBuf::from(p),
    })
}

/// The branches whose commits on no remote are counted: those not tracking a remote's branch
/// (none, or one of this repository) and those whose remote branch is gone, the most recently
/// committed to first, at most [`MAX_COUNTED_BRANCHES`].
fn branches_to_count(refs: &str) -> Vec<String> {
    read_refs(refs)
        .into_iter()
        .filter(|r| !r.tracks_remote() || r.gone())
        .map(|r| r.name)
        .take(MAX_COUNTED_BRANCHES)
        .collect()
}

/// The worktrees other than `top` (the one looked at), as `worktree list --porcelain` names
/// them, at most [`MAX_COUNTED_WORKTREES`].
fn other_worktrees(listed: &str, top: &str) -> Vec<String> {
    parse_worktrees(listed, '\n')
        .into_iter()
        .filter(|w| !w.bare && w.path.to_string_lossy() != top)
        .map(|w| w.path.to_string_lossy().into_owned())
        .take(MAX_COUNTED_WORKTREES)
        .collect()
}

/// A local branch as `for-each-ref` gave it.
struct LocalRef {
    name: String,
    /// Its upstream, short (`origin/main`, or `main` for a branch of this repository).
    upstream: String,
    /// `[ahead 1, behind 2]`, `[gone]`.
    track: String,
    /// Its upstream in full (`refs/remotes/origin/main`).
    upstream_ref: String,
}

impl LocalRef {
    fn tracks_remote(&self) -> bool {
        self.upstream_ref.starts_with("refs/remotes/")
    }

    fn gone(&self) -> bool {
        self.track == "[gone]"
    }

    fn ahead_behind(&self) -> (u32, u32) {
        crate::git::Branch {
            name: self.name.clone(),
            target: String::new(),
            is_head: false,
            upstream: self.upstream.clone(),
            track: self.track.clone(),
        }
        .ahead_behind()
    }
}

/// `for-each-ref`'s lines.
fn read_refs(refs: &str) -> Vec<LocalRef> {
    refs.lines()
        .filter_map(|line| {
            let mut f = line.split(FS);
            let name = f.next()?.strip_prefix("refs/heads/")?.to_owned();
            let mut next = || f.next().unwrap_or_default().to_owned();
            Some(LocalRef {
                name,
                upstream: next(),
                track: next(),
                upstream_ref: next(),
            })
        })
        .collect()
}

fn find<'a>(sections: &'a [Section], key: &str) -> Option<&'a Section> {
    sections.iter().find(|s| s.key == key)
}

/// A section's output when it succeeded; nothing when it failed or wasn't asked.
fn text(section: Option<&Section>) -> String {
    section
        .filter(|s| s.code == 0)
        .map(|s| String::from_utf8_lossy(&s.out).into_owned())
        .unwrap_or_default()
}

/// The overview the sections say; `path` gives a worktree's path in the app's form.
fn parse(sections: &[Section], path: impl Fn(&str) -> PathBuf) -> Result<Overview, String> {
    let status =
        find(sections, "status").ok_or(crate::i18n::text("backend-git-gave-no-answer-f59da3"))?;
    if status.code != 0 {
        let why = String::from_utf8_lossy(&status.out).trim().to_owned();
        return Err(if why.is_empty() {
            crate::i18n::text("backend-git-status-failed-5adb43").to_owned()
        } else {
            why
        });
    }
    let line = |key: &str| text(find(sections, key)).trim().to_owned();
    // In full: `--short` would make it `heads/main` when a tag is called `main` too.
    let branch = line("branch")
        .strip_prefix("refs/heads/")
        .map(str::to_owned);
    let refs = read_refs(&text(find(sections, "refs")));
    let remotes: Vec<String> = text(find(sections, "remotes"))
        .lines()
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .collect();
    let mut o = Overview {
        head: line("head"),
        changed: parse_status(&String::from_utf8_lossy(&status.out)).changed_files(),
        last_fetch: line("fetched").parse().ok(),
        has_remote: !line("remotes").is_empty(),
        stashes: text(find(sections, "stashes"))
            .lines()
            .map(stash_message)
            .collect(),
        ..Overview::default()
    };
    for r in &refs {
        let (ahead, behind) = r.ahead_behind();
        if branch.as_deref() == Some(r.name.as_str()) {
            o.upstream = r.upstream.clone();
            (o.ahead, o.behind) = (ahead, behind);
        }
        if r.gone() {
            o.gone.push(r.name.clone());
        } else if r.tracks_remote() && ahead > 0 {
            // By the full name: a remote's own name can hold a `/`.
            let same = remotes
                .iter()
                .any(|remote| r.upstream_ref == format!("refs/remotes/{remote}/{}", r.name));
            o.unpushed.push(Unpushed {
                branch: r.name.clone(),
                upstream: Some(r.upstream.clone()),
                commits: ahead,
                tracks: if same {
                    Tracks::SameName
                } else {
                    Tracks::OtherName
                },
            });
        }
    }
    for s in sections.iter().filter(|s| s.code == 0) {
        if let Some(name) = s.key.strip_prefix("count:") {
            let commits = String::from_utf8_lossy(&s.out).trim().parse().unwrap_or(0);
            let tracks = match refs.iter().find(|r| r.name == name) {
                Some(r) if r.gone() => Tracks::Gone,
                Some(r) if !r.upstream_ref.is_empty() => Tracks::Local,
                _ => Tracks::Nothing,
            };
            if commits > 0 {
                o.unpushed.push(Unpushed {
                    branch: name.to_owned(),
                    upstream: None,
                    commits,
                    tracks,
                });
            }
        } else if let Some(p) = s.key.strip_prefix("wt:") {
            let changes = parse_status(&String::from_utf8_lossy(&s.out)).changed_files();
            if changes > 0 {
                o.dirty_worktrees.push(DirtyWorktree {
                    path: path(p),
                    changes,
                });
            }
        }
    }
    o.branch = branch;
    Ok(o)
}

/// `On main: wip` / `WIP on main: 1234 msg` → the part after the colon, as the sidebar shows it.
fn stash_message(line: &str) -> String {
    line.split_once(": ").map_or(line, |(_, m)| m).to_owned()
}

#[cfg(test)]
#[path = "overview_tests.rs"]
mod tests;

mod transport;
#[cfg(all(test, unix))]
use transport::script;
use transport::sections_here;
use transport::sections_there;
#[cfg(test)]
use transport::split_sections;
