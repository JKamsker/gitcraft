//! Changes to a repository on another machine, which no watcher of this one sees: asked for
//! every few seconds, with one short script that sums up what a change would show in (the refs
//! and HEAD, the index, the submodules, the working tree's status and the times of its changed
//! files).

use std::path::Path;
use std::sync::Weak;
use std::sync::mpsc::Sender;
use std::time::Duration;

use super::messages::Msg;
use super::watch::FsChange;
use crate::remote::{self, Target, sh_quote};

/// How often the repository is asked.
const EVERY: Duration = Duration::from_secs(3);

/// How often while the window isn't focused: a change shows anyway on coming back (that
/// reloads), and the machine isn't kept busy for nobody.
const EVERY_UNFOCUSED: Duration = Duration::from_secs(15);

/// What the repository looked like at one look, section by section.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Look {
    /// Refs (summed up), HEAD, an operation's state files.
    git: String,
    /// The index's time and size.
    index: String,
    /// `.gitmodules` (summed up) and the commit each submodule is at.
    subs: String,
    /// The times and sizes of the files changed or untracked, summed up.
    times: String,
    /// `status --porcelain -z`.
    files: String,
}

/// Looks at the repository `repo` every few seconds and sends what each change calls for to
/// `tx`, until `alive` is gone (its tab closed or watching again). Once it has a first look
/// (what later ones are compared with), says so to the tab on `arrived`: what changed before
/// that, between the tab's first read and the first look, showed in no look, so the tab
/// checks again, as it does when a watcher of this machine is set up.
pub(super) fn poll(
    repo: &Path,
    tx: &Sender<FsChange>,
    arrived: &Sender<Msg>,
    ctx: &egui::Context,
    alive: &Weak<()>,
) {
    let Some(target) = remote::parse(repo) else {
        return;
    };
    let mut last: Option<Look> = None;
    while alive.upgrade().is_some() {
        // Not reached this time (the network…): next time.
        if let Some(look) = look_at(&target) {
            let change = match &last {
                Some(before) => compare(before, &look),
                None => {
                    if arrived.send(Msg::Watcher(None)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                    None
                }
            };
            last = Some(look);
            if let Some(change) = change {
                if tx.send(change).is_err() {
                    return;
                }
                ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        let focused = ctx.input(|i| i.focused);
        std::thread::sleep(if focused { EVERY } else { EVERY_UNFOCUSED });
    }
}

/// One look at the repository `target` names; `None` when it couldn't be had.
fn look_at(target: &Target) -> Option<Look> {
    let mark = new_mark();
    let req = remote::Request {
        cwd: Some(target.path.clone()),
        script: script(&mark),
        env: vec![("GIT_OPTIONAL_LOCKS".to_owned(), "0".to_owned())],
        // A look that hangs is given up (and stopped) before the next.
        timeout: Some(Duration::from_secs(60)),
        ..remote::Request::default()
    };
    remote::exec(&target.host, &req)
        .ok()
        .filter(|out| out.code == 0)
        .and_then(|out| parse(&String::from_utf8_lossy(&out.stdout), &mark))
}

/// What starts each section: with `mark`, a word new for each look, which no file name will
/// hold.
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

/// The script that prints a [`Look`], its sections started by `mark`'s markers.
///
/// Times as GNU `stat` gives them (to the nanosecond), else as BSD's (macOS) with fractions of
/// a second where it gives them, so that an edit within the second a file was last written,
/// keeping its size, still shows. The changed and untracked files' times are taken there (all
/// of them, `xargs` making as many `stat`s as needed) and summed up, so a look stays short
/// however many there are. A file already changed keeps its status when edited again: only its
/// time tells.
fn script(mark: &str) -> String {
    let echo = |section: &str| format!("printf '%s' {}\n", sh_quote(&marker(section, mark)));
    let mut s = String::from(concat!(
        // Relative to this folder, the one the script runs in, where it isn't absolute
        // (`--absolute-git-dir` is newer than some git still around).
        "g=$(git rev-parse --git-dir) || exit 1\n",
        "if stat -c %s . >/dev/null 2>&1; then o=-c; fa=\"%y %s\"; fb=\"%n %y %s\"\n",
        "elif stat -f %Fm . >/dev/null 2>&1; then o=-f; fa=\"%Fm %z\"; fb=\"%N %Fm %z\"\n",
        "else o=-f; fa=\"%m %z\"; fb=\"%N %m %z\"; fi\n",
    ));
    s.push_str(&echo("git"));
    s.push_str(concat!(
        // Notes (the test lane's results, written once per test) change nothing shown.
        "git for-each-ref --format='%(objectname) %(refname)' | grep -v ' refs/notes/' | cksum\n",
        "cat \"$g/HEAD\"\n",
        "for f in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply BISECT_START; do\n",
        "  [ -e \"$g/$f\" ] && echo \"$f\"\n",
        "done\n",
    ));
    s.push_str(&echo("index"));
    // Always a line: a new repository has no index yet.
    s.push_str("stat $o \"$fa\" \"$g/index\" 2>/dev/null || echo none\n");
    s.push_str(&echo("subs"));
    // A submodule moved from one commit to another, both other than the index's, keeps its
    // status; `.gitmodules` edited (a URL) changes nothing else.
    s.push_str(concat!(
        "if [ -f .gitmodules ]; then\n",
        "  cksum < .gitmodules\n",
        "  git submodule foreach --quiet --recursive ",
        "'echo \"$(pwd) $(git rev-parse HEAD 2>/dev/null)\"' 2>/dev/null\n",
        "fi\n",
    ));
    s.push_str(&echo("times"));
    s.push_str(
        "git ls-files -z -m -o --exclude-standard | xargs -0 stat $o \"$fb\" -- 2>/dev/null | cksum\n",
    );
    s.push_str(&echo("files"));
    s.push_str("exec git status --porcelain=v1 -z --untracked-files=all\n");
    s
}

/// The sections of what [`script`] printed with `mark`, `None` when it was cut short.
fn parse(text: &str, mark: &str) -> Option<Look> {
    let rest = text.strip_prefix(&marker("git", mark))?;
    let (git, rest) = rest.split_once(&marker("index", mark))?;
    let (index, rest) = rest.split_once(&marker("subs", mark))?;
    let (subs, rest) = rest.split_once(&marker("times", mark))?;
    let (times, files) = rest.split_once(&marker("files", mark))?;
    Some(Look {
        git: git.trim_end().to_owned(),
        index: index.trim_end().to_owned(),
        subs: subs.trim_end().to_owned(),
        times: times.trim_end().to_owned(),
        files: files.to_owned(),
    })
}

/// What the difference between two looks calls for, the most of it.
fn compare(before: &Look, now: &Look) -> Option<FsChange> {
    if before.git != now.git {
        Some(FsChange::Git)
    } else if before.index != now.index || before.subs != now.subs {
        // The status and the submodules read again.
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

    fn look(git: &str, index: &str, subs: &str, times: &str, files: &str) -> Look {
        Look {
            git: git.to_owned(),
            index: index.to_owned(),
            subs: subs.to_owned(),
            times: times.to_owned(),
            files: files.to_owned(),
        }
    }

    #[test]
    fn a_look_is_read_by_its_sections() {
        let text = concat!(
            "@git-m1\n123 45\nref: refs/heads/main\n",
            "@index-m1\n2026-11-06 10:00:00.1 300\n",
            "@subs-m1\n99 12\n/r/sub abc\n",
            "@times-m1\n4 5\n",
            "@files-m1\n M a.txt\0"
        );
        assert_eq!(
            parse(text, "m1"),
            Some(look(
                "123 45\nref: refs/heads/main",
                "2026-11-06 10:00:00.1 300",
                "99 12\n/r/sub abc",
                "4 5",
                " M a.txt\0"
            ))
        );
        // No submodules, no changed files: nothing between those markers.
        assert_eq!(
            parse(
                "@git-m1\nx\n@index-m1\ny\n@subs-m1\n@times-m1\nz\n@files-m1\n",
                "m1"
            ),
            Some(look("x", "y", "", "z", ""))
        );
        assert_eq!(parse("@git-m1\nx\n@index-m1\n", "m1"), None);
        // Another look's markers (or a file named like one) end nothing.
        assert_eq!(parse(text, "m2"), None);
    }

    #[test]
    fn the_biggest_change_wins() {
        let base = look("g", "i", "s", "t", "f");
        assert_eq!(compare(&base, &base), None);
        assert_eq!(
            compare(&base, &look("g", "i", "s", "t2", "f")),
            Some(FsChange::Files)
        );
        assert_eq!(
            compare(&base, &look("g", "i", "s2", "t", "f")),
            Some(FsChange::Index)
        );
        assert_eq!(
            compare(&base, &look("g", "i2", "s", "t2", "f2")),
            Some(FsChange::Index)
        );
        assert_eq!(
            compare(&base, &look("g2", "i2", "s2", "t", "f")),
            Some(FsChange::Git)
        );
    }

    /// Every changed and untracked file's time is taken there, without a list sent along.
    #[test]
    fn the_script_times_every_changed_file() {
        let s = script("m1");
        assert!(
            s.contains("git ls-files -z -m -o --exclude-standard | xargs -0 stat"),
            "{s}"
        );
        assert!(s.contains("%Fm"), "{s}");
        assert!(s.contains("git submodule foreach"), "{s}");
        assert!(!s.contains("--absolute-git-dir"), "{s}");
    }

    /// The machines `GITCRAFT_TEST_WSL` and `GITCRAFT_TEST_SSH` name (see `remote::live_tests`).
    fn test_hosts() -> Vec<remote::Host> {
        let mut hosts = Vec::new();
        if let Ok(distro) = std::env::var("GITCRAFT_TEST_WSL") {
            hosts.push(remote::Host::Wsl(distro));
        }
        if let Ok(ssh) = std::env::var("GITCRAFT_TEST_SSH")
            && let Some((host, _)) = ssh.split_once(':')
        {
            hosts.push(remote::Host::Ssh(host.to_owned()));
        }
        hosts
    }

    /// Runs `script` in `dir` on `host`; whether it went through.
    fn run_there(host: &remote::Host, dir: &str, script: &str) -> Result<(), String> {
        let out = remote::exec(
            host,
            &remote::Request {
                cwd: Some(dir.to_owned()),
                script: script.to_owned(),
                ..remote::Request::default()
            },
        )?;
        if out.code == 0 {
            Ok(())
        } else {
            Err(format!(
                "{script}: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    /// Against a real machine (skipped without one), in a scratch repository: an edit that
    /// keeps a file's status and size, an untracked file edited, `.gitmodules` edited, a
    /// submodule moved between two commits other than the index's — each shows.
    #[test]
    fn edits_that_keep_the_status_still_show() -> Result<(), String> {
        for host in test_hosts() {
            let dir = format!("/tmp/gitcraft-poll-{}", std::process::id());
            run_there(
                &host,
                "/",
                &format!(
                    concat!(
                        "rm -rf {d} && mkdir -p {d}/src {d}/top && cd {d}/src && git init -q && ",
                        "git config user.email t@t && git config user.name t && ",
                        "for n in 1 2 3; do echo $n > f && git add f && git commit -qm $n; done && ",
                        "cd {d}/top && git init -q && ",
                        "git config user.email t@t && git config user.name t && ",
                        "echo aaaa > a.txt && git add a.txt && git commit -qm one && ",
                        "git -c protocol.file.allow=always submodule add -q {d}/src sub && ",
                        "git commit -qm sub && echo bbbb > a.txt && echo u > new.txt"
                    ),
                    d = dir
                ),
            )?;
            let top = format!("{dir}/top");
            let target = Target {
                host: host.clone(),
                path: top.clone(),
            };
            let result = (|| {
                let mut before = look_at(&target).ok_or("no first look")?;
                let mut step = |script: &str, want: FsChange| -> Result<(), String> {
                    run_there(&host, &top, script)?;
                    let now = look_at(&target).ok_or("no look")?;
                    assert_eq!(compare(&before, &now), Some(want), "{host:?}: {script}");
                    before = now;
                    Ok(())
                };
                // Same status, same size: only the time tells.
                step("echo cccc > a.txt", FsChange::Files)?;
                step("echo v > new.txt", FsChange::Files)?;
                // The submodule off the index's commit, then off it to another.
                step("git -C sub checkout -q HEAD~1", FsChange::Index)?;
                step("git -C sub checkout -q HEAD~1", FsChange::Index)?;
                // `.gitmodules` edited once (its status changes) and again (it doesn't).
                step("echo '# one' >> .gitmodules", FsChange::Index)?;
                step("echo '# two' >> .gitmodules", FsChange::Index)?;
                // Nothing done: nothing seen.
                let again = look_at(&target).ok_or("no look")?;
                assert_eq!(compare(&before, &again), None, "{host:?}");
                Ok::<(), String>(())
            })();
            let _ = run_there(&host, "/", &format!("rm -rf {dir}"));
            result?;
        }
        Ok(())
    }
}
