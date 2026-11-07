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
    fn state(&self, ctx: &egui::Context) -> Result<State, String> {
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
    let mut state = fixture.state(&ctx)?;
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
        host: crate::remote::Host::Wsl("Fixture-Linux".into()),
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
    let upload = decoded
        .lines()
        .find(|line| line.starts_with("if tea ssh-keys add "))
        .ok_or("No fixed public upload command")?;
    assert!(upload.contains("$public_file") && !upload.contains("$key.pub"));
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
    let mut state = State::new(&tab, &ctx)?;
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
        let mut state = fixture.state(&ctx)?;
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

#[cfg(unix)]
mod recording {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    fn tool(fixture: &Fixture, name: &str, body: &str) -> Result<(), String> {
        let bin = fixture.0.join("bin");
        std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
        let path = bin.join(name);
        std::fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n"))
            .map_err(|e| e.to_string())?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())
    }
    fn script(invocation: &Invocation) -> Result<String, String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        if invocation.shell.program == "wsl.exe" {
            return invocation
                .shell
                .args
                .last()
                .cloned()
                .ok_or("No WSL script".into());
        }
        let encoded = invocation
            .shell
            .args
            .last()
            .and_then(|s| s.rsplit_once(' '))
            .map(|(_, payload)| payload)
            .ok_or("No encoded SSH script")?;
        String::from_utf8(STANDARD.decode(encoded).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
    fn run(
        fixture: &Fixture,
        invocation: &Invocation,
        input: &[u8],
    ) -> Result<std::process::Output, String> {
        let mut command = Command::new("/bin/sh");
        let tmp = fixture.0.join("tmp");
        std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        command
            .env_clear()
            .current_dir(&fixture.0)
            .env("HOME", &fixture.0)
            .env("TMPDIR", tmp)
            .env("FIXTURE_ROOT", &fixture.0)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", fixture.0.join("bin").display()),
            )
            .env("GH_TOKEN", "foreign-fixture-token")
            .env("GITEA_SERVER_TOKEN", "foreign-fixture-token")
            .args(["-c", &script(invocation)?]);
        crate::bounded_process::run_input(
            command,
            Some(input.to_vec()),
            &|| true,
            std::time::Duration::from_secs(3),
            65536,
            16384,
        )
    }
    fn machine() -> Machine {
        Machine::Remote(crate::remote::Target {
            host: crate::remote::Host::Ssh("fixture-machine".into()),
            path: "/fixture/repository".into(),
        })
    }
    #[test]
    fn remote_generation_agent_and_test_use_recording_tools_and_never_overwrite()
    -> Result<(), String> {
        let fixture = Fixture::new()?;
        for name in ["ssh-keygen", "ssh-add", "ssh"] {
            tool(
                &fixture,
                name,
                &format!("printf '%s\\n' \"$@\" >> \"$FIXTURE_ROOT/{name}-args\""),
            )?;
        }
        let folder = "~/key '$(touch INJECTED)' ü";
        let (generate, key) = plan::generate(&machine(), folder, "fixture ' $(touch OTHER) title")?;
        assert!(run(&fixture, &generate, b"")?.status.success());
        let created = fixture.0.join("key '$(touch INJECTED)' ü");
        std::fs::write(created.join("sentinel"), b"fixture-original").map_err(|e| e.to_string())?;
        assert!(!run(&fixture, &generate, b"")?.status.success());
        assert_eq!(
            std::fs::read(created.join("sentinel")).map_err(|e| e.to_string())?,
            b"fixture-original"
        );
        assert!(!fixture.0.join("INJECTED").exists() && !fixture.0.join("OTHER").exists());
        assert!(
            run(&fixture, &plan::agent(&machine(), &key)?, b"")?
                .status
                .success()
        );
        let expected = format!(
            "{}{}",
            fixture.0.display(),
            key.strip_prefix('~')
                .ok_or("No home-relative fixture path")?
        );
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("ssh-add-args")).map_err(|e| e.to_string())?,
            format!("--\n{expected}\n")
        );
        assert!(
            run(
                &fixture,
                &plan::test(&machine(), &key, "git@forge.example:2222")?,
                b""
            )?
            .status
            .success()
        );
        assert!(
            std::fs::read_to_string(fixture.0.join("ssh-args"))
                .map_err(|e| e.to_string())?
                .contains("ssh://git@forge.example:2222")
        );
        Ok(())
    }
    #[test]
    fn remote_registration_uploads_only_validated_public_fields_using_named_login()
    -> Result<(), String> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let fixture = Fixture::new()?;
        tool(&fixture, "ssh-keygen", "cat >/dev/null")?;
        tool(
            &fixture,
            "gh",
            r#"case "$1 $2" in
 'auth token') [ -z "${GH_TOKEN:-}" ] || exit 90; printf 'lookup\n' >> "$FIXTURE_ROOT/gh-lookups"; printf fixture-auth-token;;
 'api --hostname')
   [ "${GH_TOKEN:-}" = fixture-auth-token ] && [ "${GH_ENTERPRISE_TOKEN:-}" = fixture-auth-token ] || exit 91
   if [ "$4" = user ]; then printf fixture-user; else printf '%s\n' "$@" > "$FIXTURE_ROOT/gh-upload-args"; fi;;
 *) exit 92;;
esac"#,
        )?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&11u32.to_be_bytes());
        bytes.extend_from_slice(b"ssh-ed25519");
        bytes.extend_from_slice(&32u32.to_be_bytes());
        bytes.extend_from_slice(&[7; 32]);
        let public = format!("ssh-ed25519 {}", STANDARD.encode(bytes));
        std::fs::write(
            fixture.0.join("chosen.pub"),
            format!("{public} fixture-private-comment\n"),
        )
        .map_err(|e| e.to_string())?;
        let invocation = plan::remote_registration(
            &machine(),
            "~/chosen",
            "github.com",
            Some("fixture-user"),
            None,
            "fixture title",
        )?;
        let output = run(&fixture, &invocation, b"")?;
        assert!(output.status.success());
        let arguments =
            std::fs::read_to_string(fixture.0.join("gh-upload-args")).map_err(|e| e.to_string())?;
        assert!(
            arguments
                .lines()
                .any(|line| line == format!("key={public}"))
        );
        assert!(
            !arguments.contains("fixture-private") && !arguments.contains("fixture-auth-token")
        );
        std::fs::write(
            fixture.0.join("chosen.pub"),
            b"-----BEGIN OPENSSH PRIVATE KEY-----\nfixture-private\n",
        )
        .map_err(|e| e.to_string())?;
        assert!(!run(&fixture, &invocation, b"")?.status.success());
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("gh-lookups"))
                .map_err(|e| e.to_string())?
                .lines()
                .count(),
            1
        );
        std::fs::write(
            fixture.0.join("chosen.pub"),
            format!("{public} fixture-private-comment\n"),
        )
        .map_err(|e| e.to_string())?;
        tool(
            &fixture,
            "tea",
            r#"[ -z "${GITEA_SERVER_TOKEN:-}" ] || exit 93
case "$1" in
 login) printf 'fixture-login https://forge.example fixture-user\n';;
 api) printf '{"login":"fixture-user"}\n';;
 ssh-keys) for argument in "$@"; do last=$argument; done; cat "$last" > "$FIXTURE_ROOT/tea-public-payload";;
 *) exit 94;;
esac"#,
        )?;
        let invocation = plan::remote_registration(
            &machine(),
            "~/chosen",
            "forge.example",
            None,
            Some("fixture-login"),
            "fixture title",
        )?;
        assert!(!run(&fixture, &invocation, b"CANCEL\n")?.status.success());
        assert!(!fixture.0.join("tea-public-payload").exists());
        assert!(run(&fixture, &invocation, b"UPLOAD\n")?.status.success());
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("tea-public-payload"))
                .map_err(|e| e.to_string())?,
            format!("{public}\n")
        );
        assert_eq!(
            std::fs::read_dir(fixture.0.join("tmp"))
                .map_err(|e| e.to_string())?
                .count(),
            0
        );
        Ok(())
    }
}

#[test]
fn opening_setup_ui_exposes_each_step_without_running_tools_or_changing_profiles()
-> Result<(), String> {
    use egui_kittest::{Harness, kittest::Queryable};
    Context::default().run(|| {
        let fixture = Fixture::new()?;
        let ctx = egui::Context::default();
        let mut state = fixture.state(&ctx)?;
        state.folder = fixture.0.join("new key").to_string_lossy().into_owned();
        state.key = fixture.0.join("selected").to_string_lossy().into_owned();
        let profile = Settings::get(&ctx).profile();
        let harness = Harness::builder()
            .with_size(egui::vec2(1200., 1400.))
            .build_ui_state(|ui, state: &mut State| state.fields(ui), state);
        for label in [
            "1. Create ed25519 key",
            "2. Add selected key",
            "3. Upload public key",
            "4. Test with ssh -T",
        ] {
            assert!(harness.query_by_label_contains(label).is_some());
        }
        assert!(
            harness.state().receiver.is_none()
                && harness.state().command.is_none()
                && harness.state().account.is_none()
        );
        assert!(!fixture.0.join("new key").exists());
        assert_eq!(Settings::get(&ctx).profile(), profile);
        Ok(())
    })
}

#[test]
fn invalid_locations_and_unselected_local_upload_paths_refuse_before_any_io() -> Result<(), String>
{
    for path in [
        "ssh://-bad-host/repo",
        "ssh:missing-host",
        "wsl://-bad/repo",
        "wsl:///repo",
        "https://unsupported.example/repo",
    ] {
        assert!(Machine::of(std::path::Path::new(path)).is_err());
    }
    let remote = Machine::of(std::path::Path::new("ssh://fixture-host/~/repo"))?;
    assert!(!remote.is_local());
    let fixture = Fixture::new()?;
    let local = Machine::of(&fixture.0)?;
    for key in [
        "",
        "relative-key",
        "../other-key",
        "ssh://host/key",
        "/fixture/key.pub",
    ] {
        assert!(public_companion(&local, key).is_err());
    }
    let selected = fixture.0.join("chosen-key").to_string_lossy().into_owned();
    assert_eq!(
        public_companion(&local, &selected)?,
        PathBuf::from(format!("{selected}.pub"))
    );
    assert!(public_companion(&remote, "~/chosen-key").is_err());
    assert_eq!(fixture.0.read_dir().map_err(|e| e.to_string())?.count(), 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn ssh_setup_terminal_prompt_remains_visible_at_minimum_window_height() -> Result<(), String> {
    Context::default().run(|| {
        let fixture = Fixture::new()?;
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let mut tab = RepoTab::new_deferred(fixture.0.clone(), &ctx);
        tab.open_ssh_setup(&ctx);
        let state = tab.ssh_setup.as_mut().ok_or("No setup view")?;
        let terminal = Terminal::spawn(
            &fixture.0,
            &crate::terminal::ShellSpec {
                program: "/bin/sh".into(),
                args: vec!["-c".into(), "printf fixture-prompt".into()],
                label: "owned recording prompt".into(),
            },
            &ctx,
        )?;
        state.command = Some(CommandRun {
            terminal,
            step: Step::Generate,
            finished: true,
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 500.),
                    )),
                    ..Default::default()
                },
                |ui| tab.ssh_setup_window(ui.ctx()),
            );
            let visible = out.shapes.iter().any(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().contains("fixture-prompt") => {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    rect.min.y >= 0. && rect.max.y <= 500. && rect.intersects(shape.clip_rect)
                }
                _ => false,
            });
            out.drop_without_applying_deltas();
            if visible {
                break;
            }
            if std::time::Instant::now() >= until {
                return Err(
                    "Owned setup prompt was clipped at the supported800x500 viewport".into(),
                );
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        Ok(())
    })
}

#[cfg(windows)]
#[test]
fn failed_open_wsl_unc_cannot_construct_a_local_setup_view() -> Result<(), String> {
    Context::default().run(|| {
        let fixture = Fixture::new()?;
        let ctx = egui::Context::default();
        for path in [
            r"\\wsl.localhost\-bad\repo",
            r"\\wsl$\\repo",
            r"\\?\UNC\WSL.LOCALHOST\\repo",
            "//?/unc/wsl$/-bad/repo",
        ] {
            // Start the existing watcher only on our owned local fixture, then model
            // the failed-open path. No UNC filesystem lookup or WSL process is needed.
            let mut tab = RepoTab::new_deferred(fixture.0.clone(), &ctx);
            tab.path = PathBuf::from(path);
            tab.load_error = Some("fixture failed-open repository".into());
            assert!(!tab.opening());
            tab.apply(super::super::Action::OpenSshSetup, &ctx);
            assert!(tab.ssh_setup.is_none() && tab.terminals.is_empty());
            assert!(
                tab.status
                    .as_ref()
                    .is_some_and(|(text, failed)| *failed && text.contains("machine is invalid"))
            );
        }
        Ok(())
    })
}
