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

/// The questions asked of every repository: (key, git's arguments).
fn probes() -> Vec<(&'static str, Vec<String>)> {
    let words = |args: &[&str]| args.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
    vec![
        ("branch", words(&["symbolic-ref", "-q", "HEAD"])),
        (
            "head",
            words(&["rev-parse", "-q", "--verify", "--short", "HEAD"]),
        ),
        (
            "refs",
            vec![
                "for-each-ref".to_owned(),
                "--sort=-committerdate".to_owned(),
                format!(
                    "--format=%(refname){FS}%(upstream:short){FS}%(upstream:track){FS}%(upstream)"
                ),
                "refs/heads".to_owned(),
            ],
        ),
        ("remotes", words(&["remote"])),
        (
            "status",
            words(&[
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "-z",
                "--untracked-files=all",
            ]),
        ),
        ("stashes", words(&["stash", "list", UTF8, "--format=%gs"])),
        ("worktrees", words(&["worktree", "list", "--porcelain"])),
        ("top", words(&["rev-parse", "--show-toplevel"])),
    ]
}

/// The `status` asked of each other worktree.
const WORKTREE_STATUS: [&str; 5] = [
    "--no-optional-locks",
    "status",
    "--porcelain=v1",
    "-z",
    "--untracked-files=all",
];

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

/// The sections, asked here one command at a time.
fn sections_here(repo: &Path) -> Result<Vec<Section>, String> {
    let mut sections = Vec::new();
    let ask = |key: String, dir: &Path, args: &[&str]| -> Result<Section, String> {
        let (code, out) = run_coded(dir, args)?;
        Ok(Section { key, code, out })
    };
    for (key, args) in probes() {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        sections.push(ask(key.to_owned(), repo, &args)?);
    }
    let fetched = last_fetch_here(repo);
    sections.push(Section {
        key: "fetched".to_owned(),
        code: if fetched.is_some() { 0 } else { 1 },
        out: fetched
            .map(|t| t.to_string().into_bytes())
            .unwrap_or_default(),
    });
    let has_remote = !text(find(&sections, "remotes")).trim().is_empty();
    if has_remote {
        for branch in branches_to_count(&text(find(&sections, "refs"))) {
            let full = format!("refs/heads/{branch}");
            let args = ["rev-list", "--count", &full, "--not", "--remotes"];
            sections.push(ask(format!("count:{branch}"), repo, &args)?);
        }
    }
    let top = text(find(&sections, "top"))
        .trim_end_matches(['\n', '\r'])
        .to_owned();
    for path in other_worktrees(&text(find(&sections, "worktrees")), &top) {
        let key = format!("wt:{path}");
        sections.push(ask(key, Path::new(&path), &WORKTREE_STATUS)?);
    }
    Ok(sections)
}

/// When `repo` here last fetched: the time of its `FETCH_HEAD`, which every fetch writes.
fn last_fetch_here(repo: &Path) -> Option<i64> {
    let (code, out) = run_coded(repo, &["rev-parse", "--git-common-dir"]).ok()?;
    if code != 0 {
        return None;
    }
    let dir = String::from_utf8_lossy(&out);
    let dir = repo.join(dir.trim_end_matches(['\n', '\r']));
    let modified = std::fs::metadata(dir.join("FETCH_HEAD"))
        .ok()?
        .modified()
        .ok()?;
    let secs = modified
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(secs).ok()
}

/// The sections, asked of `target`'s machine in one script.
fn sections_there(target: &crate::remote::Target, limit: Duration) -> Result<Vec<Section>, String> {
    let req = crate::remote::Request {
        cwd: Some(target.path.clone()),
        script: script(),
        env: GIT_ENV
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect(),
        unset: GIT_ENV_REMOVED.iter().map(|n| (*n).to_owned()).collect(),
        stdin: None,
        long: false,
        timeout: Some(limit),
        apart: false,
        test: false,
    };
    let out = crate::remote::exec(&target.host, &req)?;
    if out.code == 1 && crate::remote::is_missing_folder(&out.stderr) {
        return Err(format!(
            "{} isn't a folder on {}",
            target.path,
            target.host.label()
        ));
    }
    if out.code != 0 {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        return Err(if why.is_empty() {
            format!(
                "couldn't look at {} on {}",
                target.path,
                target.host.label()
            )
        } else {
            why
        });
    }
    Ok(split_sections(&out.stdout))
}

/// The script that asks every section of the repository it runs in, each printed as
/// `\x1e<code> <bytes> <key>` on a line and then its output (see [`split_sections`]).
fn script() -> String {
    use crate::remote::sh_quote;
    let quoted = |args: &[String]| {
        args.iter()
            .map(|a| sh_quote(a))
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut s = String::from(concat!(
        "GIT_NO_LAZY_FETCH=1; export GIT_NO_LAZY_FETCH\n",
        "git() { command git --no-lazy-fetch \"$@\"; }\n",
        "d=$(mktemp -d \"${TMPDIR:-/tmp}/gitcraft-look.XXXXXX\") || exit 1\n",
        "trap 'rm -rf \"$d\"' EXIT HUP INT TERM\n",
        "s() { k=$1; shift\n",
        "  if \"$@\" >\"$d/o\" 2>\"$d/e\" </dev/null; then c=0; else c=$?; mv \"$d/e\" \"$d/o\"; fi\n",
        "  printf '\\036%s %s %s\\n' \"$c\" \"$(wc -c <\"$d/o\" | tr -d ' ')\" \"$k\"; cat \"$d/o\"; }\n",
    ));
    for (key, args) in probes() {
        s.push_str(&format!("s {key} git {}\n", quoted(&args)));
    }
    s.push_str(concat!(
        "f=\"$(git rev-parse --git-common-dir 2>/dev/null)/FETCH_HEAD\"\n",
        "t() { [ -f \"$f\" ] && { stat -c %Y \"$f\" 2>/dev/null || stat -f %m \"$f\"; }; }\n",
        "s fetched t\n",
    ));
    s.push_str(&format!(
        concat!(
            "if [ -n \"$(git remote 2>/dev/null)\" ]; then\n",
            "  n=0; tab=$(printf '\\t')\n",
            "  git for-each-ref --sort=-committerdate '--format=%(refname)%09%(upstream)%09%(upstream:track)' refs/heads 2>/dev/null |\n",
            "  while IFS=\"$tab\" read -r r u t; do\n",
            "    case \"$u\" in refs/remotes/*) [ \"$t\" = '[gone]' ] || continue;; esac\n",
            "    [ \"$n\" -lt {max_b} ] || break; n=$((n + 1))\n",
            "    s \"count:${{r#refs/heads/}}\" git rev-list --count \"$r\" --not --remotes\n",
            "  done\n",
            "fi\n",
            "top=$(git rev-parse --show-toplevel 2>/dev/null)\n",
            "n=0\n",
            "git worktree list --porcelain 2>/dev/null |\n",
            "awk '/^worktree /{{p=substr($0,10)}} /^bare$/{{p=\"\"}} /^$/{{if(p!=\"\")print p; p=\"\"}} END{{if(p!=\"\")print p}}' |\n",
            "while IFS= read -r p; do\n",
            "  [ \"$p\" = \"$top\" ] && continue\n",
            "  [ \"$n\" -lt {max_w} ] || break; n=$((n + 1))\n",
            "  s \"wt:$p\" git -C \"$p\" {status}\n",
            "done\n",
            "rm -rf \"$d\"\n",
        ),
        max_b = MAX_COUNTED_BRANCHES,
        max_w = MAX_COUNTED_WORKTREES,
        status = WORKTREE_STATUS.join(" "),
    ));
    s
}

/// The sections of [`script`]'s output, in order; what doesn't read as one ends the list.
fn split_sections(mut bytes: &[u8]) -> Vec<Section> {
    let mut sections = Vec::new();
    while let Some(start) = bytes.iter().position(|&b| b == 0x1e) {
        bytes = &bytes[start + 1..];
        let Some(nl) = bytes.iter().position(|&b| b == b'\n') else {
            break;
        };
        let header = String::from_utf8_lossy(&bytes[..nl]).into_owned();
        bytes = &bytes[nl + 1..];
        let mut words = header.splitn(3, ' ');
        let (Some(code), Some(len), Some(key)) = (words.next(), words.next(), words.next()) else {
            break;
        };
        let (Ok(code), Ok(len)) = (code.parse::<i32>(), len.parse::<usize>()) else {
            break;
        };
        if len > bytes.len() {
            break;
        }
        sections.push(Section {
            key: key.to_owned(),
            code,
            out: bytes[..len].to_vec(),
        });
        bytes = &bytes[len..];
    }
    sections
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
    let status = find(sections, "status").ok_or("git gave no answer")?;
    if status.code != 0 {
        let why = String::from_utf8_lossy(&status.out).trim().to_owned();
        return Err(if why.is_empty() {
            "git status failed".to_owned()
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
