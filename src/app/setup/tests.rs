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
#[test]
fn export_import_ui_round_trip_preserves_active_profile_and_requires_local_setup()
-> Result<(), String> {
    let repo = crate::practice::create(crate::practice::Scenario::Published)?;
    let root = repo.parent().ok_or("root")?.to_owned();
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
    h.get_by_label("Add profiles, authors and public settings")
        .click();
    h.run_steps(3);
    assert_eq!(h.state().settings.active_profile, active);
    let id = *h
        .state()
        .setup
        .mappings
        .get(&active)
        .ok_or("No profile mapping")?;
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
    h.get_by_label("Verify required setup and enable profile")
        .click();
    h.run_steps(3);
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
    h.get_by_label("Activate Default").click();
    h.run_steps(3);
    assert_eq!(h.state().settings.active_profile, id);
    let parent = root.join("ui-restores");
    std::fs::create_dir(&parent).map_err(|e| e.to_string())?;
    {
        let plan = &mut h.state_mut().setup.plans[0];
        plan.selected = true;
        plan.parent = parent;
        plan.source = Some(repo.clone());
    }
    h.run_steps(3);
    h.get_by_label("Clone and restore selected setup").click();
    h.run_steps(3);
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
    drop(h);
    std::fs::remove_dir_all(root).map_err(|e| e.to_string())?;
    Ok(())
}
#[test]
fn cancelled_profile_answers_cannot_activate_stale_setup() {
    let mut window = SetupWindow::default();
    let (tx, rx) = channel();
    let active = Arc::new(AtomicBool::new(false));
    window.job = Some(Job { rx, active });
    let p = UserProfile::default();
    tx.send(Ok(Answer::Verified(p.clone(), p))).unwrap();
    assert!(window.take_answer().unwrap().is_err());
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
    tx.send(Ok(Answer::Verified(before, ready))).unwrap();
    h.state_mut().setup.job = Some(Job {
        rx,
        active: Arc::new(AtomicBool::new(true)),
    });
    let ctx = h.ctx.clone();
    h.state_mut().poll_setup(&ctx);
    assert_eq!(h.state().settings.user_profiles.last(), Some(&changed));
}
