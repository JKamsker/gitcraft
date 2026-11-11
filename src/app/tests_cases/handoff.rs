/// What a later start hands over opens as the command line does: a subfolder opens its
/// repository's tab, handed over again it switches to that tab, and "come to the front" leaves
/// the tabs as they are.
#[test]
fn a_folder_handed_over_opens_its_repositorys_tab_once() -> Result<(), String> {
    use crate::instance::Request;

    let root = std::env::temp_dir().join(format!(
        "gitcraft-handoff-{}-{}",
        std::process::id(),
        crate::remote::askpass::random_token()
    ));
    let sub = root.join("src");
    std::fs::create_dir_all(&sub).map_err(|e| e.to_string())?;
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .map_err(|e| e.to_string())?;
    assert!(init.success());
    let expected = super::repo_root(&root)?;

    let mut harness = egui_kittest::Harness::builder()
        .with_size([1200.0, 800.0])
        .build_eframe(|cc| super::App::new(cc, None));
    harness.step();
    let ctx = harness.ctx.clone();
    let repo_tabs = |app: &super::App| -> Vec<std::path::PathBuf> {
        app.tabs
            .iter()
            .filter_map(|t| match t {
                super::Tab::Repo(r) => Some(r.path.clone()),
                _ => None,
            })
            .collect()
    };

    harness
        .state_mut()
        .handle_handoff(Request::Open(sub.clone()), &ctx);
    harness.step();
    assert_eq!(repo_tabs(harness.state()), vec![expected.clone()]);

    harness.state_mut().new_tab();
    harness
        .state_mut()
        .handle_handoff(Request::Open(root.clone()), &ctx);
    harness.state_mut().handle_handoff(Request::Focus, &ctx);
    harness.step();
    let state = harness.state();
    assert_eq!(repo_tabs(state), vec![expected.clone()]);
    assert!(
        matches!(state.tabs.get(state.active), Some(super::Tab::Repo(r)) if r.path == expected)
    );
    drop(harness);
    let _ = std::fs::remove_dir_all(&root);
    Ok(())
}

/// When each recent repository was last opened survives a restart; a save from before it
/// knew of them still loads, with none.
#[test]
fn last_opened_times_survive_a_restart() {
    let mut saved = Saved {
        recent: vec!["/home/me/Größe repo".into()],
        ..Saved::default()
    };
    saved
        .last_opened
        .insert("/home/me/Größe repo".into(), 1_700_000_000);
    let mut storage = Memory::default();
    eframe::set_value(&mut storage, eframe::APP_KEY, &saved);
    let back: Option<Saved> = eframe::get_value(&storage, eframe::APP_KEY);
    assert_eq!(
        back.and_then(|b| b
            .last_opened
            .get(std::path::Path::new("/home/me/Größe repo"))
            .copied()),
        Some(1_700_000_000)
    );
    let mut old = Memory::default();
    old.set_string(eframe::APP_KEY, "(recent: [\"C:/a\"])".to_owned());
    let back: Option<Saved> = eframe::get_value(&old, eframe::APP_KEY);
    let back = back.unwrap_or_default();
    assert_eq!(back.recent, [std::path::PathBuf::from("C:/a")]);
    assert!(back.last_opened.is_empty());
}
