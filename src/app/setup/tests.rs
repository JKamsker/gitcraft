use super::*;
use egui_kittest::kittest::Queryable as _;
use std::time::{Duration, Instant};
fn wait(h: &mut egui_kittest::Harness<'_, App>) -> Result<(), String> {
    let end = Instant::now() + Duration::from_secs(20);
    while h.state().setup.busy() {
        if Instant::now() >= end {
            return Err("Setup did not settle".into());
        }
        h.step();
        std::thread::sleep(Duration::from_millis(5));
    }
    h.run_steps(3);
    Ok(())
}
// Previews can exceed the modal's viewport. Exercise the real pointer route after
// scrolling the target into view rather than clicking an off-screen AccessKit rectangle.
fn click(h: &mut egui_kittest::Harness<'_, App>, label: &str) {
    h.get_by_label(label).scroll_to_me();
    // The centered modal and its shrinking scroll viewport settle together. Force
    // their layout frames, then sample the current pointer target for each event.
    h.run_steps(30);
    for _ in 0..4 {
        let pos = h.get_by_label(label).rect().center();
        h.event(egui::Event::PointerMoved(pos));
        h.step();
    }
    for pressed in [true, false] {
        let pos = h.get_by_label(label).rect().center();
        h.event(egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        h.step();
    }
    h.run_steps(3);
}

#[test]
fn export_import_ui_round_trip_preserves_active_profile_and_requires_local_setup()
-> Result<(), String> {
    // A real App activation intentionally updates the process-global profile context.
    // Keep its synthetic signing policy out of unrelated parallel Git fixtures.
    const CHILD: &str = "GITCRAFT_SETUP_ROUND_TRIP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output=std::process::Command::new(std::env::current_exe().map_err(|e|e.to_string())?).args(["--exact","app::setup::tests::export_import_ui_round_trip_preserves_active_profile_and_requires_local_setup","--nocapture"]).env(CHILD,"1").output().map_err(|e|e.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "Isolated setup UI fixture failed: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        return Ok(());
    }
    let repo = crate::practice::create(crate::practice::Scenario::Published)?;
    let root = repo.parent().ok_or("root")?.to_owned();
    crate::git::practice_run(
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            "ssh://git@work-alias/team/repository.git",
        ],
    )?;
    let selected_key = root.join("fresh-selected-key");
    std::fs::write(
        &selected_key,
        "Synthetic selected key path; no private bytes",
    )
    .map_err(|e| e.to_string())?;
    let mut h = egui_kittest::Harness::builder()
        .with_size([1200.0, 1200.0])
        .build_eframe(|cc| {
            let mut a = App::new(cc, None);
            a.settings.check_updates = false;
            a.settings.user_profiles[0].own_signing = true;
            a.settings.user_profiles[0].signing.sign_commits = true;
            a.settings.user_profiles[0].signing.key = "OLD_SECRET_KEY".into();
            a.settings.set(&cc.egui_ctx);
            a.recent = vec![repo.clone()];
            a
        });
    h.get_by_label("Export setup…").click();
    h.run_steps(3);
    assert!(h.state().setup.open);
    h.state_mut().setup.candidates[0].selected = true;
    h.get_by_label("Build export preview").click();
    h.run_steps(3);
    wait(&mut h)?;
    let file = h.state().setup.file.clone().ok_or("No export preview")?;
    assert_eq!(file.repositories.len(), 1);
    assert!(
        file.profiles[0].setup.ssh_key_reselection_required,
        "SSH repository bindings must not fall back to an ambient agent"
    );
    assert!(file.repositories[0].remotes[0].fetch.contains("work-alias"));
    let path = root.join("portable.json");
    files::save(&path, &file, &|| true)?;
    assert!(files::save(&path, &file, &|| true).is_err());
    let decoded = files::load(&path, &|| true)?;
    assert!(
        !std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())?
            .contains("OLD_SECRET_KEY")
    );
    let active = h.state().settings.active_profile;
    h.state_mut().setup.set_file(decoded);
    h.state_mut().setup.import = true;
    h.run_steps(3);
    click(&mut h, "Add profiles, authors and public settings");
    assert_eq!(h.state().settings.active_profile, active);
    let id = *h
        .state()
        .setup
        .mappings
        .get(&active)
        .ok_or_else(||format!("No profile mapping: applied={}, active={}, mappings={:?}, profiles={:?}, messages={:?}",h.state().setup.applied,active,h.state().setup.mappings,h.state().setup.file.as_ref().map(|f|f.profiles.iter().map(|p|p.source_id).collect::<Vec<_>>()),h.state().setup.messages))?;
    assert!(
        h.state()
            .settings
            .user_profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or("No imported profile")?
            .pending_setup()
    );
    let ctx = h.ctx.clone();
    assert!(!h.state_mut().switch_profile(id, &ctx));
    h.state_mut().setup.fresh.entry(id).or_default().signing = "new-local-signing-id".into();
    h.state_mut().setup.fresh.entry(id).or_default().ssh =
        selected_key.to_string_lossy().into_owned();
    click(&mut h, "Verify required setup and enable profile");
    wait(&mut h)?;
    assert!(
        !h.state()
            .settings
            .user_profiles
            .iter()
            .find(|p| p.id == id)
            .ok_or("Missing profile")?
            .pending_setup()
    );
    assert_eq!(h.state().settings.active_profile, active);
    click(&mut h, "Activate Default");
    assert_eq!(h.state().settings.active_profile, id);
    let parent = root.join("ui-restores");
    std::fs::create_dir(&parent).map_err(|e| e.to_string())?;
    {
        let plan = &mut h.state_mut().setup.plans[0];
        plan.selected = true;
        plan.parent = parent;
        plan.source = Some(repo.clone());
        plan.remotes[0].fetch = "ssh://git@expected.fixture.invalid/team/repository.git".into();
    }
    assert!(
        !h.state().setup.plans[0].binding_ready(&h.state().settings.profile()),
        "Alias editing still requires explicit confirmation before restoration"
    );
    h.run_steps(3);
    click(
        &mut h,
        "I confirmed these SSH addresses use literal hostnames, not unexported SSH aliases",
    );
    assert!(h.state().setup.plans[0].binding_ready(&h.state().settings.profile()));
    h.run_steps(3);
    click(&mut h, "Clone and restore selected setup");
    wait(&mut h)?;
    assert!(
        h.state().setup.plans[0].complete,
        "{:?}",
        h.state().setup.plans[0].messages
    );
    let restored = &h.state().setup.plans[0]
        .receipt
        .as_ref()
        .ok_or("No UI restore receipt")?
        .root;
    assert!(restored.join(".git").is_dir());
    let remote = crate::git::setup_run(restored, &["remote", "get-url", "origin"], &|| true)?;
    assert_eq!(
        String::from_utf8_lossy(&remote).trim(),
        "ssh://git@expected.fixture.invalid/team/repository.git"
    );
    drop(h);
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
#[test]
fn cancelled_profile_answers_cannot_activate_stale_setup() {
    let mut window = SetupWindow::default();
    let (tx, rx) = channel();
    let active = Arc::new(AtomicBool::new(false));
    window.job = Some(Job {
        rx,
        active,
        cancel: Default::default(),
    });
    let p = UserProfile::default();
    assert!(
        tx.send(Ok(Answer::Verified(Box::new((p.clone(), p)))))
            .is_ok()
    );
    assert!(window.take_answer().is_some_and(|r| r.is_err()));
}
#[test]
fn malicious_setup_file_cannot_restore_commands_paths_or_credential_urls() -> Result<(), String> {
    let file = Setup::new(&crate::settings::Settings::default(), &Default::default());
    let mut value = serde_json::to_value(&file).map_err(|e| e.to_string())?;
    value["profiles"][0]["connections"] =
        serde_json::json!({"github":{"config_dir":"/secret/auth"}});
    assert!(Setup::decode(&serde_json::to_vec(&value).map_err(|e| e.to_string())?).is_err());
    Ok(())
}

#[test]
fn changed_profile_answers_do_not_clear_the_current_pending_requirements() {
    let mut h = egui_kittest::Harness::builder()
        .with_size([1000.0, 1000.0])
        .build_eframe(|cc| {
            let mut a = App::new(cc, None);
            a.settings.check_updates = false;
            a
        });
    let mut before = UserProfile {
        id: 77,
        name: "Imported".into(),
        ..Default::default()
    };
    before.pending_setup = Some(portability::ConnectionRequirements {
        ssh_key_reselection_required: true,
        ..Default::default()
    });
    let mut changed = before.clone();
    changed.author_email = "changed@test.invalid".into();
    h.state_mut().settings.user_profiles.push(changed.clone());
    let mut ready = before.clone();
    ready.pending_setup = None;
    let (tx, rx) = channel();
    assert!(
        tx.send(Ok(Answer::Verified(Box::new((before, ready)))))
            .is_ok()
    );
    h.state_mut().setup.job = Some(Job {
        rx,
        active: Arc::new(AtomicBool::new(true)),
        cancel: Default::default(),
    });
    let ctx = h.ctx.clone();
    h.state_mut().poll_setup(&ctx);
    assert_eq!(h.state().settings.user_profiles.last(), Some(&changed));
}

#[test]
fn deleted_imported_profile_mappings_are_not_reused_for_another_profiles_id() {
    let mut setup = SetupWindow::default();
    setup.mappings.insert(12, 77);
    setup.forget_profile(77);
    assert!(!setup.mappings.values().any(|id| *id == 77));
}
