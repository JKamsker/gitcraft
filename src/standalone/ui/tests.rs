use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use std::path::{Path, PathBuf};
fn fixture() -> Result<(crate::git::file_comparison::Scratch, [PathBuf; 4]), String> {
    let temp = crate::git::file_comparison::Scratch::new()?;
    let paths = ["base", "local", "remote", "merged"].map(|n| temp.0.join(n));
    for (path, bytes) in paths.iter().zip([
        b"base\n".as_slice(),
        b"local\n",
        b"remote\n",
        b"existing result\n",
    ]) {
        std::fs::write(path, bytes).map_err(|e| e.to_string())?;
    }
    Ok((temp, paths))
}
fn harness(invocation: Invocation) -> (Harness<'static, ToolApp>, Arc<AtomicU8>) {
    let code = Arc::new(AtomicU8::new(1));
    let result = Arc::clone(&code);
    let h = Harness::builder()
        .with_size([1400., 950.])
        .build_eframe(move |cc| ToolApp::new(invocation, result, &cc.egui_ctx));
    (h, code)
}
fn settle(h: &mut Harness<'_, ToolApp>) -> Result<(), String> {
    // Windows metadata preservation starts a bounded native helper per snapshot.
    let seconds = if cfg!(windows) { 120 } else { 10 };
    let until = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    loop {
        h.run_steps(1);
        if h.state().loading.is_none() && h.state().saving.is_none() {
            return Ok(());
        }
        if std::time::Instant::now() > until {
            return Err("Standalone UI did not settle".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
fn close_window(h: &mut Harness<'_, ToolApp>) {
    h.input_mut()
        .viewports
        .entry(egui::ViewportId::ROOT)
        .or_default()
        .events
        .push(egui::ViewportEvent::Close);
    h.run_steps(1);
}
#[test]
fn actual_diff_close_succeeds_but_unsaved_merge_close_cancels_and_load_error_is_distinct()
-> Result<(), String> {
    let (_temp, paths) = fixture()?;
    let before = std::fs::read(&paths[3]).map_err(|e| e.to_string())?;
    let (mut diff, diff_code) = harness(Invocation::Diff([paths[0].clone(), paths[1].clone()]));
    settle(&mut diff)?;
    assert!(diff.query_by_label_contains("-base").is_some());
    assert!(diff.query_by_label_contains("+local").is_some());
    close_window(&mut diff);
    assert_eq!(diff_code.load(Ordering::Acquire), 0);
    let (mut merge, merge_code) = harness(Invocation::Merge(paths.clone()));
    settle(&mut merge)?;
    merge.get_by_label("Use Local file").click();
    merge.run_steps(1);
    close_window(&mut merge);
    assert_eq!(merge_code.load(Ordering::Acquire), 1);
    assert_eq!(std::fs::read(&paths[3]).map_err(|e| e.to_string())?, before);
    let missing = paths[0].with_file_name("does-not-exist");
    let (mut failed, failed_code) = harness(Invocation::Diff([missing, paths[1].clone()]));
    settle(&mut failed)?;
    assert!(failed.state().error.is_some());
    close_window(&mut failed);
    assert_eq!(failed_code.load(Ordering::Acquire), 2);
    Ok(())
}
#[test]
fn actual_result_editor_and_conflict_choice_save_exact_bytes_with_success() -> Result<(), String> {
    let (_temp, paths) = fixture()?;
    let (mut h, code) = harness(Invocation::Merge(paths.clone()));
    settle(&mut h)?;
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
        .focus();
    h.run_steps(1);
    h.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
    h.run_steps(1);
    h.get_by_role(egui::accesskit::Role::MultilineTextInput)
        .type_text("manual result\nlast");
    h.run_steps(1);
    h.get_by_label("Save and finish").click();
    settle(&mut h)?;
    assert_eq!(code.load(Ordering::Acquire), 0);
    assert_eq!(
        std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
        b"manual result\nlast"
    );
    assert_eq!(
        std::fs::read(&paths[1]).map_err(|e| e.to_string())?,
        b"local\n"
    );
    drop(h);
    std::fs::write(
        &paths[3],
        b"<<<<<<< LOCAL\nlocal\n||||||| BASE\nbase\n=======\nremote\n>>>>>>> REMOTE\n",
    )
    .map_err(|e| e.to_string())?;
    let (mut h, code) = harness(Invocation::Merge(paths.clone()));
    settle(&mut h)?;
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Local")
        .click();
    h.run_steps(1);
    h.get_by_label("Save and finish").click();
    settle(&mut h)?;
    assert_eq!(code.load(Ordering::Acquire), 0);
    assert_eq!(
        std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
        b"local\n"
    );
    Ok(())
}
#[test]
fn actual_save_refuses_changed_destination_and_reports_nonzero_without_overwrite()
-> Result<(), String> {
    let (_temp, paths) = fixture()?;
    let (mut h, code) = harness(Invocation::Merge(paths.clone()));
    settle(&mut h)?;
    h.get_by_label("Use Local file").click();
    h.run_steps(1);
    std::fs::write(&paths[3], b"concurrent owner bytes").map_err(|e| e.to_string())?;
    h.get_by_label("Save and finish").click();
    settle(&mut h)?;
    assert_eq!(code.load(Ordering::Acquire), 2);
    assert!(!h.state().finished);
    assert_eq!(
        std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
        b"concurrent owner bytes"
    );
    h.get_by_label("Close with error").click();
    h.run_steps(1);
    assert_eq!(code.load(Ordering::Acquire), 2);
    Ok(())
}
#[test]
fn hostile_profile_cannot_redirect_the_standalone_background_ui() -> Result<(), String> {
    const CHILD: &str = "GITCRAFT_STANDALONE_PROFILE_FIXTURE";
    const NAME: &str =
        "standalone::ui::tests::hostile_profile_cannot_redirect_the_standalone_background_ui";
    if std::env::var_os(CHILD).is_some() {
        crate::connections::set(crate::connections::Context {
            git_config: Some("nonexistent-profile-config".into()),
            ..Default::default()
        });
        let (_temp, paths) = fixture()?;
        let (mut h, code) = harness(Invocation::Merge(paths.clone()));
        settle(&mut h)?;
        assert!(h.state().error.is_none());
        h.get_by_label("Use Remote file").click();
        h.run_steps(1);
        h.get_by_label("Save and finish").click();
        settle(&mut h)?;
        assert_eq!(code.load(Ordering::Acquire), 0);
        assert_eq!(
            std::fs::read(&paths[3]).map_err(|e| e.to_string())?,
            b"remote\n"
        );
        return Ok(());
    }
    let out = std::process::Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
        .args(["--exact", NAME, "--nocapture"])
        .env(CHILD, "1")
        .env("GIT_DIR", Path::new("nonexistent-caller-repository"))
        .output()
        .map_err(|e| e.to_string())?;
    assert!(
        out.status.success(),
        "{} {}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}
