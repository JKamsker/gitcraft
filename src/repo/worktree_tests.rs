//! Worktree roots, creation through the operation worker, and local excludes.

use crate::git::{self, fixture::Repo};
use crate::git::{
    WORKTREE_ROOT_KEY, branch_worktree_folder, default_worktree_root, read_worktree_root,
    resolve_worktree_root,
};
use crate::remote;
use crate::repo::{OpKind, operations::run_steps, ops};
use std::path::{Path, PathBuf};

/// Create through the normal operation worker so preparation and Git both run.
fn add(repo: &Path, folder: &Path, branch: &str) -> Result<(), String> {
    let folder = remote::git_path(folder);
    run_steps(
        repo,
        &ops(&[&["worktree", "add", "-b", branch, "--", &folder, "HEAD"]]),
        OpKind::plain(),
        None,
    )
    .0
    .map(|_| ())
}

#[test]
fn worktree_roots_preserve_branch_namespaces_and_cannot_be_escaped() -> Result<(), String> {
    let main = Path::new("/code/gitcraft");
    let root = default_worktree_root(main);
    assert_eq!(root, Path::new("/code/gitcraft.wt"));
    assert_eq!(
        branch_worktree_folder(&root, "feat/awesome-stuff"),
        root.join("feat").join("awesome-stuff")
    );
    assert_eq!(
        resolve_worktree_root(main, ".worktrees")?,
        main.join(".worktrees")
    );
    assert_eq!(
        resolve_worktree_root(main, "../custom.wt")?,
        main.join("../custom.wt")
    );
    for branch in ["../../escape", "/absolute", "C:/escape", "..\\escape", ""] {
        let path = branch_worktree_folder(&root, branch);
        assert!(path.starts_with(&root), "{branch}: {path:?}");
        assert!(
            !path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        );
    }
    let ssh = Path::new("ssh://build/home/me/gitcraft");
    let root = resolve_worktree_root(ssh, "/srv/worktrees")?;
    assert_eq!(
        branch_worktree_folder(&root, "feat/awesome-stuff"),
        Path::new("ssh://build/srv/worktrees/feat/awesome-stuff")
    );
    assert_eq!(
        default_worktree_root(ssh),
        Path::new("ssh://build/home/me/gitcraft.wt")
    );
    assert!(resolve_worktree_root(main, "ssh://build/srv/worktrees").is_err());
    assert!(resolve_worktree_root(ssh, "ssh://other/srv/worktrees").is_err());
    Ok(())
}

#[test]
fn worktree_root_and_local_ignore_are_shared_from_linked_checkouts() -> Result<(), String> {
    let repo = Repo::new("worktree-root-shared")?;
    repo.commit("base", &[])?;
    let exclude = repo.dir.join(".git/info/exclude");
    std::fs::write(&exclude, "# Existing rule\n/build").map_err(|e| e.to_string())?;
    // Enable per-worktree config to ensure our root still lives in the common config.
    repo.git(&["config", "extensions.worktreeConfig", "true"])?;
    repo.git(&[
        "config",
        "--local",
        WORKTREE_ROOT_KEY,
        "./.worktrees [local]",
    ])?;
    let root = resolve_worktree_root(repo.path(), &read_worktree_root(repo.path())?)?;
    let first = branch_worktree_folder(&root, "feat/awesome-stuff");
    add(repo.path(), &first, "feat/awesome-stuff")?;
    assert_eq!(read_worktree_root(&first)?, "./.worktrees [local]");
    let second = branch_worktree_folder(&root, "fix/from-linked");
    add(&first, &second, "fix/from-linked")?;
    assert!(second.join(".git").is_file());
    let text = std::fs::read_to_string(&exclude).map_err(|e| e.to_string())?;
    assert!(text.starts_with("# Existing rule\n/build\n"));
    assert_eq!(
        text.matches("/.worktrees\\ \\[local\\]/feat/awesome-stuff/")
            .count(),
        1,
        "{text}"
    );
    assert!(
        repo.git(&["status", "--porcelain", "--untracked-files=all"])?
            .is_empty()
    );
    assert!(!repo.dir.join(".gitignore").exists());
    Ok(())
}

#[test]
fn external_worktree_root_needs_no_ignore_changes() -> Result<(), String> {
    let repo = Repo::new("worktree-root-external")?;
    repo.commit("base", &[])?;
    let root = default_worktree_root(repo.path());
    let _cleanup = Repo { dir: root.clone() };
    let exclude = repo.dir.join(".git/info/exclude");
    let before = std::fs::read(&exclude).map_err(|e| e.to_string())?;
    let folder = branch_worktree_folder(&root, "feat/nested");
    add(repo.path(), &folder, "feat/nested")?;
    assert!(folder.join(".git").is_file());
    assert_eq!(std::fs::read(&exclude).map_err(|e| e.to_string())?, before);
    Ok(())
}

/// A valid destination remains usable even if the stored preference cannot be resolved.
#[test]
fn worktree_creation_uses_a_valid_destination_with_an_invalid_saved_root() -> Result<(), String> {
    let repo = Repo::new("worktree-root-invalid")?;
    repo.commit("base", &[])?;
    repo.git(&["config", "--local", WORKTREE_ROOT_KEY, "~/worktrees"])?;
    let root = default_worktree_root(repo.path());
    let _cleanup = Repo { dir: root.clone() };
    let folder = branch_worktree_folder(&root, "feat/fallback");
    add(repo.path(), &folder, "feat/fallback")?;
    assert!(folder.join(".git").is_file());
    assert_eq!(read_worktree_root(repo.path())?, "~/worktrees");
    Ok(())
}

/// Worktree rules must not hide unrelated files under an existing project directory.
#[test]
fn worktree_creation_keeps_project_files_visible_and_stageable() -> Result<(), String> {
    let repo = Repo::new("worktree-root-project-files")?;
    repo.commit("base", &[("src/tracked.rs", Some("tracked"))])?;
    repo.git(&["config", "--local", WORKTREE_ROOT_KEY, "src"])?;
    std::fs::write(repo.dir.join("src/new.rs"), "new").map_err(|e| e.to_string())?;
    let folder = repo.dir.join("src/feat/task");
    add(repo.path(), &folder, "feat/task")?;
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=all"])?,
        "?? src/new.rs\n"
    );
    repo.git(&["add", "--all"])?;
    assert_eq!(
        repo.git(&["diff", "--cached", "--name-only"])?,
        "src/new.rs\n"
    );
    Ok(())
}

/// Git accepts byte patterns that cannot be decoded as UTF-8; adding a rule preserves them.
#[test]
fn worktree_creation_preserves_non_utf8_exclude_rules() -> Result<(), String> {
    let repo = Repo::new("worktree-root-exclude-bytes")?;
    repo.commit("base", &[])?;
    let exclude = repo.dir.join(".git/info/exclude");
    let original = b"# Existing rules\r\nbad\xffname";
    std::fs::write(&exclude, original).map_err(|e| e.to_string())?;
    let folder = repo.dir.join(".worktrees/feat/task");
    add(repo.path(), &folder, "feat/task")?;
    let bytes = std::fs::read(&exclude).map_err(|e| e.to_string())?;
    assert!(bytes.starts_with(original));
    assert!(bytes.ends_with(b"/.worktrees/feat/task/\n"));
    git::prepare_worktree_folder(repo.path(), &folder.to_string_lossy())?;
    assert_eq!(std::fs::read(&exclude).map_err(|e| e.to_string())?, bytes);
    Ok(())
}

/// Appending after another editor saved keeps the current bytes, here and over SSH.
#[test]
fn worktree_ignore_append_preserves_changes_since_the_read() -> Result<(), String> {
    let repo = Repo::new("worktree-root-append")?;
    let file = repo.dir.join("exclude");
    let paths = vec![file.clone()];
    #[cfg(unix)]
    let paths = {
        let mut paths = paths;
        paths.push(remote::encode(
            &remote::Host::Ssh(remote::LOOPBACK.to_owned()),
            &file.to_string_lossy(),
        ));
        paths
    };
    for path in paths {
        std::fs::write(&file, b"original\n").map_err(|e| e.to_string())?;
        let snapshot = remote::read(&path).map_err(|e| e.to_string())?;
        assert_eq!(snapshot, b"original\n");
        std::fs::write(&file, b"original\nnew-rule\xff").map_err(|e| e.to_string())?;
        remote::append(&path, b"\n/worktree/\n").map_err(|e| e.to_string())?;
        assert_eq!(
            std::fs::read(&file).map_err(|e| e.to_string())?,
            b"original\nnew-rule\xff\n/worktree/\n"
        );
    }
    Ok(())
}

#[test]
fn worktree_dialog_uses_the_main_root_and_preserves_a_folder_override() -> Result<(), String> {
    use crate::repo::{Dialog, RepoTab};
    let ctx = egui::Context::default();
    let main = PathBuf::from("ssh://build/home/me/gitcraft");
    let linked = PathBuf::from("ssh://build/srv/worktrees/old");
    let mut tab = RepoTab::new_deferred(linked.clone(), &ctx);
    tab.worktrees = vec![
        git::Worktree {
            path: main.clone(),
            main: true,
            ..Default::default()
        },
        git::Worktree {
            path: linked.clone(),
            current: true,
            ..Default::default()
        },
    ];
    tab.worktree_root = ".worktrees".to_owned();
    tab.open_dialog(
        Dialog::add_worktree(&linked, Some("feat/awesome-stuff"), "HEAD"),
        &ctx,
    );
    match tab.dialog.as_ref() {
        Some(Dialog::AddWorktree {
            repo, root, path, ..
        }) => {
            assert_eq!(repo, &main);
            assert_eq!(root, ".worktrees");
            assert_eq!(path, "/home/me/gitcraft/.worktrees/feat/awesome-stuff");
        }
        _ => return Err("Worktree dialog was not opened".to_owned()),
    }
    let mut dialog = Dialog::add_worktree(&linked, Some("feat/override"), "HEAD");
    if let Dialog::AddWorktree {
        path, path_edited, ..
    } = &mut dialog
    {
        *path = "/srv/special-folder".to_owned();
        *path_edited = true;
    }
    tab.open_dialog(dialog, &ctx);
    assert!(
        matches!(&tab.dialog, Some(Dialog::AddWorktree { path, .. }) if path == "/srv/special-folder")
    );
    Ok(())
}

#[test]
fn worktree_dialog_saves_the_chosen_root_and_creates_the_branch_folder() -> Result<(), String> {
    use crate::repo::{Action, Dialog, RepoTab};
    use egui_kittest::kittest::Queryable;
    let repo = Repo::new("worktree-root-dialog")?;
    repo.commit("base", &[])?;
    repo.git(&["branch", "feat/awesome-stuff"])?;
    let ctx = egui::Context::default();
    let mut tab = RepoTab::new_deferred(repo.dir.clone(), &ctx);
    tab.refs = git::refs(repo.path())?;
    tab.worktrees = git::worktree_list(repo.path(), None)?;
    tab.open_dialog(
        Dialog::add_worktree(repo.path(), Some("feat/awesome-stuff"), "HEAD"),
        &ctx,
    );
    let actions = std::cell::RefCell::new(Vec::new());
    let mut harness = egui_kittest::Harness::builder()
        .with_size([700.0, 900.0])
        .build_ui(|ui| {
            if ui.ctx().cumulative_pass_nr() == 0 {
                crate::theme::install_fonts(ui.ctx());
            } else {
                tab.dialogs(ui.ctx(), &mut actions.borrow_mut());
            }
        });
    harness.run();
    harness.get_by_label("Inside repository").click();
    harness.run();
    harness.get_by_label("Add").click();
    harness.run();
    drop(harness);
    let actions = actions.into_inner();
    let [Action::Ops { ops, .. }] = actions.as_slice() else {
        return Err("The dialog did not submit the worktree operation".to_owned());
    };
    run_steps(repo.path(), ops, OpKind::plain(), None).0?;
    assert_eq!(read_worktree_root(repo.path())?, ".worktrees");
    assert!(
        repo.dir
            .join(".worktrees/feat/awesome-stuff/.git")
            .is_file()
    );
    assert!(
        repo.git(&["status", "--porcelain", "--untracked-files=all"])?
            .is_empty()
    );
    Ok(())
}

#[test]
fn worktree_root_at_checkout_never_ignores_the_entire_repository() -> Result<(), String> {
    let repo = Repo::new("worktree-root-checkout")?;
    repo.commit("base", &[])?;
    repo.git(&["config", "--local", WORKTREE_ROOT_KEY, "."])?;
    let folder = branch_worktree_folder(repo.path(), "feat/inside");
    add(repo.path(), &folder, "feat/inside")?;
    std::fs::write(repo.dir.join("visible.txt"), "visible").map_err(|e| e.to_string())?;
    assert_eq!(
        repo.git(&["status", "--porcelain", "--untracked-files=all"])?,
        "?? visible.txt\n"
    );
    Ok(())
}

#[test]
fn worktree_creation_explains_an_ignore_override_before_creating_the_checkout() -> Result<(), String>
{
    let repo = Repo::new("worktree-root-negated")?;
    repo.commit(
        "ignore",
        &[(".gitignore", Some("!.worktrees/feat/blocked/\n"))],
    )?;
    repo.git(&["config", "--local", WORKTREE_ROOT_KEY, ".worktrees"])?;
    let folder = repo.dir.join(".worktrees/feat/blocked");
    let error = add(repo.path(), &folder, "feat/blocked")
        .err()
        .ok_or("Expected an ignore error")?;
    assert!(error.contains(".gitignore"), "{error}");
    assert!(!folder.join(".git").exists());
    assert!(
        repo.git(&["show-ref", "--verify", "refs/heads/feat/blocked"])
            .is_err()
    );
    Ok(())
}

#[test]
fn worktree_creation_tolerates_an_unavailable_locked_worktree() -> Result<(), String> {
    let repo = Repo::new("worktree-root-missing")?;
    repo.commit("base", &[])?;
    let missing = repo.dir.join("old");
    repo.git(&[
        "worktree",
        "add",
        "--lock",
        "-b",
        "old",
        &missing.to_string_lossy(),
    ])?;
    std::fs::remove_dir_all(&missing).map_err(|e| e.to_string())?;
    repo.git(&["config", "--local", WORKTREE_ROOT_KEY, ".worktrees"])?;
    let folder = repo.dir.join(".worktrees/feat/new");
    add(repo.path(), &folder, "feat/new")?;
    assert!(folder.join(".git").is_file());
    Ok(())
}

#[cfg(unix)]
#[test]
fn remote_worktree_root_uses_its_own_machine_and_local_exclude() -> Result<(), String> {
    let repo = Repo::new("worktree-root-remote")?;
    repo.commit("base", &[])?;
    let remote = remote::encode(
        &remote::Host::Ssh(remote::LOOPBACK.to_owned()),
        &repo.dir.to_string_lossy(),
    );
    git::run(
        Some(&remote),
        &["config", "--local", WORKTREE_ROOT_KEY, ".worktrees 'local'"],
    )?;
    let root = resolve_worktree_root(&remote, &read_worktree_root(&remote)?)?;
    let folder = branch_worktree_folder(&root, "feat/awesome-stuff");
    add(&remote, &folder, "feat/awesome-stuff")?;
    assert!(
        repo.dir
            .join(".worktrees 'local'/feat/awesome-stuff/.git")
            .is_file()
    );
    assert!(
        repo.git(&["status", "--porcelain", "--untracked-files=all"])?
            .is_empty()
    );
    Ok(())
}
