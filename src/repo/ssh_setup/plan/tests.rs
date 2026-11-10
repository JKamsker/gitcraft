use super::*;
#[test]
fn local_generation_is_interactive_and_requires_exclusive_new_folder() -> Result<(), String> {
    for (windows, folder) in [
        (false, "/fixture/key folder ' ü"),
        (true, r"C:\fixture\key folder ' ü"),
    ] {
        let (plan, key) = generate(&Machine::Local { windows }, folder, "fixture title")?;
        assert_eq!(plan.reserve_folder, Some(PathBuf::from(folder)));
        assert_eq!(plan.shell.program, "ssh-keygen");
        assert!(plan.shell.args.contains(&key));
        assert!(!plan.shell.args.iter().any(|a| a == "-N" || a == "-P"));
    }
    Ok(())
}
#[test]
fn remote_scripts_quote_paths_without_local_key_or_credential_routing() -> Result<(), String> {
    let machine = Machine::Remote(Target {
        host: Host::Ssh("work@build:2222".into()),
        path: "/repo".into(),
    });
    let key = "~/key ' $(touch INJECTED) ü/id_ed25519";
    let invocation = agent(&machine, key)?;
    assert_eq!(invocation.shell.program, "ssh");
    assert_eq!(
        &invocation.shell.args[..4],
        &["-t", "-a", "--", "ssh://work@build:2222"]
    );
    let (_, payload) = invocation
        .shell
        .args
        .last()
        .ok_or("No remote script")?
        .rsplit_once(' ')
        .ok_or("No fixed encoded payload")?;
    let script = String::from_utf8(STANDARD.decode(payload).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    assert!(script.contains(&sh_path(key)));
    assert!(!script.contains("kill") && !script.contains("ps "));
    let (create, _) = generate(&machine, "~/new fixture", "fixture")?;
    assert!(create.reserve_folder.is_none());
    Ok(())
}
#[test]
fn chosen_identity_test_ignores_other_identity_files_and_agent_keys() -> Result<(), String> {
    let plan = test(
        &Machine::Local { windows: false },
        "/fixture/key",
        "git@forge.example:2222",
    )?;
    assert_eq!(
        plan.shell.args.last().map(String::as_str),
        Some("ssh://git@forge.example:2222")
    );
    assert!(plan.shell.args.iter().any(|a| a == "IdentityAgent=none"));
    assert_eq!(&plan.shell.args[..3], &["-F", "none", "-T"]);
    for bad in ["-oProxyCommand=fixture", "bad\nhost", "git@host;touch"] {
        assert!(test(&Machine::Local { windows: false }, "/fixture/key", bad).is_err());
    }
    Ok(())
}
#[test]
fn windows_reserved_wsl_unc_never_falls_back_after_parse_failure() {
    for path in [
        r"\\wsl.localhost\-bad\repo",
        r"\\wsl$\\repo",
        r"\\WSL.LOCALHOST\",
        r"\\wsl$",
        r"\\?\UNC\wsl.localhost\-bad\repo",
        r"\\?\uNc\WSL$\\repo",
        r"\\.\UNC\wsl$\-bad\repo",
        "//wsl.localhost/-bad/repo",
        r"\\WsL.LocalHost/-bad\repo",
        "//?/UnC/WSL.LOCALHOST//repo",
    ] {
        assert!(reserved_wsl_unc(path), "{path}");
        let mut operation = false;
        let result = Machine::unparsed(Path::new(path), true).inspect(|_| {
            operation = true;
        });
        assert!(result.is_err(), "{path}");
        assert!(
            !operation,
            "No key/agent/account operation may follow an invalid machine"
        );
    }
}

#[test]
fn ordinary_unc_and_local_paths_keep_their_platform_boundary() -> Result<(), String> {
    for path in [
        r"\\server\share\repo",
        r"\\?\UNC\server\share\repo",
        r"\\.\UNC\server\share\repo",
        r"\\wsl.localhost.example\share\repo",
        r"\\wsl$-backup\share\repo",
        r"C:\work\repo",
        r"\\?\C:\work\repo",
        "/home/me/repo",
    ] {
        assert!(!reserved_wsl_unc(path), "{path}");
        assert!(
            matches!(
                Machine::unparsed(Path::new(path), true)?,
                Machine::Local { windows: true }
            ),
            "{path}"
        );
    }
    // On non-Windows systems these spellings remain ordinary local share paths.
    assert!(matches!(
        Machine::unparsed(Path::new(r"\\wsl.localhost\-bad\repo"), false)?,
        Machine::Local { windows: false }
    ));
    Ok(())
}

#[cfg(windows)]
#[test]
fn native_windows_valid_wsl_unc_variants_stay_remote() -> Result<(), String> {
    for path in [
        r"\\wsl.localhost\Ubuntu\home\me\repo",
        r"\\wsl$\Ubuntu\home\me\repo",
        r"\\?\UNC\wsl.localhost\Ubuntu\home\me\repo",
        r"\\?\unc\WSL$\Ubuntu\home\me\repo",
        "//wsl.localhost/Ubuntu/home/me/repo",
        r"\\WsL.LocalHost/Ubuntu\home/me\repo",
    ] {
        assert!(
            matches!(Machine::of(Path::new(path))?, Machine::Remote(Target { host: Host::Wsl(distro), path }) if distro=="Ubuntu" && path=="/home/me/repo")
        );
    }
    Ok(())
}
