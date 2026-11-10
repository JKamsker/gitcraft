#[test]
fn review13_ssh_catalog_check_keeps_web_authority_prefix_and_forge_case() {
    for forge in [Forge::GitHub, Forge::GitLab] {
        let h = Hosting {
            forge,
            host: "forge.example".into(),
            path: "Team/Repo".into(),
            web: "https://forge.example:4000/reviews".into(),
        };
        for (url, expected) in [
            ("https://forge.example:3000/reviews/Team/Repo.git", false),
            ("https://forge.example:4000/Team/Repo.git", false),
            ("https://forge.example:4000/REVIEWS/Team/Repo.git", false),
            ("https://FORGE.example:4000/reviews/Team/Repo.git", true),
            ("git@forge.example:Team/Repo.git", true),
            ("ssh://git@forge.example:2222/Team/Repo.git", true),
            ("git@forge.example:team/repo.git", forge != Forge::GitLab),
        ] {
            let remotes = [crate::git::Remote {
                name: "origin".into(),
                url: url.into(),
            }];
            assert_eq!(
                validate_remote_target(&remotes, &h).is_ok(),
                expected,
                "{forge:?} {url}"
            );
        }
    }
}

use super::*;

/// A self-hosted GitLab on a port of its own is reached on it; an SSH port isn't a web one.
#[test]
fn gitlab_keeps_the_port_of_its_web_pages() {
    let at = |forge, url| tool_location(forge, url);
    assert_eq!(
        at(Forge::GitLab, "https://git.example.org:8443/team/app.git"),
        Some(("git.example.org:8443".to_owned(), "team/app".to_owned()))
    );
    assert_eq!(
        at(Forge::GitLab, "ssh://git@git.example.org:2222/team/app.git"),
        Some(("git.example.org".to_owned(), "team/app".to_owned()))
    );
    assert_eq!(
        at(Forge::GitLab, "git@gitlab.com:team/app.git"),
        Some(("gitlab.com".to_owned(), "team/app".to_owned()))
    );
    // `GH_REPO` takes a plain host.
    assert_eq!(
        at(Forge::GitHub, "https://ghe.example.org:8443/team/app"),
        Some(("ghe.example.org".to_owned(), "team/app".to_owned()))
    );
}

/// A repository's remote names a plain project, never another host by way of its path.
#[test]
fn only_plain_paths_reach_the_tools() {
    assert!(plain_path("team/app"));
    assert!(plain_path("group/sub/my.app"));
    for bad in [
        "https://evil.example/x/y",
        "git@evil.example:x/y",
        "other.corp/g/p:1",
        "-x/y",
        "app",
        "a//b",
    ] {
        assert!(!plain_path(bad), "{bad}");
    }
    let (_, path) = tool_location(
        Forge::GitLab,
        "https://gitlab.com/https://evil.example/x/y.git",
    )
    .unwrap_or_default();
    assert!(!plain_path(&path), "{path}");
}

/// The hosts the arguments send the tool to, which must be the remote's own.
#[test]
fn the_hosts_arguments_name_are_found() {
    assert_eq!(
        named_hosts(&["api", "graphql", "--hostname", "ghe.example.org", "-f", "q"]),
        ["ghe.example.org"]
    );
    assert_eq!(
        named_hosts(&["run", "list", "-R", "ghe.example.org/o/r"]),
        ["ghe.example.org"]
    );
    assert!(named_hosts(&["run", "list", "-R", "o/r"]).is_empty());
    assert!(same_host("git.example.org:8443", "git.example.org"));
    assert!(!same_host("github.evil.example", "github.com"));
}

/// A tool that hangs is stopped at its time; a long answer still comes whole.
#[cfg(unix)]
#[test]
fn a_tool_given_a_time_is_stopped_at_it() {
    let run_within = |tool: &str, _: &str, here: &Path, args: &[&str], within| {
        let mut cmd = tool_command(tool);
        cmd.current_dir(here).args(args);
        within_command(tool, cmd, |e| e, within)
    };
    let here = std::env::temp_dir();
    let started = Instant::now();
    let hung = run_within("sleep", "x", &here, &["20"], Duration::from_millis(300));
    assert!(hung.is_err_and(|e| e.contains("didn't finish within")));
    assert!(started.elapsed() < Duration::from_secs(10));
    let script = "head -c 300000 /dev/zero | tr '\\0' x";
    let long = run_within("sh", "x", &here, &["-c", script], Duration::from_secs(20));
    assert_eq!(long.map(|out| out.len()), Ok(300_000));
    let failed = run_within(
        "sh",
        "x",
        &here,
        &["-c", "echo no >&2; exit 3"],
        Duration::from_secs(20),
    );
    assert_eq!(failed, Err("no".to_owned()));
}

/// `glab api` would refuse `--repo`; the commands about a project take it.
#[test]
fn only_glab_commands_about_a_project_take_repo() {
    assert!(!gitlab_takes_repo(&[
        "api",
        "--hostname",
        "gitlab.com",
        "projects/x"
    ]));
    assert!(gitlab_takes_repo(&["mr", "list", "-F", "json"]));
    assert!(!gitlab_takes_repo(&[]));
}

/// The user's environment can't colour the JSON read from the tools, nor point them at
/// another repository or host.
#[test]
fn tools_answer_in_plain_json_about_this_repository() {
    for tool in ["gh", "glab"] {
        let cmd = tool_command(tool);
        let envs: HashMap<String, Option<String>> = cmd
            .get_envs()
            .map(|(k, v)| {
                let text = |s: &std::ffi::OsStr| s.to_string_lossy().into_owned();
                (text(k), v.map(text))
            })
            .collect();
        for removed in [
            "CLICOLOR_FORCE",
            "GH_FORCE_TTY",
            "GH_REPO",
            "GH_HOST",
            "GITLAB_HOST",
            "GITLAB_REPO",
            "GITLAB_URI",
            "GL_HOST",
            "GITLAB_HEAD_REPO",
        ] {
            assert_eq!(envs.get(removed), Some(&None), "{removed}");
        }
        assert_eq!(envs.get("NO_COLOR"), Some(&Some("1".to_owned())));
    }
}

#[test]
fn bounded_requests_keep_input_deadline_and_ambient_cancellation() -> Result<(), String> {
    let cancel = crate::tasks::Cancellation::default();
    crate::tasks::with_cancellation(cancel.clone(), || {
        let options = bounded_options(Duration::from_secs(7), Some("a body\n"));
        cancel.cancel();
        let mut command = Command::new("/recorded/gh");
        command
            .args(["api", "endpoint"])
            .current_dir("/recorded/repo");
        command.env("GH_HOST", "recorded.example");
        let result = dispatch(
            command,
            options,
            |error| format!("start: {error}"),
            |command, options| {
                assert_eq!(command.get_program(), "/recorded/gh");
                assert_eq!(command.get_args().collect::<Vec<_>>(), ["api", "endpoint"]);
                assert_eq!(command.get_current_dir(), Some(Path::new("/recorded/repo")));
                assert_eq!(
                    command.get_envs().collect::<Vec<_>>(),
                    [(
                        std::ffi::OsStr::new("GH_HOST"),
                        Some(std::ffi::OsStr::new("recorded.example"))
                    )]
                );
                assert_eq!(options.input.as_deref(), Some(b"a body\n".as_slice()));
                assert_eq!(options.timeout, Some(Duration::from_secs(7)));
                assert_eq!(options.output_limit, OUTPUT_LIMIT);
                assert_eq!(options.stderr_limit, Some(ERROR_LIMIT));
                assert!(!options.hold_input);
                assert!(options.cancel.is_cancelled());
                Err(crate::processes::Failure::CancelledBeforeStart)
            },
        );
        assert!(matches!(result, Err(ref error) if error == "cancelled before execution"));
    });
    assert_eq!(TOOL_TIMEOUT, Duration::from_secs(120));
    Ok(())
}

#[test]
fn stopped_or_uncertain_results_never_become_success_or_authentication() {
    use crate::processes::{StopKind, recording_output};
    for kind in [
        StopKind::Cancelled,
        StopKind::TimedOut,
        StopKind::Failed,
        StopKind::Uncertain,
    ] {
        let result = finish_run(
            "gh",
            Duration::from_secs(3),
            Ok(recording_output(Some(kind))),
        );
        assert!(result.is_err());
        if kind == StopKind::Uncertain {
            assert_eq!(result, Err("cleanup proof missing".into()));
        }
        if kind == StopKind::TimedOut {
            assert!(result.is_err_and(|error| error.contains("didn't finish within 3 seconds")));
        }
        assert!(finish_auth(Ok(recording_output(Some(kind)))).is_err());
        assert!(finish_report(Ok(recording_output(Some(kind))), true).is_none());
    }
    assert_eq!(finish_auth(Ok(recording_output(None))), Ok(true));
}

#[test]
fn tool_exit_diagnostics_survive_and_discarded_reports_reveal_no_output() {
    use crate::processes::recording_output;
    let mut output = recording_output(None);
    output.stdout = b"tool response".to_vec();
    assert_eq!(
        finish_run("gh", TOOL_TIMEOUT, Ok(output)),
        Ok("tool response".into())
    );
    #[cfg(unix)]
    let failed_status = {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(3 << 8)
    };
    #[cfg(windows)]
    let failed_status = {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(3)
    };
    let mut failed = recording_output(None);
    failed.status = failed_status;
    failed.stderr = b"actual exit diagnostic".to_vec();
    assert_eq!(
        finish_run("gh", TOOL_TIMEOUT, Ok(failed)),
        Err("actual exit diagnostic".into())
    );
    for success in [false, true] {
        let mut output = recording_output(None);
        if !success {
            output.status = failed_status;
        }
        output.stdout = b"secret token".to_vec();
        output.stderr = b"secret account".to_vec();
        assert_eq!(
            finish_report(Ok(output), false),
            Some((success, String::new()))
        );
    }
}

#[test]
fn auth_capture_is_bounded_and_redacts_stderr_before_retention() -> Result<(), String> {
    // keep=true does not open a sink or launch a program. The production keep=false path
    // uses stdout_file, since Command.stdout would be overridden by Service.
    let mut options = auth_options(REPORT_TIMEOUT, true)?;
    assert_eq!(options.timeout, Some(REPORT_TIMEOUT));
    assert_eq!(options.output_limit, ERROR_LIMIT);
    assert_eq!(options.stderr_limit, Some(ERROR_LIMIT));
    assert!(options.stdout_file.is_none());
    assert!(options.input.is_none());
    let filter = options.stderr_filter.as_mut().ok_or("missing redaction")?;
    assert_eq!(filter(b"secret token"), vec![0; 12]);
    assert_eq!(filter(b""), Vec::<u8>::new());
    assert_eq!(filter(&vec![b's'; ERROR_LIMIT + 1]).len(), ERROR_LIMIT + 1);
    Ok(())
}

#[test]
fn secret_discard_requires_null_sink_and_refuses_failed_open() {
    // Recording opener: never opens a file or starts the service. A sink failure must
    // refuse construction rather than falling back to captured token stdout.
    let opened = std::cell::Cell::new(false);
    let result = auth_options_with(REPORT_TIMEOUT, false, |path| {
        opened.set(true);
        assert_eq!(path, NULL_SINK);
        #[cfg(unix)]
        assert_eq!(path, "/dev/null");
        #[cfg(windows)]
        assert_eq!(path, "NUL");
        Err(std::io::Error::other("recorded sink refusal"))
    });
    assert!(opened.get());
    assert!(result.is_err_and(|error| error.contains("recorded sink refusal")));
    assert!(
        auth_options_with(REPORT_TIMEOUT, true, |_| {
            opened.set(false);
            Err(std::io::Error::other("unexpected open"))
        })
        .is_ok()
    );
    assert!(opened.get());
}

#[test]
fn oversized_input_refuses_before_command_preparation() {
    // An invalid path/tool would reach discovery if preparation ran. The input guard is
    // first, so this is a pure rejection test with no filesystem or process access.
    let input = "x".repeat(INPUT_LIMIT + 1);
    assert_eq!(
        run_with_input(
            "/recorded/gh",
            "review",
            Path::new("/recorded/repo"),
            &Hosting {
                forge: Forge::GitHub,
                host: "github.com".into(),
                path: "o/r".into(),
                web: "https://github.com".into()
            },
            &[],
            Some(&input)
        ),
        Err(format!(
            "/recorded/gh input exceeds {} bytes",
            crate::i18n::number(INPUT_LIMIT)
        )),
    );
}
