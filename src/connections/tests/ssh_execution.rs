// Ssh execution. Included to preserve libtest shard names.
/// A subprocess entry exercises precisely run_ssh, including its final transport launch.
/// Ordinary test runs do nothing here; only the fixture parent supplies this metadata.
#[cfg(unix)]
#[test]
fn profile_ssh_fixture_child() -> Result<(), String> {
    let Some(request) = std::env::var_os("GITCRAFT_SSH_HELPER_FIXTURE") else {
        return Ok(());
    };
    let value: serde_json::Value =
        serde_json::from_str(&request.to_string_lossy()).map_err(|e| e.to_string())?;
    let key = value["key"].as_str().ok_or("missing fixture key")?;
    let bytes = std::fs::read(value["args_file"].as_str().ok_or("missing fixture argv")?)
        .map_err(|e| e.to_string())?;
    let args = bytes
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8(s.to_vec()).map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let result = run_ssh(key, &args);
    if value["relay"].as_bool() == Some(true) {
        match result {
            Ok(code) => std::process::exit(code),
            Err(error) => {
                eprintln!("SSH fixture: {error}");
                std::process::exit(1);
            }
        }
    }
    assert_eq!(
        result.is_ok(),
        value["expect"].as_bool().unwrap_or(false),
        "{result:?}"
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn actual_ssh_helper_refuses_lf_cr_and_different_keys_before_transport_and_accepts_equivalent_paths()
-> Result<(), String> {
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-controls-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let key = dir.join("fixture.invalid 'quoted' \\ key");
    let fixture_key = |path: &Path| -> Result<(), String> {
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(format!("{}.pub", path.display()));
        let status = Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-f"])
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Could not create an isolated SSH test key".into());
        }
        Ok(())
    };
    fixture_key(&key)?;
    let config = dir.join("ssh_config");
    let marker = dir.join("transport-reached");
    let args_file = dir.join("args");
    let proxy = format!(
        "printf reached > {}; exec false",
        shell_quote(&marker.to_string_lossy())
    );
    let args = vec![
        "-F".to_owned(),
        config.to_string_lossy().into_owned(),
        "-o".into(),
        "IdentityAgent=none".into(),
        "-o".into(),
        "UserKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "GlobalKnownHostsFile=/dev/null".into(),
        "-o".into(),
        "BatchMode=yes".into(),
        "-o".into(),
        format!("ProxyCommand={proxy}"),
        "fixture".into(),
    ];
    let mut argv = Vec::new();
    for arg in &args {
        argv.extend_from_slice(arg.as_bytes());
        argv.push(0);
    }
    std::fs::write(&args_file, argv).map_err(|e| e.to_string())?;
    let probe = Command::new("ssh")
        .args(["-G", "-F", "/dev/null", "-o", "ControlPath=%d", "fixture"])
        .output()
        .map_err(|e| e.to_string())?;
    assert!(probe.status.success());
    let probe = String::from_utf8_lossy(&probe.stdout);
    let home = probe
        .lines()
        .find_map(|line| line.strip_prefix("controlpath "))
        .ok_or("missing SSH fixture home")?;
    let up = "../".repeat(
        Path::new(home)
            .components()
            .filter(|p| matches!(p, std::path::Component::Normal(_)))
            .count(),
    );
    let absolute = key.to_string_lossy().into_owned();
    for (identity, value, expected) in [
        ("${FIXTURE_IDENTITY}".into(), absolute.clone(), true),
        (
            "${FIXTURE_IDENTITY}".into(),
            format!("{absolute}\nother-key"),
            false,
        ),
        ("${FIXTURE_IDENTITY}".into(), format!("{absolute}\r"), false),
        ("${FIXTURE_IDENTITY}".into(), format!("{absolute} "), false),
        (
            "${FIXTURE_IDENTITY}".into(),
            dir.join("other-key").to_string_lossy().into_owned(),
            false,
        ),
        (
            format!("~/{up}{}", absolute.trim_start_matches('/')),
            absolute.clone(),
            true,
        ),
        (
            absolute.replace("fixture.invalid", "%h"),
            absolute.clone(),
            true,
        ),
    ] {
        // These valid generated test keys have no account/server; unusual Unix names
        // must be refused before the recording ProxyCommand can start any transport.
        if !expected {
            fixture_key(Path::new(&value))?;
        }
        let escaped = identity.replace('\\', "\\\\").replace('"', "\\\"");
        std::fs::write(
            &config,
            format!("Host fixture\n HostName fixture.invalid\n IdentityFile \"{escaped}\"\n"),
        )
        .map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(&marker);
        let request = serde_json::json!({"key":absolute,"args_file":args_file,"expect":expected});
        let output = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args([
                "--exact",
                "connections::tests::profile_ssh_fixture_child",
                "--nocapture",
            ])
            .env("GITCRAFT_SSH_HELPER_FIXTURE", request.to_string())
            .env("FIXTURE_IDENTITY", &value)
            .env("PATH", "/usr/bin:/bin")
            .env_remove("SSH_AUTH_SOCK")
            .output()
            .map_err(|e| e.to_string())?;
        assert!(
            output.status.success(),
            "case {identity:?} {value:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(marker.exists(), expected, "{identity:?} {value:?}");
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}

#[cfg(unix)]
#[test]
fn actual_profile_ssh_wrapper_auto_probe_preserves_nondefault_port_over_inherited_plink()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-real-port-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    assert!(
        Command::new("git")
            .args(["init", "-q"])
            .arg(&repo)
            .status()
            .map_err(|e| e.to_string())?
            .success()
    );
    let key = dir.join("selected key");
    std::fs::write(&key, "").map_err(|e| e.to_string())?;
    let marker = dir.join("actual-port");
    let ssh_config = dir.join("ssh_config");
    let proxy = format!(
        "sh -c {}",
        shell_quote(&format!(
            "echo %p > {}; exec false",
            shell_quote(&marker.to_string_lossy())
        ))
    );
    std::fs::write(&ssh_config,format!("Host *\n HostName fixture.invalid\n IdentityAgent none\n UserKnownHostsFile /dev/null\n GlobalKnownHostsFile /dev/null\n BatchMode yes\n ProxyCommand {proxy}\n")).map_err(|e|e.to_string())?;
    let args_file = dir.join("args");
    let wrapper = dir.join("profile-ssh-wrapper");
    std::fs::write(&wrapper,format!("#!/bin/sh\nprintf '%s\\0' '-F' {} \"$@\" > {}\n{} --exact connections::tests::profile_ssh_fixture_child --nocapture >/dev/null\nexit $?\n",shell_quote(&ssh_config.to_string_lossy()),shell_quote(&args_file.to_string_lossy()),shell_quote(&std::env::current_exe().map_err(|e|e.to_string())?.to_string_lossy()))).map_err(|e|e.to_string())?;
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let inherited = dir.join("user.gitconfig");
    std::fs::write(&inherited, "[ssh]\n variant = plink\n").map_err(|e| e.to_string())?;
    let profile_file = dir.join("profile.gitconfig");
    let mut profile = crate::settings::profiles::UserProfile::default();
    profile.connections.ssh_key = key.to_string_lossy().into_owned();
    let mut text = crate::settings::profiles::config_text(&profile, &[inherited]);
    // Test executable relays to the exact production helper body; no SSH imitation.
    text.push_str(&format!(
        "[core]\n sshCommand = {}\n",
        serde_json::to_string(&shell_quote(&wrapper.to_string_lossy()))
            .map_err(|e| e.to_string())?
    ));
    std::fs::write(&profile_file, text).map_err(|e| e.to_string())?;
    let request = serde_json::json!({"key":key,"args_file":args_file,"relay":true});
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
    let probe_marker = dir.join("ssh-was-started");
    let real_ssh = crate::shell_path::resolve("ssh");
    std::fs::write(
        bin.join("ssh"),
        format!(
            "#!/bin/sh\nprintf called > {}\nexec {} \"$@\"\n",
            shell_quote(&probe_marker.to_string_lossy()),
            shell_quote(&real_ssh)
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(bin.join("ssh"), std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let path = std::env::join_paths([bin, PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
        .map_err(|e| e.to_string())?;
    for source in ["local", "environment"] {
        for variant in ["auto", "ssh", "simple", "plink", "putty", "tortoiseplink"] {
            let _ = std::fs::remove_file(&marker);
            let _ = std::fs::remove_file(&args_file);
            let _ = std::fs::remove_file(&probe_marker);
            let mut setting = Command::new("git");
            setting
                .current_dir(&repo)
                .env("GIT_CONFIG_GLOBAL", &profile_file)
                .env("GIT_CONFIG_NOSYSTEM", "1");
            if source == "local" {
                assert!(
                    setting
                        .args(["config", "--local", "ssh.variant", variant])
                        .status()
                        .map_err(|e| e.to_string())?
                        .success()
                );
            } else {
                let _ = setting
                    .args(["config", "--local", "--unset-all", "ssh.variant"])
                    .status();
            }
            let mut command = Command::new("git");
            command
                .current_dir(&repo)
                .args(["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"])
                .env("GIT_CONFIG_GLOBAL", &profile_file)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_COUNT", "0")
                .env("GITCRAFT_SSH_HELPER_FIXTURE", request.to_string())
                .env("PATH", &path)
                .env_remove("GIT_SSH_COMMAND")
                .env_remove("GIT_SSH")
                .env_remove("SSH_AUTH_SOCK");
            if source == "environment" {
                command.env("GIT_SSH_VARIANT", variant);
            } else {
                command.env_remove("GIT_SSH_VARIANT");
            }
            let output = command.output().map_err(|e| e.to_string())?;
            assert!(!output.status.success()); // all recording ProxyCommands refuse transport
            if matches!(variant, "auto" | "ssh") {
                assert_eq!(
                    std::fs::read_to_string(&marker).map_err(|e| format!(
                        "{source}/{variant}: {e}: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ))?,
                    "2222\n"
                );
                let args = std::fs::read(&args_file).map_err(|e| e.to_string())?;
                let args = args
                    .split(|b| *b == 0)
                    .filter_map(|s| std::str::from_utf8(s).ok())
                    .collect::<Vec<_>>();
                assert!(args.windows(2).any(|pair| pair == ["-p", "2222"]));
                assert!(!args.contains(&"-P"));
            } else {
                assert!(
                    !marker.exists(),
                    "{source}/{variant}: incompatible dialect reached a transport"
                );
                assert!(
                    !probe_marker.exists(),
                    "{source}/{variant}: incompatible dialect reached an SSH probe"
                );
                assert_eq!(
                    args_file.exists(),
                    variant != "simple",
                    "{source}/{variant}: Git/helper boundary control"
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
