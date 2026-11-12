//! Selected HTTPS/authentication and SSH assertions for the contained setup fixture.
use super::*;

pub(super) fn run(root: &Path) -> Result<(), String> {
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
    assert_https_and_secondary(root, &profile, &parent, &exe, expected)?;
    assert_anonymous_and_precedence(root, &profile, &parent, &exe, expected)?;
    assert_selected_ssh(root, &profile, &parent, &exe, before)
}

fn assert_https_and_secondary(
    root: &Path,
    profile: &connections::Context,
    parent: &Path,
    exe: &Path,
    expected: &str,
) -> Result<(), String> {
    let CloneCommand { command, workspace } = profile.run(|| {
        clone_command(
            parent,
            expected,
            &parent.join("selected"),
            "origin",
            false,
            exe,
        )
    })?;
    let isolated_home = workspace.0.clone();
    let out = setup_process(command, &|| true, Duration::from_secs(10))?;
    assert!(
        !out.status.success(),
        "Recording transport deliberately refuses a real connection"
    );
    let transport = std::fs::read_to_string(root.join("https-args")).map_err(|e| e.to_string())?;
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
    let args = std::fs::read_to_string(root.join("selected-args")).map_err(|e| e.to_string())?;
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
    // A secondary fetch uses an owned fresh Git directory, not the hostile parent
    // or profile includes, and must reach the same selected helper boundary.
    let fetched = root.join("owned-secondary-target");
    std::fs::create_dir(&fetched).map_err(|e| e.to_string())?;
    crate::git::practice_run(&fetched, &["init", "-q"])?;
    let CloneCommand { command, workspace } =
        profile.run(|| fetch_command(&fetched, expected, "secondary", exe))?;
    let out = setup_process(command, &|| true, Duration::from_secs(10))?;
    assert!(
        !out.status.success(),
        "Recording transport refuses a real connection"
    );
    drop(workspace);
    let transport = std::fs::read_to_string(root.join("https-args")).map_err(|e| e.to_string())?;
    assert_eq!(transport.lines().collect::<Vec<_>>(), [expected, expected]);
    let credentials =
        std::fs::read_to_string(root.join("credentials")).map_err(|e| e.to_string())?;
    assert!(
        credentials.contains("username=expected-work")
            && credentials.contains("password=FIXTURE_SELECTED_TOKEN")
    );
    assert!(!root.join("ambient-executed").exists());
    assert!(!root.join("wrong-ssh-host").exists());
    Ok(())
}

fn assert_anonymous_and_precedence(
    root: &Path,
    profile: &connections::Context,
    parent: &Path,
    exe: &Path,
    expected: &str,
) -> Result<(), String> {
    use base64::Engine as _;
    std::fs::remove_file(root.join("selected-args")).map_err(|e| e.to_string())?;
    let CloneCommand { command, workspace } = connections::Context::default().run(|| {
        clone_command(
            parent,
            expected,
            &parent.join("anonymous"),
            "origin",
            false,
            exe,
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
            parent,
            expected,
            &parent.join("last-explicit-helper"),
            "origin",
            false,
            exe,
        )
    })?;
    let out = setup_process(command, &|| true, Duration::from_secs(10))?;
    assert!(!out.status.success());
    drop(workspace);
    let args = std::fs::read_to_string(root.join("selected-args")).map_err(|e| e.to_string())?;
    let args = args.lines().collect::<Vec<_>>();
    assert_eq!(args[1], "glab");
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(args[2])
        .map_err(|e| e.to_string())?;
    let account: connections::Account =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    assert_eq!(account, both.choices.gitlab);
    assert!(!root.join("ambient-executed").exists());
    Ok(())
}

fn assert_selected_ssh(
    root: &Path,
    profile: &connections::Context,
    parent: &Path,
    exe: &Path,
    before: Vec<u8>,
) -> Result<(), String> {
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
            parent,
            "ssh://git@expected.fixture.invalid:2222/team/repository.git",
            &parent.join("ssh-selected"),
            "origin",
            false,
            exe,
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
    Ok(())
}
