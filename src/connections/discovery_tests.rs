//! Selected GitHub identity is verified with the exact worker-local token.
use super::*;
#[cfg(unix)]
fn fake_gh(owner: &str) -> Result<(tempfile::TempDir, PathBuf), Box<dyn std::error::Error>> {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir()?;
    let program = dir.path().join("gh");
    let script = format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n 'auth token') printf '%s' 'fixture-token';;\n 'api user') [ \"$GH_TOKEN\" = 'fixture-token' ] || exit 9; printf '%s' '{{\"login\":\"{owner}\"}}';;\n *) exit 10;;\nesac\n"
    );
    std::fs::write(&program, script)?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))?;
    Ok((dir, program))
}
#[cfg(unix)]
struct Restore(Option<PathBuf>);
#[cfg(unix)]
impl Drop for Restore {
    fn drop(&mut self) {
        TEST_PROGRAMS.with(|programs| {
            let mut programs = programs.borrow_mut();
            if let Some(previous) = self.0.take() {
                programs.insert("gh".into(), previous);
            } else {
                programs.remove("gh");
            }
        });
    }
}
#[cfg(unix)]
#[test]
fn discovery_verifies_selected_identity_and_never_switches_global_account()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, program) = fake_gh("selected-user")?;
    let _restore =
        Restore(TEST_PROGRAMS.with(|programs| programs.borrow_mut().insert("gh".into(), program)));
    let account = Account {
        host: "github.com".into(),
        user: "selected-user".into(),
        config_dir: String::new(),
    };
    let command = discovery_command(&account)?;
    let env: std::collections::HashMap<_, _> = command.get_envs().collect();
    assert_eq!(
        env.get(std::ffi::OsStr::new("GH_TOKEN")),
        Some(&Some(std::ffi::OsStr::new("fixture-token")))
    );
    // The fixture accepts only token resolution and /user verification. A switch would fail.
    assert!(command.get_args().next().is_none());
    let wrong = Account {
        user: "another-user".into(),
        ..account
    };
    assert!(discovery_command(&wrong).is_err());
    Ok(())
}
