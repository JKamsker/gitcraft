use super::*;
use crate::git::Branch;

fn branch(name: &str, is_head: bool) -> Branch {
    Branch {
        name: name.to_owned(),
        target: String::new(),
        is_head,
        upstream: String::new(),
        track: String::new(),
    }
}

fn refs() -> Refs {
    Refs {
        local: vec![branch("main", true), branch("feat/a", false)],
        remotes: vec![(
            "origin".to_owned(),
            vec![branch("origin/main", false), branch("origin/feat/b", false)],
        )],
        head: "abc".to_owned(),
        current: Some("main".to_owned()),
        ..Refs::default()
    }
}

#[test]
fn nothing_hidden_logs_every_branch() {
    let v = Visibility::default();
    assert_eq!(v.revisions(&refs()), None);
    assert_eq!(v.counts(&refs()), (4, 4));
}

#[test]
fn hidden_branches_are_left_out_of_the_log() {
    let mut v = Visibility::default();
    v.toggle("refs/remotes/origin/feat/b");
    assert_eq!(
        v.revisions(&refs()),
        Some(vec![
            "HEAD".to_owned(),
            "refs/heads/feat/a".to_owned(),
            "refs/remotes/origin/main".to_owned(),
        ])
    );
    assert_eq!(v.counts(&refs()), (3, 4));
    v.toggle("refs/remotes/origin/feat/b");
    assert_eq!(v.revisions(&refs()), None);
}

#[test]
fn soloed_branches_are_the_only_ones_logged_besides_head() {
    let mut v = Visibility::default();
    v.toggle("refs/heads/feat/a");
    v.toggle_solo("refs/remotes/origin/main");
    assert_eq!(
        v.revisions(&refs()),
        Some(vec![
            "HEAD".to_owned(),
            "refs/remotes/origin/main".to_owned()
        ])
    );
    assert_eq!(v.counts(&refs()), (2, 4));
    // While soloing, the eye solos another branch too.
    v.toggle("refs/heads/feat/a");
    assert!(v.shows("refs/heads/feat/a"));
    assert_eq!(v.counts(&refs()), (3, 4));
    // Unsoloing all of them brings back the hidden ones' state.
    v.toggle_solo("refs/heads/feat/a");
    v.toggle_solo("refs/remotes/origin/main");
    assert!(!v.shows("refs/heads/feat/a"));
    assert!(v.shows("refs/remotes/origin/main"));
}

#[test]
fn the_checked_out_branch_is_logged_through_head() {
    let mut v = Visibility::default();
    v.toggle("refs/heads/main");
    assert_eq!(v.counts(&refs()), (4, 4));
    let revs = v.revisions(&refs()).unwrap_or_default();
    assert!(!revs.contains(&"refs/heads/main".to_owned()));
    assert_eq!(revs[0], "HEAD");
}

#[test]
fn hiding_everything_on_an_unborn_branch_logs_nothing() {
    let mut v = Visibility::default();
    v.toggle_solo("refs/heads/gone");
    let unborn = Refs::default();
    assert_eq!(v.revisions(&unborn), Some(Vec::new()));
}

#[test]
fn deleted_branches_are_forgotten() {
    let mut v = Visibility::default();
    v.toggle("refs/heads/feat/a");
    v.toggle("refs/heads/gone");
    v.toggle_solo("refs/remotes/origin/gone");
    assert!(v.prune(&refs()));
    assert_eq!(v.hidden.len(), 1);
    assert!(!v.is_soloing());
    assert!(!v.prune(&refs()));
}
#[test]
fn remote_groups_compose_with_individual_show_hide_solo_and_local_head() {
    let mut refs = refs();
    refs.remotes.push((
        "fork".into(),
        vec![branch("fork/main", false), branch("fork/topic", false)],
    ));
    let mut v = Visibility::default();
    v.prune(&refs);
    v.toggle_remote_graph("origin");
    v.toggle_remote_graph("fork");
    v.hidden.insert(local_ref("main"));
    assert_eq!(v.counts(&refs), (2, 6));
    assert_eq!(
        v.revisions(&refs),
        Some(vec!["HEAD".into(), local_ref("feat/a")])
    );
    v.toggle(&remote_ref("fork/topic"));
    assert!(
        v.shows(&remote_ref("fork/topic")),
        "individual Show overrides group"
    );
    assert!(!v.shows(&remote_ref("origin/main")));
    v.toggle_solo(&remote_ref("origin/main"));
    assert!(v.shows(&remote_ref("origin/main")), "Solo overrides group");
    assert!(!v.shows(&remote_ref("fork/topic")));
    assert_eq!(
        v.revisions(&refs),
        Some(vec!["HEAD".into(), remote_ref("origin/main")])
    );
    v.toggle_solo(&remote_ref("origin/main"));
    assert!(v.shows(&remote_ref("fork/topic")));
    v.sidebar_remotes.insert("fork".into());
    v.show_all();
    assert!(!v.is_filtering());
    assert!(
        v.sidebar_remotes.contains("fork"),
        "graph Show all does not change sidebar choices"
    );
}

#[test]
fn a_remote_group_uses_the_longest_remote_name_and_keeps_detached_head() {
    let mut refs = refs();
    refs.current = None;
    refs.local.clear();
    refs.remotes
        .push(("my".into(), vec![branch("my/topic", false)]));
    refs.remotes
        .push(("my/fork".into(), vec![branch("my/fork/topic", false)]));
    let mut v = Visibility::default();
    v.prune(&refs);
    v.toggle_remote_graph("my");
    assert!(!v.shows(&remote_ref("my/topic")));
    assert!(v.shows(&remote_ref("my/fork/topic")));
    let mut deserialized = v.clone();
    deserialized.remote_names.clear();
    assert!(
        deserialized
            .revisions(&refs)
            .unwrap_or_default()
            .contains(&remote_ref("my/fork/topic")),
        "background roots must recognize nested remote names after restart"
    );
    v.toggle_remote_graph("my/fork");
    v.toggle_remote_graph("origin");
    assert_eq!(v.revisions(&refs), Some(vec!["HEAD".into()]));
    v.toggle(&remote_ref("my/fork/topic"));
    assert!(v.shows(&remote_ref("my/fork/topic")));
    v.toggle_remote_graph("my");
    v.toggle_remote_graph("my");
    assert!(
        v.shows(&remote_ref("my/fork/topic")),
        "a parent-named remote must not erase another remote's Show override"
    );
}

#[test]
fn sidebar_and_graph_group_choices_persist_independently() -> Result<(), String> {
    let ctx = egui::Context::default();
    let path = Path::new("/repo/hidden-remotes");
    let mut v = Visibility::default();
    v.sidebar_remotes.insert("origin".into());
    v.graph_remotes.insert("fork".into());
    v.store(&ctx, path);
    let saved = ctx.memory(ron::to_string).map_err(|e| e.to_string())?;
    let restored = egui::Context::default();
    restored.memory_mut(|m| *m = ron::from_str(&saved).unwrap_or_default());
    assert_eq!(Visibility::load(&restored, path), v);
    assert_eq!(
        Visibility::load(&restored, Path::new("/other")),
        Visibility::default()
    );
    Ok(())
}

#[test]
fn hiding_remote_history_keeps_local_head_ancestry_without_modifying_refs() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-hidden-head-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let run = |args: &[&str]| crate::git::run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &["remote", "add", "origin", "https://github.com/up/repo"],
        &["remote", "add", "fork", "https://github.com/me/repo"],
        &["checkout", "-q", "-b", "temporary"],
        &["commit", "-q", "--allow-empty", "-m", "remote-only"],
        &["update-ref", "refs/remotes/fork/topic", "HEAD"],
        &["checkout", "-q", "main"],
        &["branch", "-D", "temporary"],
        &["commit", "-q", "--allow-empty", "-m", "local-head"],
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    ] {
        run(args)?;
    }
    let refs_before = run(&["show-ref", "--head"])?;
    let config_before = std::fs::read(dir.join(".git/config")).map_err(|e| e.to_string())?;
    let refs = crate::git::refs(&dir)?;
    let mut v = Visibility::default();
    v.prune(&refs);
    v.toggle_remote_graph("origin");
    v.toggle_remote_graph("fork");
    v.hidden.insert(local_ref("main"));
    let revisions = v.revisions(&refs);
    let history = crate::git::log(&dir, revisions.as_deref(), Default::default(), 0, 100)?;
    assert_eq!(
        history
            .iter()
            .map(|c| c.summary.as_str())
            .collect::<Vec<_>>(),
        ["local-head", "base"]
    );
    assert_eq!(run(&["show-ref", "--head"])?, refs_before);
    assert_eq!(
        std::fs::read(dir.join(".git/config")).map_err(|e| e.to_string())?,
        config_before
    );
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
