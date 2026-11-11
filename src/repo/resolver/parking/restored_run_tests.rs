use super::*;
#[test]
fn selecting_restored_id_preserves_the_other_parked_run() -> Result<(), String> {
    let repo = crate::git::fixture::Repo::new("resolver-switch-parked")?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.path().to_path_buf(), &ctx);
    let directory = repo.path().join("saved-agent");
    std::fs::create_dir_all(directory.join("resolved")).map_err(|e| e.to_string())?;
    std::fs::write(
        directory.join("workspace.json"),
        r#"{"files":[],"sides":{}}"#,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(directory.join("run.json"), "original record").map_err(|e| e.to_string())?;
    let mut previous = super::super::look_tests::run(Phase::Ready, Instant::now());
    previous.id = "previous".into();
    previous.parked = Some(Parked {
        id: "previous".into(),
        ..Default::default()
    });
    previous.workspace = Some(Arc::new(Workspace::load(&directory)?));
    tab.resolver.run = Some(previous);
    let restored = || {
        let mut run = super::super::look_tests::run(Phase::Ready, Instant::now());
        run.id = "restored".into();
        run.results.clear();
        run
    };
    tab.select_restored_run("wrong-id", Some(restored()), &ctx);
    assert_eq!(
        tab.resolver.run.as_ref().map(|r| r.id.as_str()),
        Some("previous")
    );
    tab.select_restored_run("restored", None, &ctx);
    assert_eq!(
        tab.resolver.run.as_ref().map(|r| r.id.as_str()),
        Some("previous")
    );
    tab.select_restored_run("restored", Some(restored()), &ctx);
    assert_eq!(
        tab.resolver.run.as_ref().map(|r| r.id.as_str()),
        Some("restored")
    );
    assert!(Workspace::load(&directory).is_ok());
    assert_eq!(
        std::fs::read_to_string(directory.join("run.json")).map_err(|e| e.to_string())?,
        "original record"
    );
    Ok(())
}
