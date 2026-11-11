use super::suggest_folder;
use std::path::{Path, PathBuf};

fn form(branch: &str, folder: &str, repo: &str) -> super::StartForm {
    super::StartForm {
        task: String::new(),
        branch: branch.to_owned(),
        branch_edited: false,
        folder: folder.to_owned(),
        folder_edited: false,
        base: "main".to_owned(),
        agent: 0,
        custom: String::new(),
        give_prompt: true,
        hooks: false,
        focus: false,
        repo: PathBuf::from(repo),
        branches: vec!["main".to_owned(), "agent/x".to_owned()],
        folder_for: Box::new(|_| String::new()),
    }
}

#[test]
fn the_dialog_says_what_keeps_it_from_starting() {
    let ok = form("agent/new", "/w/app-new", "/w/app");
    assert_eq!(ok.problem(&Ok("/w/app-new".to_owned()), "claude"), None);
    let taken = form("agent/x", "/w/app-x", "/w/app");
    let said = taken
        .problem(&Ok(String::new()), "claude")
        .unwrap_or_default();
    assert!(said.contains("exists already"), "{said}");
    assert!(
        form("", "/w", "/w/app")
            .problem(&Ok(String::new()), "claude")
            .is_some()
    );
    let no_agent = form("agent/new", "/w/app-new", "/w/app");
    assert!(
        no_agent
            .problem(&Ok(String::new()), "")
            .is_some_and(|p| p.contains("command"))
    );
    let home = form("agent/new", "~/app-new", "ssh://h/home/me/app");
    assert!(
        home.problem(&Ok("~/app-new".to_owned()), "claude")
            .is_some_and(|p| p.contains("full path"))
    );
}

#[test]
fn a_task_folder_uses_the_root_and_is_never_taken() {
    let main = Path::new("ssh://build/home/me/app.wt");
    assert_eq!(
        suggest_folder(main, "agent/fix ü", &[]),
        "/home/me/app.wt/agent/fix-ü"
    );
    let taken = [PathBuf::from("ssh://build/home/me/app.wt/agent/x")];
    assert_eq!(
        suggest_folder(main, "agent/x", &taken),
        "/home/me/app.wt/agent/x-2"
    );
}

/// Agent tasks use the configured root when opened from a linked worktree.
#[test]
fn a_task_form_uses_the_saved_root_of_the_main_checkout() {
    let ctx = egui::Context::default();
    let mut tab = crate::repo::RepoTab::new_deferred("ssh://build/elsewhere/linked".into(), &ctx);
    tab.worktrees = vec![crate::git::Worktree {
        path: "ssh://build/home/me/app".into(),
        main: true,
        ..Default::default()
    }];
    tab.worktree_root = "custom.worktrees".to_owned();
    let form = super::StartForm::new(&tab);
    assert_eq!(
        (form.folder_for)("feat/awesome-stuff"),
        "/home/me/app/custom.worktrees/feat/awesome-stuff"
    );
}
