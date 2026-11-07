use super::*;

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
        let root = PathBuf::from(root);
        let parent = root.join("destination parent");
        let exe = root.join("selected gitcraft 'helper'");
        let expected = "https://expected.fixture.invalid/team/repository.git";
        let profile = connections::Context {
            git_config: Some(root.join("hostile-profile.gitconfig")),
            choices: Choices {
                github: connections::Account {
                    host: "expected.fixture.invalid".into(),
                    user: "expected-work".into(),
                    config_dir: root.join("work-auth-folder").to_string_lossy().into_owned(),
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let before = std::fs::read(parent.join(".git/config")).map_err(|e| e.to_string())?;
        let CloneCommand { command, workspace } = profile.run(|| {
            clone_command(
                &parent,
                expected,
                &parent.join("selected"),
                "origin",
                false,
                &exe,
            )
        })?;
        let isolated_home = workspace.0.clone();
        let out = setup_process(command, &|| true, Duration::from_secs(10))?;
        assert!(
            !out.status.success(),
            "Recording transport deliberately refuses a real connection"
        );
        let transport =
            std::fs::read_to_string(root.join("https-args")).map_err(|e| e.to_string())?;
        assert_eq!(transport.lines().collect::<Vec<_>>(), ["origin", expected]);
        assert_eq!(
            std::fs::read_to_string(root.join("git-home"))
                .map_err(|e| e.to_string())?
                .trim(),
            isolated_home.to_string_lossy()
        );
        assert_eq!(
            std::fs::read_to_string(root.join("selected-home"))
                .map_err(|e| e.to_string())?
                .trim(),
            root.join("home with 'quote'").to_string_lossy()
        );
        let credentials =
            std::fs::read_to_string(root.join("credentials")).map_err(|e| e.to_string())?;
        assert!(
            credentials.contains("username=expected-work")
                && credentials.contains("password=FIXTURE_SELECTED_TOKEN")
        );
        assert!(!credentials.contains("AMBIENT"));
        let args =
            std::fs::read_to_string(root.join("selected-args")).map_err(|e| e.to_string())?;
        let args = args.lines().collect::<Vec<_>>();
        assert_eq!(args.len(), 4);
        assert_eq!(args[0], "--gitcraft-profile-credential");
        assert_eq!(args[1], "gh");
        assert_eq!(args[3], "get");
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(args[2])
            .map_err(|e| e.to_string())?;
        let account: connections::Account =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        assert_eq!(account, profile.choices.github);
        assert!(!root.join("ambient-executed").exists());
        assert!(!root.join("wrong-ssh-host").exists());
        assert!(root.join("netrc-was-isolated").exists());
        drop(workspace);
        assert!(!isolated_home.exists());
        std::fs::remove_file(root.join("selected-args")).map_err(|e| e.to_string())?;
        let CloneCommand { command, workspace } = connections::Context::default().run(|| {
            clone_command(
                &parent,
                expected,
                &parent.join("anonymous"),
                "origin",
                false,
                &exe,
            )
        })?;
        let out = setup_process(command, &|| true, Duration::from_secs(10))?;
        assert!(!out.status.success());
        drop(workspace);
        assert!(
            !root.join("selected-args").exists(),
            "Anonymous public clone must not invoke a personal/previous selected helper"
        );
        assert!(!root.join("ambient-executed").exists());
        assert!(!root.join("wrong-ssh-host").exists());
        assert!(
            std::fs::read(root.join("credentials"))
                .map_err(|e| e.to_string())?
                .is_empty()
        );
        // Preserve normal Profile helper precedence if two explicitly selected
        // tools share a host, instead of introducing a new unsupported choice.
        let mut both = profile.clone();
        both.choices.gitlab = connections::Account {
            host: "expected.fixture.invalid".into(),
            user: "expected-work".into(),
            config_dir: root
                .join("explicit-glab-folder")
                .to_string_lossy()
                .into_owned(),
        };
        let CloneCommand { command, workspace } = both.run(|| {
            clone_command(
                &parent,
                expected,
                &parent.join("last-explicit-helper"),
                "origin",
                false,
                &exe,
            )
        })?;
        let out = setup_process(command, &|| true, Duration::from_secs(10))?;
        assert!(!out.status.success());
        drop(workspace);
        let args =
            std::fs::read_to_string(root.join("selected-args")).map_err(|e| e.to_string())?;
        let args = args.lines().collect::<Vec<_>>();
        assert_eq!(args[1], "glab");
        let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(args[2])
            .map_err(|e| e.to_string())?;
        let account: connections::Account =
            serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        assert_eq!(account, both.choices.gitlab);
        assert!(!root.join("ambient-executed").exists());
        // SSH uses only the explicit selected key and fixed OpenSSH dialect; its helper
        // boundary records argv instead of starting any SSH executable/connection.
        let key = root.join("explicit-selected-key");
        std::fs::write(&key, "synthetic key path only; never parsed").map_err(|e| e.to_string())?;
        let ssh = connections::Context {
            choices: Choices {
                ssh_key: key.to_string_lossy().into_owned(),
                ..Default::default()
            },
            git_config: profile.git_config.clone(),
            ..Default::default()
        };
        let CloneCommand { command, workspace } = ssh.run(|| {
            clone_command(
                &parent,
                "ssh://git@expected.fixture.invalid:2222/team/repository.git",
                &parent.join("ssh-selected"),
                "origin",
                false,
                &exe,
            )
        })?;
        let out = setup_process(command, &|| true, Duration::from_secs(10))?;
        assert!(!out.status.success());
        drop(workspace);
        let args =
            std::fs::read_to_string(root.join("selected-ssh-args")).map_err(|e| e.to_string())?;
        assert!(args.contains("--gitcraft-profile-ssh"));
        assert!(args.lines().any(|a| a == "-F"));
        assert!(args.lines().any(|a| a == "/dev/null"));
        assert!(args.lines().any(|a| a == "2222"));
        assert!(args.lines().any(|a| a == "git@expected.fixture.invalid"));
        assert!(!args.contains("wrong.fixture.invalid"));
        assert!(!root.join("wrong-ssh-host").exists());
        assert_eq!(
            std::fs::read_to_string(root.join("selected-ssh-agent"))
                .map_err(|e| e.to_string())?
                .trim(),
            root.join("recording-selected-agent.sock").to_string_lossy()
        );
        assert_eq!(
            std::fs::read(parent.join(".git/config")).map_err(|e| e.to_string())?,
            before
        );
        return Ok(());
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
