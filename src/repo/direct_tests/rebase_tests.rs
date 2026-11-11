use super::*;

fn entry(hash: &str, step: Step) -> Entry {
    Entry {
        commit: RebaseCommit {
            hash: hash.to_owned(),
            summary: format!("summary {hash}"),
            message: format!("summary {hash}"),
            ..RebaseCommit::default()
        },
        step,
        message: format!("summary {hash}"),
    }
}

/// Rebasing the top of a stack with Update dependent branches moves the layers below
/// along; one checked out in another worktree stays (git's `--update-refs` skips it).
#[test]
fn update_dependent_branches_moves_the_lower_layers() -> Result<(), String> {
    let repo = crate::git::stack::test_repo::TestRepo::new("rebase update refs")?;
    repo.stack(&["schema", "api", "ui"])?;
    repo.commit("m2.txt", "2\n", "main moves on")?;
    let wt = repo.dir.with_file_name(format!(
        "{} schema wt",
        repo.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&wt);
    let schema = repo.tip("schema")?;
    repo.checkout("ui")?;
    let result = (|| -> Result<(), String> {
        let commits = crate::git::rebase_commits(repo.path(), "main")?;
        let mut editor = RebaseEditor::new("main".into(), "main".into(), commits);
        editor.update_refs = true;
        // schema checked out in a worktree of its own: it stays.
        repo.git(&["worktree", "add", "-q", &wt.to_string_lossy(), "schema"])?;
        let command = editor.command(repo.path())?;
        let args: Vec<&str> = command.iter().map(String::as_str).collect();
        assert!(args.contains(&"--update-refs"));
        repo.git(&args)?;
        let is_ancestor =
            |a: &str, b: &str| repo.git(&["merge-base", "--is-ancestor", a, b]).is_ok();
        assert!(is_ancestor("main", "ui") && is_ancestor("main", "api"));
        assert_eq!(repo.tip("schema")?, schema);
        Ok(())
    })();
    let _ = repo.git(&["worktree", "remove", "--force", &wt.to_string_lossy()]);
    let _ = std::fs::remove_dir_all(&wt);
    result
}

#[test]
fn todo_lines() {
    let entries = vec![
        entry("a1", Step::Pick),
        entry("b2", Step::Reword),
        entry("c3", Step::Squash),
        entry("d4", Step::Drop),
    ];
    // api on b2, which c3 is squashed into: placed after the squash. ui on the last one.
    let refs = [
        ("b2".to_owned(), "refs/heads/api".to_owned()),
        ("d4".to_owned(), "refs/heads/ui".to_owned()),
    ];
    let todo = todo(
        &entries,
        |i| (i == 1).then(|| "'/tmp/m 1.txt'".to_owned()),
        &refs,
    );
    assert_eq!(
        todo,
        "pick a1 summary a1\npick b2 summary b2\nexec git commit --amend --allow-empty --cleanup=whitespace -F '/tmp/m 1.txt'\nsquash c3 summary c3\nupdate-ref refs/heads/api\ndrop d4 summary d4\nupdate-ref refs/heads/ui\n"
    );
}

#[test]
fn problems() {
    let mut ed = RebaseEditor::new("x".into(), "x".into(), Vec::new());
    ed.entries = vec![entry("a1", Step::Drop), entry("b2", Step::Fixup)];
    assert!(ed.problem().is_some_and(|p| p.contains("below")));
    ed.entries[1].step = Step::Pick;
    assert_eq!(ed.problem(), None);
    ed.entries[1].step = Step::Drop;
    assert!(ed.problem().is_some());
    ed.entries[1].step = Step::Reword;
    ed.entries[1].message = "  ".into();
    assert!(ed.problem().is_some());
}

#[test]
fn a_branch_moved_since_the_read_is_not_rebased() {
    let commits = ["a1", "b2"].map(|h| entry(h, Step::Pick).commit).to_vec();
    let mut ed = RebaseEditor::new("x".into(), "x".into(), commits);
    assert_eq!(ed.moved("b2"), None);
    // Reordering doesn't change which commit was HEAD.
    ed.entries.swap(0, 1);
    assert_eq!(ed.moved("b2"), None);
    // A commit made since: its todo list would drop it.
    assert!(ed.moved("c3").is_some());
}

#[test]
fn private_dirs_are_new() {
    let parent = std::env::temp_dir().join(format!("gitcraft-test-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&parent);
    let a = private_dir(&parent);
    let b = private_dir(&parent);
    assert!(a.is_ok() && b.is_ok());
    if let (Ok(a), Ok(b)) = (&a, &b) {
        assert_ne!(a, b);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(a).map(|m| m.permissions().mode() & 0o777);
            assert_eq!(mode.ok(), Some(0o700));
        }
    }
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn shell_paths() {
    assert_eq!(
        sh_path(Path::new(r"C:\Temp\it's\todo.txt")),
        r"'C:/Temp/it'\''s/todo.txt'"
    );
}
