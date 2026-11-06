//! Changes to a repository on another machine, which no watcher of this one sees: asked for
//! every few seconds, with one short script that sums up what a change would show in (the refs
//! and HEAD, the index, the working tree's status and the times of the files it lists).

use std::path::Path;
use std::sync::Weak;
use std::sync::mpsc::Sender;
use std::time::Duration;

use super::watch::FsChange;
use crate::remote::{self, sh_quote};

/// How often the repository is asked.
const EVERY: Duration = Duration::from_secs(3);

/// The changed files whose times are asked, at most: an edit to a file already changed
/// changes its status in nothing else.
const TIMED_AT_MOST: usize = 200;

/// What the repository looked like at one look, section by section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Look {
    /// Refs (summed up), HEAD, an operation's state files.
    git: String,
    /// The index's time and size.
    index: String,
    /// The times and sizes of the files listed changed last time.
    times: String,
    /// `status --porcelain -z`.
    files: String,
}

/// Looks at the repository `repo` every few seconds and sends what each change calls for to
/// `tx`, until `alive` is gone (its tab closed or watching again).
pub(super) fn poll(repo: &Path, tx: &Sender<FsChange>, ctx: &egui::Context, alive: &Weak<()>) {
    let Some(target) = remote::parse(repo) else {
        return;
    };
    let mut last: Option<Look> = None;
    let mut timed: Vec<String> = Vec::new();
    while alive.upgrade().is_some() {
        let mark = new_mark();
        let req = remote::Request {
            cwd: Some(target.path.clone()),
            script: script(&timed, &mark),
            env: vec![("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned())],
            ..remote::Request::default()
        };
        // Not reached this time (the network…): next time.
        let look = remote::exec(&target.host, &req)
            .ok()
            .filter(|out| out.code == 0)
            .and_then(|out| parse(&String::from_utf8_lossy(&out.stdout), &mark));
        if let Some(look) = look {
            timed = changed_paths(&look.files);
            let change = last.as_ref().and_then(|before| compare(before, &look));
            last = Some(look);
            if let Some(change) = change {
                if tx.send(change).is_err() {
                    return;
                }
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        std::thread::sleep(EVERY);
    }
}

/// What starts each section: with `mark`, a word new for each look, which no file name will
/// hold (a file called `x@files` among the times would otherwise end them early).
fn marker(section: &str, mark: &str) -> String {
    format!("@{section}-{mark}\n")
}

/// A new `mark` for [`marker`].
fn new_mark() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}{:x}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// The script that prints a [`Look`], its sections started by `mark`'s markers; `timed` the
/// files whose times it gives. Times as GNU `stat` gives them, else as BSD's (macOS).
fn script(timed: &[String], mark: &str) -> String {
    let echo = |section: &str| format!("printf '%s' {}\n", sh_quote(&marker(section, mark)));
    let mut s = String::from(concat!(
        "g=$(git rev-parse --absolute-git-dir) || exit 1\n",
        "if stat -c %s . >/dev/null 2>&1; then o=-c; fa=\"%y %s\"; fb=\"%n %y %s\"\n",
        "else o=-f; fa=\"%m %z\"; fb=\"%N %m %z\"; fi\n",
    ));
    s.push_str(&echo("git"));
    s.push_str(concat!(
        "git for-each-ref --format='%(objectname) %(refname)' | cksum\n",
        "cat \"$g/HEAD\"\n",
        "for f in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply; do\n",
        "  [ -e \"$g/$f\" ] && echo \"$f\"\n",
        "done\n",
    ));
    s.push_str(&echo("index"));
    // Always a line: a new repository has no index yet.
    s.push_str("stat $o \"$fa\" \"$g/index\" 2>/dev/null || echo none\n");
    s.push_str(&echo("times"));
    if !timed.is_empty() {
        let names: Vec<String> = timed.iter().map(|p| sh_quote(p)).collect();
        s.push_str(&format!(
            "stat $o \"$fb\" -- {} 2>/dev/null\n",
            names.join(" ")
        ));
    }
    s.push_str(&echo("files"));
    s.push_str("exec git status --porcelain=v1 -z --untracked-files=all\n");
    s
}

/// The sections of what [`script`] printed with `mark`, `None` when it was cut short.
fn parse(text: &str, mark: &str) -> Option<Look> {
    let rest = text.strip_prefix(&marker("git", mark))?;
    let (git, rest) = rest.split_once(&marker("index", mark))?;
    let (index, rest) = rest.split_once(&marker("times", mark))?;
    let (times, files) = rest.split_once(&marker("files", mark))?;
    let (git, index) = (git.trim_end(), index.trim_end());
    Some(Look {
        git: git.to_owned(),
        index: index.to_owned(),
        times: times.to_owned(),
        files: files.to_owned(),
    })
}

/// The files `status --porcelain=v1 -z` lists (a rename's old name left out), at most
/// [`TIMED_AT_MOST`].
fn changed_paths(status: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut entries = status.split('\0').filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let (Some(code), Some(path)) = (entry.get(..2), entry.get(3..)) else {
            continue;
        };
        if code.contains(['R', 'C']) {
            entries.next();
        }
        if paths.len() < TIMED_AT_MOST {
            paths.push(path.to_owned());
        }
    }
    paths
}

/// What the difference between two looks calls for, the most of it.
fn compare(before: &Look, now: &Look) -> Option<FsChange> {
    if before.git != now.git {
        Some(FsChange::Git)
    } else if before.index != now.index {
        Some(FsChange::Index)
    } else if before.files != now.files || before.times != now.times {
        Some(FsChange::Files)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(git: &str, index: &str, times: &str, files: &str) -> Look {
        Look {
            git: git.to_owned(),
            index: index.to_owned(),
            times: times.to_owned(),
            files: files.to_owned(),
        }
    }

    #[test]
    fn a_look_is_read_by_its_sections() {
        let text = "@git-m1\n123 45\nref: refs/heads/main\n@index-m1\n2026-11-06 10:00:00.1 300\n@times-m1\na.txt 2026 3\n@files-m1\n M a.txt\0";
        assert_eq!(
            parse(text, "m1"),
            Some(look(
                "123 45\nref: refs/heads/main",
                "2026-11-06 10:00:00.1 300",
                "a.txt 2026 3\n",
                " M a.txt\0"
            ))
        );
        // No changed files: nothing between the markers.
        assert_eq!(
            parse("@git-m1\nx\n@index-m1\ny\n@times-m1\n@files-m1\n", "m1"),
            Some(look("x", "y", "", ""))
        );
        assert_eq!(parse("@git-m1\nx\n@index-m1\n", "m1"), None);
        // Another look's markers (or a file named like one) end nothing.
        assert_eq!(parse(text, "m2"), None);
    }

    /// A changed file named like a marker of the old, fixed kind stays in its section.
    #[test]
    fn file_names_end_no_section() {
        let text = "@git-m1\ng\n@index-m1\ni\n@times-m1\nx\n@files\n 1 2\n@files-m1\n M x\0";
        let parsed = parse(text, "m1").map(|l| l.times);
        assert_eq!(parsed.as_deref(), Some("x\n@files\n 1 2\n"));
    }

    #[test]
    fn changed_paths_leave_out_old_names() {
        let status = " M a.txt\0R  new.txt\0old.txt\0?? dir/b c.txt\0";
        assert_eq!(changed_paths(status), ["a.txt", "new.txt", "dir/b c.txt"]);
    }

    #[test]
    fn the_biggest_change_wins() {
        let base = look("g", "i", "t", "f");
        assert_eq!(compare(&base, &base), None);
        assert_eq!(
            compare(&base, &look("g", "i", "t2", "f")),
            Some(FsChange::Files)
        );
        assert_eq!(
            compare(&base, &look("g", "i2", "t2", "f2")),
            Some(FsChange::Index)
        );
        assert_eq!(
            compare(&base, &look("g2", "i2", "t", "f")),
            Some(FsChange::Git)
        );
    }

    #[test]
    fn the_script_asks_the_times_of_changed_files() {
        let s = script(&["it's.txt".to_owned()], "m1");
        assert!(s.contains("stat $o \"$fb\" -- 'it'\\''s.txt'"), "{s}");
        assert!(!script(&[], "m1").contains("$fb\" --"));
    }
}
