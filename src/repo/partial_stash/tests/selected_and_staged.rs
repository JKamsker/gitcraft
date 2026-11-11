#[test]
fn picked_files_are_stashed_and_the_others_stay() -> Result<(), String> {
    let repo = Repo::new("files")?;
    repo.write("a.txt", b"a changed, not picked\n")?;
    repo.write("sp ace.txt", b"space changed\n")?;
    repo.write(UNICODE, b"unicode changed\n")?;
    repo.write("bin.dat", &[9, 0, 9, 0, 255])?;
    let _ = std::fs::remove_file(repo.path().join("del.txt"));
    repo.write(NEW, b"new\n")?;
    repo.write("other new.txt", b"not picked\n")?;
    repo.stage_in_part()?;
    let before = repo.state()?;
    let picked: Vec<String> = ["sp ace.txt", UNICODE, "bin.dat", "del.txt", NEW, "part.txt"]
        .map(str::to_owned)
        .to_vec();
    // Picked twice (staged and unstaged rows of one file) is one path.
    let mut twice = picked.clone();
    twice.push("part.txt".to_owned());
    let (result, kind) = repo.stash(StashMode::Files(twice), false);
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(repo.stashes(), 1);
    let left = repo.status()?;
    let mut changed: Vec<&str> = left.unstaged.iter().map(|f| f.path.as_str()).collect();
    changed.sort_unstable();
    assert_eq!(changed, ["a.txt", "other new.txt"]);
    assert!(left.staged.is_empty());
    assert_eq!(repo.read("part.txt"), PART.as_bytes());
    assert_eq!(repo.read("bin.dat"), [0, 1, 2, 3, 255, 0, 7]);
    let stashed = repo.git(&[
        "-c",
        "core.quotePath=false",
        "stash",
        "show",
        "--include-untracked",
        "--name-only",
        "stash@{0}",
    ])?;
    for p in &picked {
        assert!(stashed.lines().any(|l| l == p), "{p} not in {stashed}");
    }
    // Undone: every byte back, the staging of part.txt too; redone and undone again.
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.stashes(), 0);
    let redone = repo.redo(undone)?;
    assert_eq!(repo.stashes(), 1);
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn a_staged_rename_or_deletion_is_refused_before_anything_happens() -> Result<(), String> {
    let repo = Repo::new("files-refused")?;
    repo.git(&["mv", "old.txt", "new.txt"])?;
    repo.git(&["rm", "-q", "del.txt"])?;
    repo.write("a.txt", b"changed\n")?;
    let before = repo.state()?;
    let work = repo.status()?;
    let rename = files_problem(&["new.txt".to_owned()], &work).unwrap_or_default();
    assert!(rename.contains("rename"), "{rename}");
    let deletion = files_problem(&["del.txt".to_owned()], &work).unwrap_or_default();
    assert!(deletion.contains("deletion"), "{deletion}");
    for path in ["new.txt", "del.txt"] {
        let (result, _) = repo.stash(
            StashMode::Files(vec![path.to_owned(), "a.txt".to_owned()]),
            false,
        );
        assert!(result.is_err());
    }
    assert!(
        files_problem(&["a.txt".to_owned()], &work)
            .is_some_and(|p| p.contains("staged outside the selection"))
    );
    assert_eq!(repo.stashes(), 0);
    assert_eq!(repo.state()?, before);
    Ok(())
}

/// A rename made in the working tree (not staged) is a deletion and a new file: picked, both go.
#[test]
fn an_unstaged_rename_is_stashed_as_its_two_files() -> Result<(), String> {
    let repo = Repo::new("files-rename")?;
    std::fs::rename(
        repo.path().join("old.txt"),
        repo.path().join("re named.txt"),
    )
    .map_err(|e| e.to_string())?;
    let before = repo.state()?;
    let files = vec!["old.txt".to_owned(), "re named.txt".to_owned()];
    let (result, kind) = repo.stash(StashMode::Files(files), false);
    assert!(result.is_ok(), "{result:?}");
    assert!(repo.status()?.is_clean());
    assert_eq!(repo.read("old.txt"), b"old\n");
    repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn files_with_no_changes_left_stash_nothing() -> Result<(), String> {
    let repo = Repo::new("files-nothing")?;
    let picked = vec!["a.txt".to_owned()];
    // Picked while changed, back as it was by the time it runs.
    let work = {
        repo.write("a.txt", b"x\n")?;
        let w = repo.status()?;
        repo.write("a.txt", b"a\n")?;
        w
    };
    let request = StashRequest {
        mode: StashMode::Files(picked),
        message: String::new(),
        untracked: false,
    };
    let args = push_args(&request, &work).map_err(|e| e.to_string())?;
    let (result, kind) = run_push(repo.path(), args, git::head(repo.path()), false);
    assert_eq!(
        result.err().as_deref(),
        Some("there are no changes to stash")
    );
    assert!(matches!(kind, OpKind::Normal(Undo::None, _)));
    assert_eq!(repo.stashes(), 0);
    // Not changed at all: refused before git is asked.
    let (result, _) = repo.stash(StashMode::Files(vec!["a.txt".to_owned()]), false);
    assert!(result.is_err_and(|e| e.contains("no changes")));
    Ok(())
}

#[test]
fn only_what_is_staged_is_stashed_and_comes_back_on_undo() -> Result<(), String> {
    let repo = Repo::new("staged")?;
    if git::staged_stash_support(repo.path()) != Ok(Ok(())) {
        return Ok(());
    }
    repo.stage_in_part()?;
    repo.git(&["mv", "old.txt", "new name ü.txt"])?;
    repo.git(&["rm", "-q", "del.txt"])?;
    repo.write(NEW, b"staged new\n")?;
    repo.git(&["add", "--", NEW])?;
    repo.write("sp ace.txt", b"unstaged only\n")?;
    repo.write("untracked.txt", b"stays\n")?;
    let before = repo.state()?;
    let (result, kind) = repo.stash(StashMode::Staged, true);
    assert!(result.is_ok(), "{result:?}");
    let left = repo.status()?;
    assert!(left.staged.is_empty(), "{:?}", left.staged);
    // The unstaged changes stay: the end of part.txt, sp ace.txt, the untracked file.
    assert_eq!(
        repo.read("part.txt"),
        PART.replace("seven", "SEVEN").as_bytes()
    );
    assert_eq!(repo.read("sp ace.txt"), b"unstaged only\n");
    assert_eq!(repo.read("untracked.txt"), b"stays\n");
    assert_eq!(repo.read("old.txt"), b"old\n");
    assert_eq!(repo.read("del.txt"), b"del\n");
    // Undone with the unstaged changes still there: everything as it was, staging too.
    let undone = repo.undo(entry(kind)?)?;
    assert_eq!(repo.state()?, before);
    assert_eq!(repo.stashes(), 0);
    let redone = repo.redo(undone)?;
    assert_eq!(repo.stashes(), 1);
    assert!(repo.status()?.staged.is_empty());
    repo.undo(redone)?;
    assert_eq!(repo.state()?, before);
    Ok(())
}

#[test]
fn a_staged_binary_file_that_git_cant_take_out_leaves_no_stash() -> Result<(), String> {
    let repo = Repo::new("staged-binary")?;
    if git::staged_stash_support(repo.path()) != Ok(Ok(())) {
        return Ok(());
    }
    repo.write("bin.dat", &[5, 0, 5, 0, 5])?;
    repo.git(&["add", "bin.dat"])?;
    repo.write("a.txt", b"unstaged\n")?;
    let before = repo.state()?;
    let (result, kind) = repo.stash(StashMode::Staged, false);
    match result {
        // Newer gits may manage it: then it is an ordinary stash.
        Ok(_) => {
            repo.undo(entry(kind)?)?;
            assert_eq!(repo.state()?, before);
        }
        Err(e) => {
            assert!(e.starts_with("nothing was stashed"), "{e}");
            assert_eq!(repo.stashes(), 0);
            assert_eq!(repo.state()?, before);
        }
    }
    Ok(())
}

#[test]
fn nothing_staged_is_refused() -> Result<(), String> {
    let repo = Repo::new("staged-nothing")?;
    repo.write("a.txt", b"unstaged\n")?;
    let (result, _) = repo.stash(StashMode::Staged, false);
    assert_eq!(result.err().as_deref(), Some("nothing is staged"));
    // Unstaged since it was looked at: git says so, and it is said plainly.
    let work = {
        repo.git(&["add", "a.txt"])?;
        let w = repo.status()?;
        repo.git(&["reset", "-q"])?;
        w
    };
    let request = StashRequest {
        mode: StashMode::Staged,
        message: String::new(),
        untracked: false,
    };
    if git::staged_stash_support(repo.path()) == Ok(Ok(())) {
        let args = push_args(&request, &work).map_err(|e| e.to_string())?;
        let (result, _) = run_push(repo.path(), args, git::head(repo.path()), true);
        assert_eq!(result.err().as_deref(), Some("nothing is staged"));
    }
    assert_eq!(repo.stashes(), 0);
    Ok(())
}
