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
    let verify = |command| {
        read_private(command)
            .and_then(|json| verify_user_output("gh", &account, &json))
            .map(|_| ())
    };
    let command = discovery_command(&account, verify, |error| error)?;
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
    assert!(
        discovery_command(
            &wrong,
            |command| read_private(command)
                .and_then(|json| verify_user_output("gh", &wrong, &json))
                .map(|_| ()),
            |error| error
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn discovery_session_is_invalid_after_actual_a_b_a_transition() {
    let original = current();
    struct RestoreContext(Context);
    impl Drop for RestoreContext {
        fn drop(&mut self) {
            set(self.0.clone());
        }
    }
    let _restore = RestoreContext(original.clone());
    let mut a = original.clone();
    a.choices.github.user = "discovery-session-a".into();
    let mut b = a.clone();
    b.choices.github.user = "discovery-session-b".into();
    set(a.clone());
    let session = crate::forge::discovery::DiscoverySession::current();
    assert!(session.valid());
    set(b);
    set(a);
    assert!(!session.valid());
}
