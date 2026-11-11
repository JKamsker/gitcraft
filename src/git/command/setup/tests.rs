use super::*;
#[cfg(unix)]
mod selected_context;

#[test]
fn setup_network_context_refuses_pending_profiles_and_unselected_ssh_identities()
-> Result<(), String> {
    let workspace = Workspace::new()?;
    let parent = &workspace.0;
    let target = parent.join("target");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let pending = connections::Context {
        error: Some("Pending fixture account".into()),
        ..Default::default()
    };
    assert!(
        pending
            .run(|| clone_command(
                parent,
                "https://expected.fixture.invalid/team/repo.git",
                &target,
                "origin",
                false,
                &exe
            ))
            .is_err_and(|e| e.contains("Pending fixture"))
    );
    let ready = connections::Context::default();
    assert!(
        ready
            .run(|| clone_command(
                parent,
                "ssh://git@expected.fixture.invalid:2222/team/repo.git",
                &target,
                "origin",
                false,
                &exe
            ))
            .is_err_and(|e| e.contains("explicit SSH key"))
    );
    let missing = connections::Context {
        choices: Choices {
            ssh_key: parent
                .join("missing-selected-key")
                .to_string_lossy()
                .into_owned(),
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(
        missing
            .run(|| clone_command(
                parent,
                "ssh://git@expected.fixture.invalid/team/repo.git",
                &target,
                "origin",
                false,
                &exe
            ))
            .is_err_and(|e| e.contains("unavailable"))
    );
    for source in [
        "https://user:SECRET@expected.fixture.invalid/team/repo.git",
        "ext::untrusted",
        "file:///outside",
    ] {
        assert!(
            ready
                .run(|| clone_command(parent, source, &target, "origin", false, &exe))
                .is_err()
        );
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn network_setup_clone_pins_https_and_explicit_profile_authentication_under_hostile_configuration()
-> Result<(), String> {
    const CHILD: &str = "GITCRAFT_SETUP_NETWORK_FIXTURE";
    if let Some(root) = std::env::var_os(CHILD) {
        return selected_context::run(Path::new(&root));
    }
    let workspace = Workspace::new()?;
    let root = &workspace.0;
    let parent = root.join("destination parent");
    let exec = root.join("recording-git-tools");
    let home = root.join("home with 'quote'");
    for p in [&parent, &exec, &home] {
        std::fs::create_dir(p).map_err(|e| e.to_string())?;
    }
    crate::git::practice_run(&parent, &["init", "-q"])?;
    let shell = |p: &Path| connections::shell_quote(&p.to_string_lossy());
    let write_script = |path: &Path, text: &str| -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        std::fs::write(path, text).map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())
    };
    let selected = root.join("selected gitcraft 'helper'");
    write_script(
        &selected,
        &format!(
            "#!/bin/sh\ncase \"$1\" in\n--gitcraft-profile-credential) printf '%s\\n' \"$@\" > {}; printf '%s\\n' \"$HOME\" > {}; printf 'username=expected-work\\npassword=FIXTURE_SELECTED_TOKEN\\n\\n';;\n--gitcraft-profile-ssh) printf '%s\\n' \"$@\" > {}; printf '%s\\n' \"$SSH_AUTH_SOCK\" > {}; exit 7;;\n*) exit 8;;\nesac\n",
            shell(&root.join("selected-args")),
            shell(&root.join("selected-home")),
            shell(&root.join("selected-ssh-args")),
            shell(&root.join("selected-ssh-agent"))
        ),
    )?;
    let ambient = root.join("ambient-helper");
    write_script(
        &ambient,
        &format!(
            "#!/bin/sh\nprintf ran > {}\nprintf 'username=AMBIENT_PERSONAL\\npassword=AMBIENT_FIXTURE_TOKEN\\n\\n'\n",
            shell(&root.join("ambient-executed"))
        ),
    )?;
    let ssh = root.join("wrong-ssh");
    write_script(
        &ssh,
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}; exit 7\n",
            shell(&root.join("wrong-ssh-host"))
        ),
    )?;
    write_script(
        &exec.join("git-remote-https"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nprintf '%s\\n' \"$HOME\" > {}\n[ -e \"$HOME/.netrc\" ] || printf yes > {}\nprintf 'protocol=https\\nhost=expected.fixture.invalid\\n\\n' | git credential fill > {} 2>/dev/null\nexit 7\n",
            shell(&root.join("https-args")),
            shell(&root.join("git-home")),
            shell(&root.join("netrc-was-isolated")),
            shell(&root.join("credentials"))
        ),
    )?;
    std::fs::write(
        home.join(".netrc"),
        "machine expected.fixture.invalid login AMBIENT_PERSONAL password AMBIENT_FIXTURE_TOKEN\n",
    )
    .map_err(|e| e.to_string())?;
    let global = root.join("hostile-global.gitconfig");
    let profile = root.join("hostile-profile.gitconfig");
    let text = format!(
        "[url \"https://wrong.fixture.invalid/\"]\n insteadOf = https://expected.fixture.invalid/\n[credential]\n helper = !{}\n[core]\n sshCommand = {}\n",
        ambient.display(),
        ssh.display()
    );
    std::fs::write(&global, &text).map_err(|e| e.to_string())?;
    std::fs::write(&profile,format!("{text}[url \"ssh://git@wrong.fixture.invalid/\"]\n insteadOf = https://expected.fixture.invalid/team/\n")).map_err(|e|e.to_string())?;
    crate::git::practice_run(
        &parent,
        &[
            "config",
            "--local",
            "url.ssh://git@wrong.fixture.invalid/.insteadOf",
            "https://expected.fixture.invalid/",
        ],
    )?;
    let out=Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact","git::command::setup::tests::network_setup_clone_pins_https_and_explicit_profile_authentication_under_hostile_configuration","--nocapture"]).env(CHILD,root).env("HOME",&home).env("USERPROFILE",&home).env("XDG_CONFIG_HOME",root.join("original-xdg")).env("GIT_EXEC_PATH",&exec).env("SSH_AUTH_SOCK",root.join("recording-selected-agent.sock")).env("GIT_CONFIG_GLOBAL",&global).env("GIT_DIR",parent.join(".git")).env("GIT_WORK_TREE",&parent).env("GIT_SSH_COMMAND",&ssh).env("GIT_CONFIG_COUNT","1").env("GIT_CONFIG_KEY_0","url.ssh://git@wrong.fixture.invalid/.insteadOf").env("GIT_CONFIG_VALUE_0","https://expected.fixture.invalid/").output().map_err(|e|e.to_string())?;
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[test]
fn secondary_fetch_is_pinned_to_previewed_url_and_an_owned_git_directory() -> Result<(), String> {
    let workspace = Workspace::new()?;
    let exe = workspace.0.join("fixed gitcraft helper");
    let source = "https://secondary.fixture.invalid/team/repository.git";
    let context = connections::Context::default();
    let prepared = context.run(|| fetch_command(&workspace.0, source, "upstream", &exe))?;
    let args = prepared
        .command
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    let operation = args
        .iter()
        .position(|a| a == "fetch")
        .ok_or("Missing fetch operation")?;
    assert_eq!(
        &args[operation..],
        [
            "fetch",
            "--no-tags",
            "--no-write-fetch-head",
            "--",
            source,
            "+refs/heads/*:refs/remotes/upstream/*",
            "+refs/tags/*:refs/gitcraft-setup/upstream/tags/*"
        ]
    );
    assert_eq!(args[operation - 2], "--git-dir");
    assert_eq!(
        args[operation - 1],
        workspace.0.join(".git").to_string_lossy()
    );
    let folder = args
        .iter()
        .position(|a| a == "-C")
        .ok_or("Missing isolated working folder")?;
    assert_eq!(args[folder + 1], prepared.workspace.0.to_string_lossy());
    for (source, remote) in [
        ("ext::UNTRUSTED", "upstream"),
        (source, "../../outside"),
        (
            "https://token:SECRET@secondary.fixture.invalid/repo",
            "upstream",
        ),
    ] {
        assert!(
            context
                .run(|| fetch_command(&workspace.0, source, remote, &exe))
                .is_err()
        );
    }
    Ok(())
}

#[cfg(windows)]
#[test]
fn local_setup_clone_uses_git_paths_without_enabling_network_auth() -> Result<(), String> {
    let workspace = Workspace::new()?;
    let destination = workspace.0.join("target with spaces");
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    // Recording only: none of these source paths are opened or executed.
    for (source, expected) in [
        (
            r"\\?\C:\source with spaces\repo",
            "C:/source with spaces/repo",
        ),
        (r"C:\source with spaces\repo", "C:/source with spaces/repo"),
        (r"\\?\UNC\server\share\repo", "//server/share/repo"),
        (r"\\server\share\repo", "//server/share/repo"),
    ] {
        let pending = connections::Context {
            error: Some("Local clone must not consult profile authentication".into()),
            ..Default::default()
        };
        let prepared = pending
            .run(|| clone_command(&workspace.0, source, &destination, "origin", true, &exe))?;
        let args: Vec<_> = prepared.command.get_args().collect();
        assert_eq!(args[args.len() - 2], std::ffi::OsStr::new(expected));
        let recorded_destination = Path::new(args.last().ok_or("Missing clone destination")?);
        assert!(recorded_destination.is_absolute());
        assert_eq!(
            recorded_destination
                .parent()
                .ok_or("Missing parent")?
                .canonicalize()
                .map_err(|e| e.to_string())?,
            workspace.0
        );
        assert_eq!(recorded_destination.file_name(), destination.file_name());
        assert!(!recorded_destination.to_string_lossy().contains(['?', '\\']));
        let env: std::collections::BTreeMap<_, _> = prepared.command.get_envs().collect();
        assert_eq!(
            env.get(std::ffi::OsStr::new("GIT_ALLOW_PROTOCOL")),
            Some(&Some(std::ffi::OsStr::new("file")))
        );
        assert!(!env.contains_key(std::ffi::OsStr::new("GITCRAFT_GIT_PASSWORD")));
    }
    Ok(())
}
