#[cfg(unix)]
use super::config::entry;
use super::*;
use std::process::Command;

#[test]
fn ssh_key_and_host_helpers_round_trip_through_git_config() -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!(
        "gitcraft-profile-key-config-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let key = directory.join("work 'key' $(not-a-command); `literal` \\");
    let mut profile = UserProfile::default();
    profile.connections.ssh_key = key.to_string_lossy().into_owned();
    profile.connections.github.user = "work".into();
    profile.author_name = "Work author".into();
    let text = config_text(&profile, &[]);
    let file = directory.join("profile.gitconfig");
    std::fs::write(&file, &text).map_err(|e| e.to_string())?;
    let get = |name: &str| -> Result<String, String> {
        let out = Command::new("git")
            .args(["config", "--file"])
            .arg(&file)
            .args(["--get-all", name])
            .output()
            .map_err(|e| e.to_string())?;
        if !out.status.success() {
            return Err(String::from_utf8_lossy(&out.stderr).into_owned());
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_owned())
    };
    assert_eq!(
        get("core.sshCommand")?,
        crate::connections::ssh_helper_command(
            &profile.connections.ssh_key,
            &std::env::current_exe().map_err(|e| e.to_string())?
        )
    );
    assert_eq!(get("user.name")?, "Work author");
    assert_eq!(get("ssh.variant")?, "auto");
    let helpers = get("credential.https://github.com.helper")?;
    assert!(helpers.starts_with('\n'));
    assert!(helpers.contains("--gitcraft-profile-credential gh "));
    assert!(!text.contains("password="));
    let _ = std::fs::remove_dir_all(directory);
    Ok(())
}

#[cfg(unix)]
#[test]
fn selected_openssh_profile_overrides_inherited_putty_variant_and_keeps_nondefault_port()
-> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = std::env::temp_dir().join(format!(
        "gitcraft-profile-ssh-variant-{}",
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
    let recorder = dir.join("record-ssh");
    let args_file = dir.join("args");
    let effective = dir.join("effective");
    // This transport only records Git's arguments and asks real OpenSSH to parse them.
    // -G never connects, and stdout is kept away from Git's protocol parser.
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nssh -F /dev/null -G -o IdentitiesOnly=yes -i {} \"$@\" > {} 2>/dev/null\ncase \" $* \" in *' -G '*) exit 0;; esac\nexit 1\n",
        crate::connections::shell_quote(&args_file.to_string_lossy()),
        crate::connections::shell_quote(&key.to_string_lossy()),
        crate::connections::shell_quote(&effective.to_string_lossy())
    );
    std::fs::write(&recorder, script).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&recorder, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    let global = dir.join("user.gitconfig");
    let config = dir.join("profile.gitconfig");
    let mut profile = UserProfile::default();
    profile.connections.ssh_key = key.to_string_lossy().into_owned();
    for variant in ["plink", "tortoiseplink", "putty", "simple"] {
        std::fs::write(&global, entry("ssh.variant", variant)).map_err(|e| e.to_string())?;
        let mut text = config_text(&profile, std::slice::from_ref(&global));
        // Replace only the executable with the recording transport, retaining profile
        // variant generation and production Git invocation/context handling.
        text.push_str(&entry(
            "core.sshCommand",
            &crate::connections::shell_quote(&recorder.to_string_lossy()),
        ));
        std::fs::write(&config, text).map_err(|e| e.to_string())?;
        let context = crate::connections::Context {
            git_config: Some(config.clone()),
            choices: profile.connections.clone(),
            ..Default::default()
        };
        assert!(
            context
                .run(|| crate::git::run(
                    Some(&repo),
                    &["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"]
                ))
                .is_err()
        );
        let args = std::fs::read_to_string(&args_file).map_err(|e| e.to_string())?;
        assert!(
            args.lines()
                .collect::<Vec<_>>()
                .windows(2)
                .any(|pair| pair == ["-p", "2222"]),
            "{variant}: {args}"
        );
        assert!(!args.lines().any(|arg| arg == "-P"), "{variant}: {args}");
        let parsed = std::fs::read_to_string(&effective).map_err(|e| e.to_string())?;
        assert!(
            parsed.lines().any(|line| line == "port 2222"),
            "{variant}: {parsed}"
        );
    }
    // A local command alone must retain Git's Plink basename detection; requiring
    // users to add a second local ssh.variant setting would break existing repos.
    let plink = dir.join("plink");
    std::fs::write(
        &plink,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit 1\n",
            crate::connections::shell_quote(&args_file.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&plink, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    assert!(
        Command::new("git")
            .current_dir(&repo)
            .args([
                "config",
                "core.sshCommand",
                &crate::connections::shell_quote(&plink.to_string_lossy())
            ])
            .status()
            .map_err(|e| e.to_string())?
            .success()
    );
    let context = crate::connections::Context {
        git_config: Some(config.clone()),
        choices: profile.connections.clone(),
        ..Default::default()
    };
    assert!(
        context
            .run(|| crate::git::run(
                Some(&repo),
                &["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"]
            ))
            .is_err()
    );
    let args = std::fs::read_to_string(&args_file).map_err(|e| e.to_string())?;
    assert!(
        args.lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["-P", "2222"]),
        "{args}"
    );
    assert!(!args.lines().any(|arg| arg == "-p"), "{args}");
    // Git's repository-local policy still has precedence over the generated profile.
    assert!(
        Command::new("git")
            .current_dir(&repo)
            .args(["config", "ssh.variant", "plink"])
            .status()
            .map_err(|e| e.to_string())?
            .success()
    );
    let context = crate::connections::Context {
        git_config: Some(config),
        choices: profile.connections,
        ..Default::default()
    };
    assert!(
        context
            .run(|| crate::git::run(
                Some(&repo),
                &["ls-remote", "ssh://git@fixture.invalid:2222/team/repo"]
            ))
            .is_err()
    );
    let args = std::fs::read_to_string(args_file).map_err(|e| e.to_string())?;
    assert!(
        args.lines()
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| pair == ["-P", "2222"]),
        "{args}"
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
