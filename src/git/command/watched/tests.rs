use std::path::PathBuf;

use super::*;
use crate::git::run;

fn scratch(name: &str) -> Result<PathBuf, String> {
    let dir =
        std::env::temp_dir().join(format!("gitcraft-watched {name} ü {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A repository with a few commits and files (names with spaces and accents among them).
fn source(dir: &Path) -> Result<(), String> {
    run(Some(dir), &["init", "-q"])?;
    run(Some(dir), &["config", "user.name", "t"])?;
    run(Some(dir), &["config", "user.email", "t@t"])?;
    for i in 0..20 {
        let name = format!("fichier {i} é.txt");
        std::fs::write(dir.join(name), format!("{i}\n").repeat(200)).map_err(|e| e.to_string())?;
        run(Some(dir), &["add", "-A"])?;
        run(Some(dir), &["commit", "-q", "-m", &format!("commit {i}")])?;
    }
    Ok(())
}

#[test]
fn progress_goes_after_the_subcommand_unless_told_to_be_quiet() {
    let cases: [(&[&str], &[&str]); 6] = [
        (&["fetch", "--all"], &["fetch", "--progress", "--all"]),
        (
            &["-c", "push.default=simple", "push", "-u", "o"],
            &["-c", "push.default=simple", "push", "--progress", "-u", "o"],
        ),
        (&["pull", "--quiet"], &["pull", "--quiet"]),
        (&["fetch", "-q"], &["fetch", "-q"]),
        (&["ls-remote", "o"], &["ls-remote", "o"]),
        (&[], &[]),
    ];
    for (args, want) in cases {
        assert_eq!(with_progress(args), want, "{args:?}");
    }
}

#[test]
fn a_clone_reports_its_progress_and_ends() -> Result<(), String> {
    let dir = scratch("clone")?;
    let src = dir.join("source repo");
    std::fs::create_dir_all(&src).map_err(|e| e.to_string())?;
    source(&src)?;
    let dest = dir.join("copie é");
    let control = OpControl::new(|| {});
    let (src_s, dest_s) = (src.to_string_lossy(), dest.to_string_lossy());
    // `--no-local`: through git's transport, as from a server, not a copy of the files.
    let args = ["clone", "--no-local", "--", &src_s, &dest_s];
    let cloned = run_watched(None, &args, None, false, &control);
    let progress = control.progress();
    let files = std::fs::read_dir(&dest).map(|d| d.count()).unwrap_or(0);
    let _ = std::fs::remove_dir_all(&dir);
    cloned?;
    assert!(progress.is_some(), "no progress reported");
    // 20 files and `.git`.
    assert_eq!(files, 21);
    Ok(())
}

#[test]
fn a_failure_says_why_without_the_progress() -> Result<(), String> {
    let dir = scratch("fails")?;
    run(Some(&dir), &["init", "-q"])?;
    let control = OpControl::new(|| {});
    let missing = dir.join("nowhere");
    let missing = missing.to_string_lossy();
    let err = run_watched(Some(&dir), &["fetch", &missing], None, false, &control);
    let _ = std::fs::remove_dir_all(&dir);
    let err = err.err().unwrap_or_default();
    assert!(
        err.contains("does not appear to be a git repository"),
        "{err}"
    );
    assert!(!err.contains('%'), "{err}");
    Ok(())
}

/// A cancel stops git and what it started (here the "ssh" it runs, which hangs), at once.
#[cfg(unix)]
#[test]
fn a_cancel_stops_git_and_what_it_started() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("cancel")?;
    run(Some(&dir), &["init", "-q"])?;
    let pid_file = dir.join("ssh.pid");
    let ssh = dir.join("slow ssh.sh");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\necho $$ > '{}'\nexec sleep 60\n",
            pid_file.display()
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let control = OpControl::new(|| {});
    let canceller = Arc::clone(&control);
    let pid_seen = pid_file.clone();
    let canceller_thread = std::thread::spawn(move || {
        for _ in 0..200 {
            if std::fs::read_to_string(&pid_seen).ok().is_some_and(|pid| {
                pid.ends_with('\n') && pid.trim().parse::<u32>().is_ok_and(|pid| pid > 0)
            }) {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        canceller.cancel();
    });
    let ssh_config = format!("core.sshCommand='{}'", ssh.display());
    let started = Instant::now();
    let result = run_watched(
        Some(&dir),
        &["-c", &ssh_config, "fetch", "ssh://example.invalid/r.git"],
        None,
        false,
        &control,
    );
    let took = started.elapsed();
    canceller_thread.join().map_err(|_| "canceller panicked")?;
    let pid = std::fs::read_to_string(&pid_file).unwrap_or_default();
    let pid = pid.trim();
    assert_eq!(result, Err(CANCELLED.to_owned()));
    assert!(!pid.is_empty(), "the fake ssh never ran");
    assert!(took < Duration::from_secs(15), "took {took:?}");
    let running = || -> Result<bool, String> {
        // One /proc snapshot avoids a kill-0/read-stat race: the process can be
        // reaped between those calls. A container's PID 1 may also leave zombies,
        // which have stopped executing and must not count as surviving helpers.
        #[cfg(target_os = "linux")]
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => {
                return Ok(!stat
                    .rsplit_once(") ")
                    .is_some_and(|(_, rest)| rest.starts_with("Z ") || rest.starts_with("X ")));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(_) => {}
        }
        std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .map_err(|error| error.to_string())
    };
    // SIGKILL delivery and waiting for Git do not synchronize the scheduler's exit
    // transition of its nonchild helper. Still require that helper to stop promptly:
    // an escaped sleep would continue for 60 seconds and fail this bounded check.
    let deadline = Instant::now() + Duration::from_secs(2);
    let alive = loop {
        let alive = running()?;
        if !alive || Instant::now() >= deadline {
            break alive;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let state = if alive {
        std::process::Command::new("ps")
            .args(["-o", "pid=,ppid=,pgid=,stat=,command=", "-p", pid])
            .output()
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!alive, "what git started still runs: {state}");
    Ok(())
}

#[test]
fn nothing_runs_once_cancelled() -> Result<(), String> {
    let dir = scratch("cancelled-before")?;
    let control = OpControl::new(|| {});
    control.cancel();
    let result = run_watched(Some(&dir), &["fetch"], None, false, &control);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(result, Err(CANCELLED.to_owned()));
    Ok(())
}
/// Exercise the same owned streaming dispatcher used by watched Git.
#[cfg(unix)]
fn run_there_here(
    dir: &Path,
    args: &[&str],
    stop_after: Option<Duration>,
) -> Result<(Option<i32>, bool), String> {
    let target = Target {
        host: Host::Ssh(remote::LOOPBACK.into()),
        path: dir.to_string_lossy().into_owned(),
    };
    let req = super::super::remote_request(&target, &Extra::default(), args);
    let streaming = remote::streaming_command(&target.host, &req)?;
    let lease = streaming.lease.clone();
    let token = crate::tasks::Cancellation::default();
    let stop = token.clone();
    let canceller = stop_after.map(|after| {
        std::thread::spawn(move || {
            std::thread::sleep(after);
            stop.cancel();
        })
    });
    let mut options = crate::processes::Options {
        cancel: token,
        timeout: Some(Duration::from_secs(15)),
        ..Default::default()
    };
    let command = streaming.configure(&mut options);
    let result = crate::processes::run(command, options)?;
    if let Some(canceller) = canceller {
        canceller.join().map_err(|_| "canceller panicked")?;
    }
    if let Some(reason) = &result.stopped {
        assert!(!reason.contains("unconfirmed"), "{reason}");
    }
    Ok((result.status.code(), lease.endpoint().is_some()))
}

#[cfg(unix)]
#[test]
fn the_script_run_elsewhere_has_an_owner_and_ends_as_git_did() -> Result<(), String> {
    let dir = scratch("there")?;
    run(Some(&dir), &["init", "-q"])?;
    let ok = run_there_here(&dir, &["status", "--short"], None)?;
    let failed = run_there_here(&dir, &["fetch", "/no/such/repo here"], None)?;
    let stopped = run_there_here(
        &dir,
        &["-c", "alias.wait=!sleep 60", "wait"],
        Some(Duration::from_millis(300)),
    )?;
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
    assert_eq!(ok, (Some(0), true));
    assert_eq!(failed, (Some(128), true));
    assert!(stopped.1);
    assert_ne!(stopped.0, Some(0));
    Ok(())
}
#[cfg(unix)]
#[test]
fn remote_stream_progress_cancel_and_partial_output_use_the_event_loop() -> Result<(), String> {
    let dir = scratch("async-remote")?;
    let repo = std::path::PathBuf::from(format!("ssh://{}{}", remote::LOOPBACK, dir.display()));
    let control = OpControl::new(|| {});
    let cancel = Arc::clone(&control);
    let canceller = std::thread::spawn(move || {
        let start = Instant::now();
        while cancel.progress().is_none() && start.elapsed() < Duration::from_secs(10) {
            std::thread::sleep(Duration::from_millis(10));
        }
        let progress = cancel.progress();
        cancel.cancel();
        progress
    });
    let (result, recorded) = run_watched_recorded(
        Some(&repo),
        &[
            "-c",
            "alias.stream=!printf partial-output; printf 'Receiving objects: 42%% (42/100)\\r' >&2; sleep 60",
            "stream",
        ],
        None,
        false,
        &control,
    );
    let progress = canceller.join().map_err(|_| "canceller panicked")?;
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(result, Err(CANCELLED.into()));
    assert_eq!(progress.and_then(|p| p.percent), Some(42));
    assert!(
        recorded.output.contains("partial-output"),
        "{}",
        recorded.output
    );
    Ok(())
}
#[cfg(unix)]
#[test]
fn successful_and_untraced_push_hooks_keep_both_logs_and_results_private() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let repo = crate::git::stack::test_repo::TestRepo::new("watched-hook-privacy")?;
    let bare = repo.path().join("remote.git");
    repo.git(&["init", "--bare", bare.to_str().ok_or("path")?])?;
    repo.git(&["remote", "add", "origin", bare.to_str().ok_or("path")?])?;
    let hook = repo.path().join(".git/hooks/pre-push");
    let secret = "PRIVATE-HOOK-STDOUT-STDERR";
    for (quiet, code) in [(false, 0), (true, 0), (true, 3), (false, 3)] {
        std::fs::write(
            &hook,
            format!("#!/bin/sh\necho {secret}\necho {secret} >&2\nexit {code}\n"),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
        let control = OpControl::new(|| {});
        let (result, recorded) = run_watched_recorded(
            Some(repo.path()),
            &["push", "--porcelain", "origin", "HEAD:refs/heads/main"],
            None,
            quiet,
            &control,
        );
        assert_eq!(result.is_ok(), code == 0, "{result:?}");
        assert!(!format!("{result:?}").contains(secret));
        assert!(!recorded.output.contains(secret));
        assert!(
            crate::git::doctor::hook_log::runs(repo.path())
                .iter()
                .all(|entry| !entry.output.contains(secret))
        );
        if code == 0 {
            assert_eq!(
                crate::git::resolve(&bare, "refs/heads/main")?,
                repo.tip("HEAD")?
            );
        }
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn a_remote_push_to_a_missing_folder_preserves_its_recovery_diagnostic() -> Result<(), String> {
    let dir = scratch("remote missing push")?;
    let missing = dir.join("absent");
    let path = crate::remote::encode(
        &crate::remote::Host::Ssh(crate::remote::LOOPBACK.into()),
        missing.to_str().ok_or("path")?,
    );
    let control = OpControl::new(|| {});
    let (result, recorded) = run_watched_extra_recorded(
        Some(&path),
        &["push", "--porcelain", "origin", "HEAD:refs/heads/main"],
        None,
        false,
        &control,
        Extra::default(),
    );
    let target = crate::remote::parse(&path).ok_or("target")?;
    let expected = super::super::missing_folder(&target);
    assert_eq!(result, Err(expected.clone()));
    assert!(recorded.output.contains(&expected));
    Ok(())
}
