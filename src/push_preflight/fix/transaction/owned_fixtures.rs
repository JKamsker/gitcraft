//! Gate 2 only: isolated, explicitly selected owned-local fixtures. Never default tests.
use super::*;
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use std::time::Duration;

// Includes preserve the externally selected Gate-2 test name.
include!("owned_fixtures/fixture.rs");
include!("owned_fixtures/transaction_cases.rs");
include!("owned_fixtures/tree_repairs.rs");
include!("owned_fixtures/native_locking.rs");

#[test]
fn gate2_owned_local_fixture() -> Result<(), String> {
    let Some(case) = std::env::var_os("GITCRAFT_GATE2_CASE") else {
        return default_owned_suite();
    };
    let case = case.to_str().ok_or("Owned case must be UTF-8")?;
    let root = PathBuf::from(
        std::env::var_os("GITCRAFT_GATE2_ROOT").ok_or("Explicit owned root required")?,
    );
    if std::env::var_os("GITCRAFT_GATE2_CHILD").is_some() {
        return execute_case(&root, case);
    }
    owned_child(&root, case, &fixture_git()?)
}
fn default_owned_suite() -> Result<(), String> {
    let git = fixture_git()?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gitcraft-owned-repair-default-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    if !supported_version(&version(&git)?) {
        println!(
            "Owned repair: older Git must refuse before mutation; evidence {}",
            root.display()
        );
        return owned_child(&root.join("capability-refusal"), "capability-refusal", &git);
    }
    for case in [
        "files-sha1",
        "files-sha256",
        "reftable-sha1",
        "reftable-sha256",
        "prelock-switch-head",
        "prelock-detach-head",
        "switch-head",
        "detach-head",
        "reftable-switch-head",
        "reftable-detach-head",
        "held-ref-lock",
        "veto",
        "index-change",
        "work-change",
        "checkout-failure",
        "committed-failure",
        "timeout",
        "split-index",
        "missing-ack",
        "full-fix",
    ] {
        println!("Owned default case {case}: {}", root.display());
        owned_child(&root.join(case), case, &git)?;
    }
    let old = Path::new("/usr/bin/git");
    if old.is_file() && !supported_version(&version(old)?) {
        owned_child(&root.join("old-git"), "old-git", &git)?;
    }
    Ok(())
}
fn owned_child(root: &Path, case: &str, git: &Path) -> Result<(), String> {
    fs::create_dir(root).map_err(|e| e.to_string())?;
    fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    for name in ["home", "xdg", "tmp", "template"] {
        fs::create_dir(root.join(name)).map_err(|e| e.to_string())?;
    }
    write(root.join("global.gitconfig"), b"")?;
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .env_clear()
        .current_dir(root)
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env(
            "PATH",
            if case == "old-git" {
                "/usr/bin:/bin".into()
            } else {
                std::env::join_paths([
                    git.parent().ok_or("Git executable has no parent")?,
                    Path::new("/usr/bin"),
                    Path::new("/bin"),
                ])
                .map_err(|e| e.to_string())?
            },
        )
        .env("GITCRAFT_GATE2_GIT", git)
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("xdg"))
        .env("TMPDIR", root.join("tmp"))
        .env("GIT_CONFIG_GLOBAL", root.join("global.gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TEMPLATE_DIR", root.join("template"))
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_COUNT", "0")
        .env("GITCRAFT_GATE2_CASE", case)
        .env("GITCRAFT_GATE2_ROOT", root)
        .env("GITCRAFT_GATE2_CHILD", "1");
    let output = crate::push_preflight::recording_input(
        command,
        None,
        &|| true,
        Duration::from_secs(45),
        1024 * 1024,
        1024 * 1024,
    )?;
    write(root.join("child.stdout"), &output.stdout)?;
    write(root.join("child.stderr"), &output.stderr)?;
    println!("owned case {case}, evidence {}", root.display());
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
