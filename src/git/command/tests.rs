#[test]
fn syntax_errors_only_suggest_upgrade_for_the_unknown_git_option() {
    for error in [
        "cancelled",
        "process timed out and was stopped",
        "fatal: bad object",
        "process exceeded capture limit",
    ] {
        assert_eq!(super::syntax_read_error(error.into()), error);
    }
    let unsupported = "unknown option: --no-lazy-fetch\nusage: git [...]";
    assert!(
        super::syntax_read_error(unsupported.into())
            .contains("requires Git with --no-lazy-fetch support")
    );
    let incidental = "fatal: file named unknown option: --no-lazy-fetch";
    assert_eq!(super::syntax_read_error(incidental.into()), incidental);
}
use std::io::Write;
#[cfg(unix)]
use std::process::Stdio;

use super::*;

#[test]
fn large_binary_reads_and_file_exports_exceed_the_diagnostic_capture_bound() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-large-output-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    run(Some(&dir), &["init", "-q"])?;
    let blob = dir.join("large.bin");
    let mut file = std::fs::File::create(&blob).map_err(|e| e.to_string())?;
    let chunk = vec![0; 1 << 20];
    for _ in 0..65 {
        file.write_all(&chunk).map_err(|e| e.to_string())?;
    }
    drop(file);
    let hash = run(Some(&dir), &["hash-object", "-w", "large.bin"])?;
    let args = ["cat-file", "blob", hash.trim()];
    let captured = run_bytes(Some(&dir), &args)?.len();
    let export = dir.join("export.bin");
    run_to_file(&dir, &args, &export)?;
    let saved = std::fs::metadata(&export).map_err(|e| e.to_string())?.len();
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(captured, 65 << 20);
    assert_eq!(saved, captured as u64);
    Ok(())
}

/// Network operations go to shells of their own on another machine; reads don't.
#[test]
fn network_operations_are_told_apart() {
    for (args, long) in [
        (&["fetch", "--prune"][..], true),
        (&["-c", "credential.helper=x", "push", "origin"][..], true),
        (&["--literal-pathspecs", "pull"][..], true),
        (&["submodule", "update"][..], true),
        (
            &[
                "-c",
                "alias.gitcraft-user-command=!cargo test",
                USER_COMMAND_ALIAS,
            ][..],
            true,
        ),
        (&["status", "--porcelain"][..], false),
        (&["-c", "fetch.x=1", "log"][..], false),
        (&["--no-optional-locks", "status"][..], false),
        (&[][..], false),
    ] {
        assert_eq!(goes_over_network(args), long, "{args:?}");
    }
}

/// A diff or merge tool on another machine stays open as long as the user works in it, in
/// a shell of its own; any other read keeps its lane's limit.
#[test]
fn a_tool_on_another_machine_may_stay_open_for_hours() {
    let target = Target {
        host: remote::Host::Ssh("build".to_owned()),
        path: "/r".to_owned(),
    };
    let tool = Extra {
        tool: true,
        ..Extra::default()
    };
    let req = remote_request(&target, &tool, &["mergetool", "--tool=meld", "-y"]);
    assert!(req.long);
    assert_eq!(req.timeout, Some(TOOL_FOR));
    assert!(
        req.script
            .starts_with("exec git '--no-lazy-fetch' 'mergetool'"),
        "{}",
        req.script
    );
    let read = remote_request(&target, &Extra::default(), &["status"]);
    assert!(!read.long);
    assert_eq!(read.timeout, None);
}

/// `git config --global` reaches the user's own file, whatever comes before it; every
/// other command (and a repository's own `config`) goes by the profile.
#[test]
fn only_global_config_commands_skip_the_profile() {
    assert!(user_global_config(&["config", "--global", "user.name"]));
    assert!(user_global_config(&[
        "-c", "core.x=y", "config", "--global", "--list"
    ]));
    assert!(!user_global_config(&["config", "user.name"]));
    assert!(!user_global_config(&["commit", "-m", "--global"]));
    assert!(!user_global_config(&["-c", "config"]));
    // Whatever options come before the subcommand.
    assert!(user_global_config(&["-C", "/r", "config", "--global", "x"]));
    assert!(user_global_config(&[
        "--literal-pathspecs",
        "config",
        "--global",
        "x"
    ]));
    assert!(user_global_config(&["-ck=v", "config", "--global", "x"]));
    assert!(user_global_config(&["maintenance", "start"]));
    // Running it reads the configuration as every command does.
    assert!(!user_global_config(&["maintenance", "run"]));
    assert!(!user_global_config(&[]));
}

/// What git gets from the helper is what was typed, backslashes (`DOMAIN\user`) included.
#[test]
fn credentials_preserve_typed_fields_without_authorizing_helper_only_use() -> Result<(), String> {
    let (user, pass) = ("CORP\\nancy", "p\\c%s$x 'q\"");
    let credentials = Credentials::for_url("https://example.invalid/", user.into(), pass.into())?;
    let environment = credentials.helper_environment()?;
    assert!(environment.contains(&("GITCRAFT_GIT_USERNAME", user.into())));
    assert!(environment.contains(&("GITCRAFT_GIT_PASSWORD", pass.into())));
    assert!(
        !environment
            .iter()
            .any(|(key, _)| *key == "GITCRAFT_CREDENTIAL_OWNER")
    );
    Ok(())
}

#[cfg(unix)]
fn scratch(name: &str) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-cmd-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// A script that leaves `marker` behind when run, then prints `x` (an askpass answer).
#[cfg(unix)]
fn marking_script(dir: &Path, marker: &Path) -> Result<std::path::PathBuf, String> {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("askpass.sh");
    std::fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\necho x\n", marker.display()),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    Ok(script)
}

/// Whether git, asked for a credential (quietly or not) in an environment that sets
/// `SSH_ASKPASS` (KDE's opens a window) and maybe `GIT_ASKPASS` (VS Code's terminal), ran
/// the askpass program.
#[cfg(unix)]
fn askpass_asked(dir: &Path, quiet: bool, git_askpass: bool) -> Result<bool, String> {
    let marker = dir.join("asked");
    let _ = std::fs::remove_file(&marker);
    let script = marking_script(dir, &marker)?;
    let mut cmd = git_cmd();
    // Not the user's own config, whose `core.askPass` would stand in for the script.
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    // What the app inherited, which `unattended` then has to override.
    cmd.env("SSH_ASKPASS", &script);
    if git_askpass {
        cmd.env("GIT_ASKPASS", &script);
    } else {
        cmd.env_remove("GIT_ASKPASS");
    }
    if quiet {
        unattended(&mut cmd);
    }
    let mut child = cmd
        .args(["-c", "credential.helper=", "credential", "fill"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"protocol=https\nhost=example.invalid\n\n");
    }
    child.wait().map_err(|e| e.to_string())?;
    Ok(marker.exists())
}

/// The background fetch asks nobody to sign in, every few minutes, through a window: not
/// even with a `GIT_ASKPASS` in the environment, which git reads before `core.askPass`.
#[cfg(unix)]
#[test]
fn a_quiet_operation_opens_no_sign_in_window() -> Result<(), String> {
    let dir = scratch("unattended")?;
    let asked = [(false, false), (true, false), (false, true), (true, true)]
        .map(|(quiet, git_askpass)| askpass_asked(&dir, quiet, git_askpass));
    let _ = std::fs::remove_dir_all(&dir);
    // Without it, git does ask (or the test would prove nothing).
    assert_eq!(asked, [Ok(true), Ok(false), Ok(true), Ok(false)]);
    Ok(())
}

/// Nor does ssh, which would ask on a terminal or run `SSH_ASKPASS` itself.
#[cfg(unix)]
#[test]
fn a_quiet_operation_tells_ssh_to_ask_nobody() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = scratch("unattended-ssh")?;
    let seen = dir.join("seen");
    let ssh = dir.join("ssh.sh");
    std::fs::write(
        &ssh,
        format!(
            "#!/bin/sh\nprintf '%s %s' \"$SSH_ASKPASS_REQUIRE\" \"$SSH_ASKPASS\" > '{}'\nexit 1\n",
            seen.display()
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let mut cmd = git_cmd();
    cmd.env("GIT_SSH_COMMAND", &ssh)
        .env_remove("SSH_ASKPASS_REQUIRE");
    unattended(&mut cmd);
    let out = cmd
        .args(["ls-remote", "ssh://example.invalid/repo.git"])
        .output()
        .map_err(|e| e.to_string())?;
    let required = std::fs::read_to_string(&seen);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!out.status.success());
    // Asked through `false`, which answers nothing, never on a terminal.
    assert_eq!(required.map_err(|e| e.to_string())?, "force false");
    Ok(())
}

/// Credentials typed into the tab still answer a quiet operation.
#[cfg(unix)]
#[test]
fn quiet_transient_credentials_use_the_shared_transport_policy() -> Result<(), String> {
    let credentials = Credentials::for_url(
        "https://example.invalid/",
        "nancy".into(),
        "DUMMY_SECRET".into(),
    )?;
    let extra = extra_for(None, &["fetch", "origin"], Some(&credentials), true)?;
    assert!(extra.normalize_repository);
    assert_eq!(
        extra.owned_pre.last().map(String::as_str),
        Some("gitcraft-credential-transport")
    );
    assert!(
        extra
            .env
            .contains(&("GITCRAFT_GIT_PASSWORD", "DUMMY_SECRET".into()))
    );
    assert!(!extra.owned_pre.join(" ").contains("DUMMY_SECRET"));
    assert!(
        !extra
            .env
            .iter()
            .any(|(key, _)| *key == "GITCRAFT_CREDENTIAL_OWNER")
    );
    Ok(())
}

#[test]
fn reads_a_stored_credential() {
    let out = "protocol=https\nhost=codeberg.org\nusername=ana\npassword=s3=cret\n";
    let creds = parse_credentials(out);
    assert_eq!(
        creds.map(|c| (c.username, c.password)),
        Some(("ana".to_owned(), "s3=cret".to_owned()))
    );
    assert!(parse_credentials("protocol=https\nhost=x\n").is_none());
    assert!(parse_credentials("username=ana\npassword=\n").is_none());
}
