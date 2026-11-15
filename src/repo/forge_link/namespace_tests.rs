use super::*;

#[test]
fn sidebar_remote_named_refs_retains_heads_branch_path() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred("/recording/233-refs-remote".into(), &ctx);
    tab.remotes = vec![Remote {
        name: "refs".into(),
        url: "https://github.com/alice/repo".into(),
    }];
    tab.refs.local.push(crate::git::Branch {
        name: "topic".into(),
        target: "tip".into(),
        is_head: false,
        upstream: String::new(),
        track: String::new(),
    });
    let full = crate::repo::branch_visibility::remote_ref("refs/heads/topic");
    let (menu, target) = tab
        .branch_link_menu(&full)
        .ok_or("remote branch link missing")?;
    assert_eq!(menu.choices[0].remote, "refs");
    assert!(matches!(target, Target::Branch { name } if name == "heads/topic"));
    Ok(())
}
