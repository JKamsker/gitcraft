//! Fixture-only lifecycle/recording checks. No real SSH tools, keys, agents or accounts.
use super::*;
use std::sync::atomic::AtomicU64;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "gitcraft-ssh-helper-fixture-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
    fn state(&self, ctx: &egui::Context) -> State {
        State::new(&RepoTab::new_deferred(self.0.clone(), ctx), ctx)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn exclusive_folder_reservation_never_reuses_or_removes_existing_files() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let chosen = fixture.0.join("new key folder ' ü");
    reserve_folder(&chosen)?;
    std::fs::write(chosen.join("sentinel"), b"fixture-original").map_err(|e| e.to_string())?;
    assert!(reserve_folder(&chosen).is_err());
    assert_eq!(
        std::fs::read(chosen.join("sentinel")).map_err(|e| e.to_string())?,
        b"fixture-original"
    );
    assert!(reserve_folder(&fixture.0.join("missing-parent/child")).is_err());
    assert!(!fixture.0.join("missing-parent").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&chosen)
                .map_err(|e| e.to_string())?
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
    }
    Ok(())
}

#[test]
fn closing_worker_drops_late_results_without_applying_an_action() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let ctx = egui::Context::default();
    let mut state = fixture.state(&ctx);
    let (started, wait_started) = mpsc::channel();
    let (release, wait_release) = mpsc::channel();
    state.background(&ctx, move |_| {
        let _ = started.send(());
        let _ = wait_release.recv_timeout(std::time::Duration::from_secs(2));
        Ok(Ready::Uploaded)
    });
    wait_started
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    let active = state.active.clone().ok_or("No cancellation ownership")?;
    state.cancel_worker();
    assert!(!active.load(Ordering::Acquire));
    release.send(()).map_err(|e| e.to_string())?;
    assert!(state.receiver.is_none() && state.account.is_none() && state.command.is_none());
    Ok(())
}

#[test]
fn remote_plans_never_need_local_profile_key_or_token_values() -> Result<(), String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let machine = Machine::Remote(crate::remote::Target {
        host: crate::remote::Host::Wsl("Fixture Linux".into()),
        path: "/fixture/repo".into(),
    });
    let plan = plan::remote_registration(
        &machine,
        "~/key ' ü/id_ed25519",
        "github.com",
        Some("fixture-user"),
        None,
        "fixture title",
    )?;
    assert_eq!(plan.shell.program, "wsl.exe");
    let script = plan.shell.args.last().ok_or("No fixed script")?;
    assert!(script.contains("gh auth token --hostname \"$host\" --user \"$user\""));
    assert!(script.contains(
        "unset GH_DEBUG GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN GITHUB_ENTERPRISE_TOKEN"
    ));
    assert!(script.contains("--raw-field \"key=$public\"") && !script.contains("key=@"));
    assert!(script.contains("ssh-keygen -l -f /dev/stdin"));
    assert!(!script.contains("auth switch") && !script.contains("kill") && !script.contains("ps "));
    let target = Machine::Remote(crate::remote::Target {
        host: crate::remote::Host::Ssh("fixture-host".into()),
        path: "/fixture/repo".into(),
    });
    let plan = plan::remote_registration(
        &target,
        "~/key/id_ed25519",
        "forge.example",
        None,
        Some("fixture-login"),
        "fixture title",
    )?;
    let encoded = plan
        .shell
        .args
        .last()
        .and_then(|s| s.rsplit_once(' '))
        .map(|(_, payload)| payload)
        .ok_or("No encoded script")?;
    let decoded = String::from_utf8(STANDARD.decode(encoded).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    assert!(
        decoded.contains("Type UPLOAD")
            && decoded.contains("mktemp")
            && decoded.contains("--login 'fixture-login'")
    );
    assert!(!decoded.contains("-- \"$key.pub\""));
    Ok(())
}

#[test]
fn practice_backstop_refuses_before_any_setup_job() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let root = fixture.0.join("gitcraft-practice-fixture");
    let repo = root.join("repository");
    std::fs::create_dir_all(repo.join(".git")).map_err(|e| e.to_string())?;
    let oid = "0".repeat(40);
    std::fs::write(repo.join(".git/gitcraft-practice.json"),serde_json::json!({"version":1,"scenario":"detached","before":oid,"target":"0".repeat(40),"original_tree":"0".repeat(40)}).to_string()).map_err(|e|e.to_string())?;
    // Use the enum's actual serialization; the marker grants no external capability.
    let mut marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(repo.join(".git/gitcraft-practice.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    marker["scenario"] =
        serde_json::to_value(crate::practice::Scenario::Detached).map_err(|e| e.to_string())?;
    std::fs::write(repo.join(".git/gitcraft-practice.json"), marker.to_string())
        .map_err(|e| e.to_string())?;
    assert!(crate::practice::is_repository(&repo));
    let ctx = egui::Context::default();
    let tab = RepoTab::new_deferred(repo.clone(), &ctx);
    let mut state = State::new(&tab, &ctx);
    let touched = fixture.0.join("should-not-run");
    let attempted = touched.clone();
    state.background(&ctx, move |_| {
        std::fs::write(attempted, b"fixture").map_err(|e| e.to_string())?;
        Ok(Ready::Uploaded)
    });
    let result = state
        .receiver
        .as_ref()
        .ok_or("No worker")?
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|e| e.to_string())?;
    assert!(result.is_err_and(|e| e.contains("Practice")));
    assert!(!touched.exists());
    Ok(())
}

#[cfg(unix)]
#[test]
fn owned_recording_terminal_reports_actual_exit_without_generating_a_key() -> Result<(), String> {
    Context::default().run(|| {
        use std::os::unix::fs::PermissionsExt;
        let fixture = Fixture::new()?;
        let ctx = egui::Context::default();
        let mut state = fixture.state(&ctx);
        let fake = fixture.0.join("record-only-tool");
        let marker = fixture.0.join("arguments");
        std::fs::write(
            &fake,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}\nexit 7\n",
                crate::remote::sh_quote(&marker.to_string_lossy())
            ),
        )
        .map_err(|e| e.to_string())?;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let folder = fixture.0.join("new key");
        let (mut invocation, key) = plan::generate(
            &Machine::Local { windows: false },
            &folder.to_string_lossy(),
            "fixture title",
        )?;
        reserve_folder(&folder)?;
        invocation.shell.program = fake.to_string_lossy().into_owned();
        state.launch(invocation, Step::Generate, Some(key.clone()), &ctx);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            state.poll(&ctx);
            if state.command.as_ref().is_some_and(|run| run.finished) {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("Owned recording terminal did not finish".into());
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(state.status.contains("exited 7"));
        assert!(!PathBuf::from(key).exists());
        let arguments = std::fs::read_to_string(marker).map_err(|e| e.to_string())?;
        assert!(
            arguments.contains("ed25519") && !arguments.lines().any(|a| a == "-N" || a == "-P")
        );
        Ok(())
    })
}
