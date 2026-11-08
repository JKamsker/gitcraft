//! The matcher against git's real output (`samples/`, named after the git version, or the
//! platform, they come from), and the fixes on real repositories.

use std::path::{Path, PathBuf};

use super::fix::{self, ConfigEdit, Scope};
use super::lock;
use super::{GitProblem, explain};
use crate::git::run;

/// The error of `r`, which should have failed (`what`).
fn failure<T: std::fmt::Debug>(r: Result<T, String>, what: &str) -> Result<String, String> {
    match r {
        Ok(v) => Err(format!("{what} worked: {v:?}")),
        Err(e) => Ok(e),
    }
}

/// The sample `name` (in `samples/`).
fn sample(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/git/errors/samples")
        .join(name);
    // A sample missing reads as a message no expectation matches.
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| format!("missing sample {}: {e}", path.display()))
}

#[test]
fn dubious_ownership_in_every_wording() {
    let cases = [
        // Captured: a folder owned by another user, with a space and letters outside ASCII.
        ("dubious_2.43.txt", "/home/me/work/dub dir/ünï"),
        ("unsafe_2.35.2.txt", "/home/me/work/repo"),
        ("dubious_2.45_windows.txt", "D:/Projects/My Repo"),
        // A network share: what git says to add, not the folder it names.
        ("dubious_2.40_share.txt", "%(prefix)///server/share/repo"),
    ];
    for (name, path) in cases {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::DubiousOwnership {
                path: path.to_owned()
            }),
            "{name}"
        );
    }
    assert!(super::is_dubious_ownership(&sample("dubious_2.43.txt")));
    assert!(!super::is_dubious_ownership(&sample("lock_2.43.txt")));
}

#[test]
fn dubious_ownership_never_trusts_everything() {
    let err = "fatal: detected dubious ownership in repository at '*'\n\
               To add an exception for this directory, call:\n\n\
               \tgit config --global --add safe.directory '*'";
    assert_eq!(explain(err), None);
    // A quote in the name, as git quotes it for a shell.
    let err = "fatal: detected dubious ownership in repository at '/srv/it's'\n\
               \tgit config --global --add safe.directory '/srv/it'\\''s'";
    assert_eq!(
        explain(err),
        Some(GitProblem::DubiousOwnership {
            path: "/srv/it's".to_owned()
        })
    );
}

#[test]
fn locks_of_the_index_and_of_refs() {
    for (name, lock) in [
        ("lock_2.43.txt", "/home/me/work/r/.git/index.lock"),
        ("lock_commit_2.43.txt", "/home/me/work/r/.git/index.lock"),
        ("lock_2.8.txt", "/home/me/work/r/.git/index.lock"),
        (
            "lock_ref_2.43.txt",
            "/home/me/work/r/.git/refs/heads/master.lock",
        ),
        (
            "lock_windows.txt",
            "C:/Users/Me/source/my repo/.git/index.lock",
        ),
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::StaleLock {
                lock: lock.to_owned()
            }),
            "{name}"
        );
    }
    // Not a lock git made, or not because it exists.
    assert_eq!(
        explain("fatal: Unable to create '/x/.git/index.lock': Permission denied"),
        None
    );
}

#[test]
fn line_endings_warned_and_refused() {
    let lf_to_crlf = |files: &[&str], fatal| GitProblem::LineEndings {
        files: files.iter().map(|f| (*f).to_owned()).collect(),
        to_crlf: true,
        fatal,
    };
    assert_eq!(
        explain(&sample("crlf_warn_2.43.txt")),
        Some(lf_to_crlf(&["lf.txt"], false))
    );
    assert_eq!(
        explain(&sample("crlf_warn_2.36.txt")),
        Some(lf_to_crlf(&["src/main.rs", "README.md"], false))
    );
    assert_eq!(
        explain(&sample("crlf_fatal_2.43.txt")),
        Some(lf_to_crlf(&["lf2.txt"], true))
    );
    assert_eq!(
        explain(&sample("crlf_input_fatal_2.43.txt")),
        Some(GitProblem::LineEndings {
            files: vec!["crlf.txt".to_owned()],
            to_crlf: false,
            fatal: true,
        })
    );
    // Names with spaces, quotes and letters outside ASCII.
    let warn = "warning: in the working copy of 'dir/ä b's.txt', LF will be replaced by CRLF \
                the next time Git touches it";
    assert_eq!(explain(warn), Some(lf_to_crlf(&["dir/ä b's.txt"], false)));
}

#[test]
fn line_endings_give_way_to_the_real_error() {
    let err = format!(
        "{}\n{}",
        sample("crlf_warn_2.43.txt"),
        sample("lock_2.43.txt")
    );
    assert!(matches!(explain(&err), Some(GitProblem::StaleLock { .. })));
}

#[test]
fn many_line_ending_warnings_are_kept_short() -> Result<(), String> {
    let err: String = (0..5000)
        .map(|i| format!("warning: in the working copy of 'f{i}.txt', LF will be replaced by CRLF the next time Git touches it\n"))
        .collect();
    let Some(GitProblem::LineEndings { files, .. }) = explain(&err) else {
        return Err("not line endings".to_owned());
    };
    assert_eq!(files.len(), super::MAX_FILES);
    assert_eq!(files[0], "f0.txt");
    Ok(())
}

#[test]
fn names_too_long_here_and_on_windows() {
    for name in [
        "long_checkout_2.43.txt",
        "longpath_windows.txt",
        "longpath_dir_windows.txt",
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::NameTooLong),
            "{name}"
        );
    }
}

#[test]
fn certificate_problems_of_every_tls_library() {
    for (name, detail) in [
        (
            "ssl_2.43.txt",
            "server certificate verification failed. CAfile: /root/.ccr/ca-bundle.crt CRLfile: none",
        ),
        (
            "ssl_openssl.txt",
            "SSL certificate problem: unable to get local issuer certificate",
        ),
        (
            "ssl_selfsigned_openssl3.txt",
            "SSL certificate problem: self-signed certificate in certificate chain",
        ),
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::Certificate {
                detail: detail.to_owned()
            }),
            "{name}"
        );
    }
    assert!(matches!(
        explain(&sample("ssl_schannel.txt")),
        Some(GitProblem::Certificate { .. })
    ));
}

#[test]
fn broken_transfers() {
    for (name, too_large) in [
        ("http413_2.43.txt", true),
        ("rpc_old_2.9.txt", true),
        ("early_2.43.txt", false),
        ("rpc_curl18.txt", false),
    ] {
        assert_eq!(
            explain(&sample(name)),
            Some(GitProblem::Transfer { too_large }),
            "{name}"
        );
    }
}

#[test]
fn histories_shallow_pushes_and_ssh_keys() {
    assert_eq!(
        explain(&sample("unrelated_2.43.txt")),
        Some(GitProblem::UnrelatedHistories)
    );
    assert_eq!(
        explain(&sample("shallow_2.43.txt")),
        Some(GitProblem::ShallowPush)
    );
    assert_eq!(
        explain(&sample("publickey_openssh.txt")),
        Some(GitProblem::SshKeyRefused {
            login: Some("git@github.com".to_owned())
        })
    );
    assert_eq!(
        explain(&sample("publickey_port.txt")),
        Some(GitProblem::SshKeyRefused {
            login: Some("me@git.example.com".to_owned())
        })
    );
}

/// A refused push is the push banner's; an unknown error, nobody's.
#[test]
fn others_are_not_explained() {
    assert_eq!(explain(&sample("rejected_nonff_2.43.txt")), None);
    for err in [
        "",
        "fatal: not a git repository (or any of the parent directories): .git",
        "error: pathspec 'x' did not match any file(s) known to git",
        "\u{0}\u{1}\u{fffd}",
    ] {
        assert_eq!(explain(err), None, "{err:?}");
    }
    // Huge output with nothing known in it.
    assert_eq!(explain(&"remote: counting objects\n".repeat(200_000)), None);
}

#[test]
fn value_patterns_match_only_the_value() {
    assert_eq!(fix::exact_pattern("/a.b"), r"^/a\.b$");
    assert_eq!(fix::exact_pattern(r"C:\x (1)+[y]"), r"^C:\\x \(1\)\+\[y\]$");
}

/// A repository with one commit, in a fresh folder named after `name` (with a space and
/// letters outside ASCII in its path).
fn repo(name: &str) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft errors ü-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
    ] {
        run(Some(&dir), args)?;
    }
    Ok(dir)
}

#[test]
fn a_value_added_is_taken_out_again_and_only_it() -> Result<(), String> {
    let dir = repo("config-add")?;
    let file = dir.join("other.gitconfig");
    let scope = Scope::File(file.clone());
    // Already there, and must stay: a value the pattern would match if it weren't exact.
    let kept = "/srv/a.b (1)";
    run(
        Some(&dir),
        &[
            "config",
            "--file",
            &file.display().to_string(),
            "--add",
            "safe.directory",
            "/srv/aXb (1)",
        ],
    )?;
    let edit = ConfigEdit::add(scope.clone(), "safe.directory", kept);
    fix::apply(&dir, &edit)?;
    assert_eq!(
        fix::get_all(&dir, &scope, "safe.directory")?,
        ["/srv/aXb (1)", kept]
    );
    assert_eq!(fix::file_of(&dir, &scope)?, file.display().to_string());
    fix::revert(&dir, &edit)?;
    assert_eq!(
        fix::get_all(&dir, &scope, "safe.directory")?,
        ["/srv/aXb (1)"]
    );
    // Taking out what is gone already is no error.
    fix::revert(&dir, &edit)?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_value_set_is_put_back_or_unset() -> Result<(), String> {
    let dir = repo("config-set")?;
    let scope = Scope::Repo;
    let edit = ConfigEdit::set(&dir, scope.clone(), "core.longpaths", "true")?;
    assert_eq!(edit.previous, None);
    fix::apply(&dir, &edit)?;
    assert_eq!(
        fix::get(&dir, &scope, "core.longpaths")?.as_deref(),
        Some("true")
    );
    // The repository's own config file, named as git names it.
    let file = fix::file_of(&dir, &scope)?;
    assert!(file.ends_with(".git/config"), "{file}");
    fix::revert(&dir, &edit)?;
    assert_eq!(fix::get(&dir, &scope, "core.longpaths")?, None);

    run(Some(&dir), &["config", "core.longpaths", "false"])?;
    let edit = ConfigEdit::set(&dir, scope.clone(), "core.longpaths", "true")?;
    assert_eq!(edit.previous.as_deref(), Some("false"));
    fix::apply(&dir, &edit)?;
    fix::revert(&dir, &edit)?;
    assert_eq!(
        fix::get(&dir, &scope, "core.longpaths")?.as_deref(),
        Some("false")
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// The whole way: git refuses because of a lock, the matcher finds it, it is looked at and
/// removed, and git works again.
#[test]
fn a_stale_lock_found_and_removed() -> Result<(), String> {
    let dir = repo("lock")?;
    std::fs::write(dir.join("a.txt"), "a\n").map_err(|e| e.to_string())?;
    let lock_file = dir.join(".git/index.lock");
    std::fs::write(&lock_file, "").map_err(|e| e.to_string())?;
    let err = failure(run(Some(&dir), &["add", "a.txt"]), "the lock stops it")?;
    let Some(GitProblem::StaleLock { lock: printed }) = explain(&err) else {
        return Err(format!("not a lock: {err}"));
    };
    let seen = lock::inspect(&dir, &printed)?.ok_or("there")?;
    assert!(seen.age < 60, "{seen:?}");
    lock::remove(&dir, &seen)?;
    assert!(!lock_file.exists());
    assert_eq!(lock::inspect(&dir, &printed)?, None);
    // Gone already: removing it again is what was asked.
    lock::remove(&dir, &seen)?;
    run(Some(&dir), &["add", "a.txt"])?;
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn a_lock_written_again_is_left() -> Result<(), String> {
    let dir = repo("lock-again")?;
    let lock_file = dir.join(".git/index.lock");
    std::fs::write(&lock_file, "").map_err(|e| e.to_string())?;
    let printed = lock_file.display().to_string();
    let mut seen = lock::inspect(&dir, &printed)?.ok_or("there")?;
    // As if a git wrote it since it was looked at.
    seen.modified -= 100;
    let err = failure(lock::remove(&dir, &seen), "left alone")?;
    assert!(err.contains("written again"), "{err}");
    assert!(lock_file.exists());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn only_locks_in_the_git_folder_are_touched() -> Result<(), String> {
    let dir = repo("lock-elsewhere")?;
    let outside = dir.join("work.lock");
    std::fs::write(&outside, "keep").map_err(|e| e.to_string())?;
    let not_a_lock = dir.join(".git/config");
    for path in [&outside, &not_a_lock] {
        let err = failure(lock::inspect(&dir, &path.display().to_string()), "refused")?;
        assert!(
            err.contains("left alone") || err.contains("not a lock"),
            "{err}"
        );
    }
    let sneaky = format!("{}/.git/../work.lock", dir.display());
    assert!(lock::inspect(&dir, &sneaky).is_err());
    assert!(outside.exists() && not_a_lock.exists());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[cfg(unix)]
#[test]
fn a_linked_lock_is_left() -> Result<(), String> {
    let dir = repo("lock-link")?;
    let target = dir.join("precious.txt");
    std::fs::write(&target, "keep").map_err(|e| e.to_string())?;
    let link = dir.join(".git/index.lock");
    std::os::unix::fs::symlink(&target, &link).map_err(|e| e.to_string())?;
    let err = failure(lock::inspect(&dir, &link.display().to_string()), "refused")?;
    assert!(err.contains("symbolic link"), "{err}");
    assert!(target.exists());
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A worktree's index lock is in the worktree's own git folder, a branch's in the shared one.
#[test]
fn locks_of_a_linked_worktree() -> Result<(), String> {
    let dir = repo("lock-worktree")?;
    let wt = dir.with_file_name(format!(
        "{}-wt",
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    run(
        Some(&dir),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "side",
            &wt.display().to_string(),
        ],
    )?;
    let own = run(
        Some(&wt),
        &["rev-parse", "--path-format=absolute", "--git-dir"],
    )?;
    let index_lock = format!("{}/index.lock", own.trim_end());
    let ref_lock = format!("{}/.git/refs/heads/side.lock", dir.display());
    for printed in [&index_lock, &ref_lock] {
        std::fs::write(printed, "").map_err(|e| e.to_string())?;
        let seen = lock::inspect(&wt, printed)?.ok_or("there")?;
        lock::remove(&wt, &seen)?;
        assert!(!Path::new(printed).exists(), "{printed}");
    }
    let _ = std::fs::remove_dir_all(&wt);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn git_processes_of_ps() {
    let ps = "    1     0 /sbin/init\n  200     1 /usr/bin/git commit -m wip\n  201   200 vim .git/COMMIT_EDITMSG\n  300    42 /usr/lib/git-core/git-remote-https origin https://x\n  400   999 git status\n  401   400 git-lfs filter-process\n  500     1 gitcraft\n  501     1 gitk --all\n";
    // 999 is this app: its git and what that git started are its own.
    assert_eq!(
        lock::git_lines(ps, Some(999)),
        [
            "/usr/bin/git commit -m wip",
            "/usr/lib/git-core/git-remote-https origin https://x",
        ]
    );
    assert!(lock::git_lines("", None).is_empty());
    let long = format!("  7 1 git {}", "x".repeat(500));
    let lines = lock::git_lines(&long, None);
    assert_eq!(lines[0].chars().count(), 101);
}

#[test]
fn git_processes_of_tasklist() {
    let out = "\"git.exe\",\"4242\",\"Console\",\"1\",\"9,120 K\"\r\n\"GIT.EXE\",\"7\",\"Console\",\"1\",\"1 K\"\r\n\"bash.exe\",\"8\",\"Console\",\"1\",\"1 K\"\r\nINFO: No tasks are running which match the specified criteria.\r\n";
    assert_eq!(
        lock::tasklist_git(out),
        ["git.exe (process 4242)", "git.exe (process 7)"]
    );
}

/// A supplied local-process snapshot excludes our descendants, not unrelated Git
/// commands whose arguments happen to mention this repository or application.
#[cfg(unix)]
#[test]
fn git_processes_here() {
    let snapshot = "100 1 gitcraft\n\
                    101 100 /bin/sh -c git status\n\
                    102 101 /usr/bin/git status\n\
                    103 102 /usr/libexec/git-core/git-remote-https origin https://owned.invalid\n\
                    200 1 /usr/bin/git ls-remote https://github.com/JKamsker/gitcraft\n\
                    201 1 /usr/bin/git -C /work/gitcraft commit -m retained\n";
    assert_eq!(
        lock::git_lines(snapshot, Some(100)),
        [
            "/usr/bin/git ls-remote https://github.com/JKamsker/gitcraft",
            "/usr/bin/git -C /work/gitcraft commit -m retained",
        ]
    );
    // With no local ownership information, the same parser retains all Git rows.
    assert_eq!(lock::git_lines(snapshot, None).len(), 4);
}

/// Review findings: each one's case, as git really prints it.
#[test]
fn a_bang_in_the_folder_name_is_unquoted() {
    // git quotes `!` as `'\!'` for a shell, besides `'`.
    assert_eq!(
        explain(&sample("dubious_bang_2.43.txt")),
        Some(GitProblem::DubiousOwnership {
            path: "/home/me/work/bang!dir".to_owned()
        })
    );
}

#[test]
fn a_hook_refusing_a_commit_is_not_a_line_ending_problem() {
    // The warning came along; the hook is why it failed.
    assert_eq!(explain(&sample("hook_with_crlf_2.43.txt")), None);
    // Warnings alone, or a refusal, still are.
    assert!(explain(&sample("crlf_warn_2.36.txt")).is_some());
    let refusal = format!(
        "{}\n{}",
        sample("crlf_warn_2.43.txt"),
        sample("crlf_fatal_2.43.txt")
    );
    assert!(matches!(
        explain(&refusal),
        Some(GitProblem::LineEndings { fatal: true, .. })
    ));
}

#[test]
fn a_file_with_in_in_its_name() {
    let warn = "warning: LF will be replaced by CRLF in notes in progress.txt.\n\
                The file will have its original line endings in your working directory";
    assert_eq!(
        explain(warn),
        Some(GitProblem::LineEndings {
            files: vec!["notes in progress.txt".to_owned()],
            to_crlf: true,
            fatal: false,
        })
    );
}

/// No config file yet (git exits 128 listing it): the file named is the one git will make.
#[test]
fn the_file_of_a_config_not_made_yet() -> Result<(), String> {
    let dir = repo("config-missing")?;
    let file = dir.join("not yet.gitconfig");
    let scope = Scope::File(file.clone());
    assert_eq!(fix::file_of(&dir, &scope)?, file.display().to_string());
    assert_eq!(
        fix::get_all(&dir, &scope, "safe.directory")?,
        Vec::<String>::new()
    );
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// With both global files there, git writes ~/.gitconfig, whichever it lists first.
#[test]
fn the_global_file_written_is_named() {
    let both = "file:/h/.config/git/config\0core.x\ny\0file:/h/.gitconfig\0user.name\nme\0file:/h/.gitconfig\0a.b\nc\0";
    assert_eq!(
        fix::origins(both),
        ["/h/.config/git/config", "/h/.gitconfig"]
    );
    let xdg_only = "file:/h/.config/git/config\0core.x\ny\0";
    assert_eq!(fix::origins(xdg_only), ["/h/.config/git/config"]);
    assert!(fix::origins("").is_empty());
}

/// A lock named through another spelling of the folder (a link to it here; on Windows the
/// short `RUNNER~1` form of the temp folder): still the repository's, removed.
#[cfg(unix)]
#[test]
fn a_lock_named_through_a_link_to_the_folder() -> Result<(), String> {
    let dir = repo("lock-alias")?;
    let alias = dir.with_extension("alias");
    let _ = std::fs::remove_file(&alias);
    std::os::unix::fs::symlink(&dir, &alias).map_err(|e| e.to_string())?;
    let lock_file = dir.join(".git/index.lock");
    std::fs::write(&lock_file, "").map_err(|e| e.to_string())?;
    let printed = format!("{}/.git/index.lock", alias.display());
    let seen = lock::inspect(&dir, &printed)?.ok_or("there")?;
    lock::remove(&dir, &seen)?;
    assert!(!lock_file.exists());
    let _ = std::fs::remove_file(&alias);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
