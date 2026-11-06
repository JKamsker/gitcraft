//! The Unity support through the code paths the views use, on real repositories.

use super::smart_merge::{is_unity_yaml, sh_tool, version_key};
use super::{UnityAct, banner::describe};
use crate::git::{self, MetaKind, MetaProblem};
use crate::repo::{Action, RepoTab};

#[test]
fn unity_yaml_files() {
    for yes in [
        "Assets/Main.unity",
        "a/Hero.PREFAB",
        "x.asset",
        "y.meta",
        "z.controller",
    ] {
        assert!(is_unity_yaml(yes), "{yes}");
    }
    for no in ["a.png", "a.cs", "unity", "Makefile", "a.unity.bak"] {
        assert!(!is_unity_yaml(no), "{no}");
    }
}

#[test]
fn editor_versions_sort_by_their_numbers() {
    let mut v = vec!["2022.3.10f1", "2022.3.9f1", "6000.0.1f1", "2021.3.30f1"];
    v.sort_by_key(|s| version_key(s));
    assert_eq!(
        v,
        ["2021.3.30f1", "2022.3.9f1", "2022.3.10f1", "6000.0.1f1"]
    );
}

#[test]
fn the_tool_quoted_for_sh() {
    let t = std::path::Path::new("C:\\Program Files\\Unity\\Hub\\it's\\UnityYAMLMerge.exe");
    assert_eq!(
        sh_tool(t),
        "'C:/Program Files/Unity/Hub/it'\\''s/UnityYAMLMerge.exe'"
    );
}

#[test]
fn meta_problems_in_words() {
    let p = |kind| MetaProblem {
        path: "Assets/a.png".into(),
        partner: "Assets/a.png.meta".into(),
        kind,
    };
    assert_eq!(
        describe(&p(MetaKind::AddedWithoutMeta)),
        "Assets/a.png is staged without Assets/a.png.meta"
    );
    assert_eq!(
        describe(&p(MetaKind::DeletedWithoutMeta)),
        "Assets/a.png is deleted but Assets/a.png.meta stays"
    );
}

fn wait(tab: &mut RepoTab, ctx: &egui::Context, done: impl Fn(&RepoTab) -> bool) {
    for _ in 0..400 {
        tab.poll(ctx);
        tab.unity_tick(ctx);
        if done(tab) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

fn idle(tab: &RepoTab) -> bool {
    tab.unity.busy.is_none() && tab.busy_op.is_none() && !tab.unity.checking
}

/// A Unity project at the root: found, checked, fixed from the panel's actions; an asset staged
/// without its `.meta` flagged, and the `.meta` staged from the banner's action.
#[test]
fn unity_in_the_tab() -> Result<(), String> {
    let dir = git::unity_tests::unity_repo("tab", "")?;
    git::run(Some(&dir), &["commit", "-q", "--allow-empty", "-m", "base"])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| matches!(t.unity.setup, Some(Ok(_))));
    assert_eq!(tab.unity.project().map(|p| p.root.as_str()), Some(""));
    assert!(matches!(&tab.unity.setup, Some(Ok(s)) if s.force_text == Some(false)));

    tab.apply(Action::Unity(UnityAct::OpenPanel), &ctx);
    wait(&mut tab, &ctx, |t| matches!(t.unity.setup, Some(Ok(_))));
    tab.apply(Action::Unity(UnityAct::AddAttributes), &ctx);
    wait(&mut tab, &ctx, |t| {
        idle(t) && matches!(t.unity.setup, Some(Ok(_)))
    });
    tab.apply(Action::Unity(UnityAct::AddIgnores), &ctx);
    wait(&mut tab, &ctx, |t| {
        idle(t) && matches!(&t.unity.setup, Some(Ok(s)) if s.unignored.is_empty())
    });
    let setup = match &tab.unity.setup {
        Some(Ok(s)) => s.clone(),
        other => return Err(format!("not checked: {other:?}")),
    };
    assert!(setup.missing_attributes.is_empty());
    assert!(setup.unignored.is_empty());
    assert!(!setup.all_good(), "Force Text is the editor's to set");

    // An asset staged alone; its .meta left unstaged.
    std::fs::write(dir.join("Assets/Hero Ü.png"), "p").map_err(|e| e.to_string())?;
    std::fs::write(dir.join("Assets/Hero Ü.png.meta"), "guid: 1").map_err(|e| e.to_string())?;
    git::run(
        Some(&dir),
        &["--literal-pathspecs", "add", "--", "Assets/Hero Ü.png"],
    )?;
    tab.reload_status(&ctx);
    wait(&mut tab, &ctx, |t| !t.unity.meta.is_empty());
    let partners: Vec<String> = tab.unity.meta.iter().map(|m| m.partner.clone()).collect();
    assert_eq!(partners, ["Assets/Hero Ü.png.meta"]);
    let stage = tab.stage_partners_action().ok_or("nothing to stage")?;
    tab.apply(stage, &ctx);
    wait(&mut tab, &ctx, |t| {
        t.busy_op.is_none() && t.unity.meta.is_empty() && !t.unity.meta_running
    });
    assert!(tab.unity.meta.is_empty());
    let staged = git::run(Some(&dir), &["diff", "--cached", "--name-only", "-z"])?;
    assert!(staged.split('\0').any(|p| p == "Assets/Hero Ü.png.meta"));
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

/// A conflicted scene solved by "UnityYAMLMerge" (a stand-in script taking their version) into
/// the conflict editor; the driver registered in the repository's config.
#[cfg(unix)]
#[test]
fn smart_merge_in_the_conflict_editor() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let dir = git::unity_tests::unity_repo("merge", "")?;
    let tool = dir.with_extension("tools").join("UnityYAMLMerge");
    let _ = std::fs::create_dir_all(tool.parent().ok_or("no folder")?);
    // merge -p BASE THEIRS OURS MERGED: their version, as Smart Merge might choose.
    std::fs::write(&tool, "#!/bin/sh\ncp \"$4\" \"$6\"\n").map_err(|e| e.to_string())?;
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| e.to_string())?;
    git::run(
        Some(&dir),
        &[
            "config",
            super::smart_merge::TOOL_KEY,
            &tool.to_string_lossy(),
        ],
    )?;
    let scene = "Assets/Main Scene.unity";
    let write = |t: &str| std::fs::write(dir.join(scene), t).map_err(|e| e.to_string());
    write("a: 1\n")?;
    git::run(Some(&dir), &["add", "-A"])?;
    git::run(Some(&dir), &["commit", "-qm", "base"])?;
    git::run(Some(&dir), &["checkout", "-qb", "other"])?;
    write("a: 2\n")?;
    git::run(Some(&dir), &["commit", "-qam", "theirs"])?;
    git::run(Some(&dir), &["checkout", "-q", "-"])?;
    write("a: 3\n")?;
    git::run(Some(&dir), &["commit", "-qam", "ours"])?;
    assert!(git::run(Some(&dir), &["merge", "other"]).is_err());

    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.unity.project().is_some() && t.unity.tool.is_some()
    });
    assert_eq!(tab.unity.tool.as_deref(), Some(tool.as_path()));
    tab.conflict =
        crate::repo::conflict::ConflictEditor::open(&dir, scene, Some(git::InProgress::Merge));
    assert!(tab.unity.offers_smart_merge(scene));
    tab.apply(Action::Unity(UnityAct::SolveConflict), &ctx);
    wait(&mut tab, &ctx, |t| {
        t.conflict.as_ref().is_some_and(|c| !c.solving)
    });
    let result = tab.conflict.as_ref().map(|c| c.result.clone());
    assert_eq!(result.as_deref(), Some("a: 2\n"), "{:?}", tab.status);

    tab.apply(Action::Unity(UnityAct::RegisterDriver), &ctx);
    wait(&mut tab, &ctx, |t| idle(t) && t.unity.driver.is_some());
    let driver = tab.unity.driver.clone().unwrap_or_default();
    assert!(
        driver.ends_with("UnityYAMLMerge' merge -p %O %B %A %A"),
        "{driver}"
    );
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(tool.parent().ok_or("no folder")?);
    Ok(())
}

/// A repository without a Unity project offers nothing.
#[test]
fn no_unity_project_nothing_offered() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-no-unity-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["init", "-q"])?;
    git::run(
        Some(&dir),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ],
    )?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new(dir.clone(), &ctx);
    wait(&mut tab, &ctx, |t| t.unity.detected_for.is_some());
    let _ = std::fs::remove_dir_all(&dir);
    assert!(tab.unity.project().is_none());
    assert!(!tab.unity.offers_smart_merge("Assets/a.unity"));
    Ok(())
}
