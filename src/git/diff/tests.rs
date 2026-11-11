use std::process::Command;

use super::*;

/// Old patches are cleaned up, but not this run's: undoing a discard of lines a day ago
/// applies its patch, the only copy of those lines.
#[test]
fn old_patches_of_this_run_are_kept() -> Result<(), String> {
    let text = |e: std::io::Error| e.to_string();
    let first = save_patch("x\n")?;
    let dir = first.parent().map(Path::to_path_buf).unwrap_or_default();
    let ours = dir.join(format!("{}-old-test.patch", std::process::id()));
    let theirs = dir.join(format!("{}-old-test.patch", u32::MAX));
    let day_and_more = std::time::Duration::from_secs(25 * 3600);
    let then = std::time::SystemTime::now()
        .checked_sub(day_and_more)
        .ok_or("no time")?;
    for file in [&ours, &theirs] {
        std::fs::write(file, "x\n").map_err(text)?;
        std::fs::File::options()
            .write(true)
            .open(file)
            .and_then(|f| f.set_modified(then))
            .map_err(text)?;
    }
    let second = save_patch("y\n")?;
    let (kept, cleaned) = (ours.exists(), !theirs.exists());
    for file in [&first, &second, &ours, &theirs] {
        let _ = std::fs::remove_file(file);
    }
    assert!(kept);
    assert!(cleaned);
    Ok(())
}

#[test]
fn files_read_in_a_batch_are_those_read_one_by_one() -> Result<(), String> {
    let text = |e: std::io::Error| e.to_string();
    let dir = std::env::temp_dir().join(format!("gitcraft-batch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(text)?;
    let git = |args: &[&str]| run(Some(&dir), args);
    git(&["init", "-q", "-b", "main"])?;
    git(&["config", "user.name", "T"])?;
    git(&["config", "user.email", "t@t"])?;
    std::fs::write(dir.join("a.txt"), "one\ntwo\nthree\nfour\n").map_err(text)?;
    git(&["add", "."])?;
    git(&["commit", "-q", "-m", "root"])?;
    git(&["switch", "-q", "-c", "side"])?;
    git(&["mv", "a.txt", "b.txt"])?;
    std::fs::write(dir.join("b.txt"), "one\ntwo\nthree\nfive\n").map_err(text)?;
    git(&["commit", "-q", "-am", "rename"])?;
    git(&["switch", "-q", "main"])?;
    std::fs::write(dir.join("c.txt"), "c\n").map_err(text)?;
    git(&["add", "."])?;
    git(&["commit", "-q", "-m", "other"])?;
    git(&["merge", "-q", "--no-edit", "side"])?;
    let hashes: Vec<String> = git(&["log", "--format=%H", "--all"])?
        .lines()
        .map(str::to_owned)
        .collect();
    let batch = commit_files_batch(&dir, &hashes);
    let single: Result<Vec<_>, String> = hashes
        .iter()
        .map(|h| commit_files(&dir, h).map(|files| (h.clone(), files)))
        .collect();
    let _ = std::fs::remove_dir_all(&dir);
    let describe = |files: &[(String, Vec<ChangedFile>)]| {
        files
            .iter()
            .map(|(h, files)| {
                let files: Vec<_> = files
                    .iter()
                    .map(|f| {
                        (
                            f.status,
                            f.path.clone(),
                            f.old_path.clone(),
                            f.added,
                            f.removed,
                        )
                    })
                    .collect();
                format!("{h}: {files:?}")
            })
            .collect::<Vec<_>>()
    };
    let (batch, single) = (batch?, single?);
    assert_eq!(batch.len(), 4);
    assert_eq!(describe(&batch), describe(&single));
    Ok(())
}

#[test]
fn a_renamed_file_shows_what_changed_rather_than_all_of_it() -> Result<(), String> {
    let text = |e: std::io::Error| e.to_string();
    let dir = std::env::temp_dir().join(format!("gitcraft-rename-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(text)?;
    let git = |args: &[&str]| -> Result<String, String> {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@t"])
            .args(["-c", "core.autocrlf=false"])
            .args(args)
            .current_dir(&dir)
            .output()
            .map_err(text)?;
        assert!(out.status.success(), "{args:?}");
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    };
    git(&["init", "-q"])?;
    let lines: String = (1..=20).map(|n| format!("line {n}\n")).collect();
    std::fs::write(dir.join("old.txt"), &lines).map_err(text)?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "one"])?;
    let from = git(&["rev-parse", "HEAD"])?;
    std::fs::remove_file(dir.join("old.txt")).map_err(text)?;
    std::fs::write(dir.join("new.txt"), lines.replace("line 7\n", "seven\n")).map_err(text)?;
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "two"])?;
    let to = git(&["rev-parse", "HEAD"])?;
    let diff = range_file_diff(&dir, &from, Some(&to), "new.txt", DiffOptions::default())?;
    let _ = std::fs::remove_dir_all(&dir);
    assert!(diff.contains("rename from old.txt"), "{diff}");
    assert!(diff.contains("+seven"), "{diff}");
    assert!(!diff.contains("+line 1\n"), "{diff}");
    Ok(())
}

/// A file whose name is a pattern (`[ab].txt`) is shown alone, not with `a.txt`; the first
/// commit lists its files even with `log.showRoot` off.
#[test]
fn file_names_are_not_patterns_and_the_first_commit_has_files() -> Result<(), String> {
    let text = |e: std::io::Error| e.to_string();
    let dir = std::env::temp_dir().join(format!("gitcraft-pathspec-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(text)?;
    let git = |args: &[&str]| run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "log.showRoot", "false"],
    ] {
        git(args)?;
    }
    let name = "[ab].txt";
    for (file, body) in [("a.txt", "a\n"), (name, "x\n")] {
        std::fs::write(dir.join(file), body).map_err(text)?;
    }
    git(&["add", "-A"])?;
    git(&["commit", "-qm", "one"])?;
    let root = git(&["rev-parse", "HEAD"])?;
    let root = root.trim();
    let root_files = commit_files(&dir, root)?;
    let root_patch = file_diff(&dir, root, name, DiffOptions::default())?;
    for (file, body) in [("a.txt", "b\n"), (name, "y\n")] {
        std::fs::write(dir.join(file), body).map_err(text)?;
    }
    let file = StatusFile {
        path: name.to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    };
    let work = work_diff(&dir, &file, DiffOptions::default())?;
    let range = range_file_diff(&dir, root, None, name, DiffOptions::default())?;
    git(&["commit", "-qam", "two"])?;
    let head = git(&["rev-parse", "HEAD"])?;
    let patch = file_diff(&dir, head.trim(), name, DiffOptions::default())?;
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(root_files.len(), 2, "{root_files:?}");
    assert!(root_patch.contains("+x"), "{root_patch}");
    for diff in [&work, &range, &patch] {
        assert!(diff.contains("+y"), "{diff}");
        assert!(!diff.contains("a.txt"), "{diff}");
    }
    Ok(())
}

/// With no lines of context in the user's config, a patch of the working tree still has
/// the context `git apply` needs to take lines of it back.
#[test]
fn work_patches_keep_context_for_git_apply() -> Result<(), String> {
    let text = |e: std::io::Error| e.to_string();
    let dir = std::env::temp_dir().join(format!("gitcraft-context-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(text)?;
    let git = |args: &[&str]| run(Some(&dir), args);
    for args in [
        &["init", "-q"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["config", "core.autocrlf", "false"],
    ] {
        git(args)?;
    }
    std::fs::write(dir.join("a.txt"), "1\n2\n3\n4\n5\n").map_err(text)?;
    git(&["add", "a.txt"])?;
    git(&["commit", "-qm", "one"])?;
    git(&["config", "diff.context", "0"])?;
    std::fs::write(dir.join("a.txt"), "1\n2\nX\n4\n5\n").map_err(text)?;
    let file = StatusFile {
        path: "a.txt".to_owned(),
        old_path: None,
        code: 'M',
        area: Area::Unstaged,
    };
    let patch = work_diff(&dir, &file, DiffOptions::default());
    let applied = patch
        .as_deref()
        .map_err(String::clone)
        .and_then(save_patch)
        .and_then(|p| git(&["apply", "--cached", "--check", &p.to_string_lossy()]));
    let _ = std::fs::remove_dir_all(&dir);
    let patch = patch?;
    assert!(patch.contains("\n 2\n-3\n+X\n 4\n"), "{patch}");
    applied?;
    Ok(())
}

#[test]
fn name_status_with_rename() {
    let files = parse_name_status("M\0a.txt\0R090\0old.rs\0new.rs\0D\0gone\0");
    assert_eq!(files.len(), 3);
    assert_eq!(files[1].status, 'R');
    assert_eq!(files[1].path, "new.rs");
    assert_eq!(files[1].old_path.as_deref(), Some("old.rs"));
}

#[test]
fn numstat_with_rename_and_binary() {
    let stats = parse_numstat("1\t2\ta.txt\0-\t-\timg.png\0\n3\t0\t\0old.rs\0new.rs\0");
    assert_eq!(stats.len(), 3);
    assert_eq!(stats[0], ("a.txt".to_owned(), Some(1), Some(2)));
    assert_eq!(stats[1], ("img.png".to_owned(), None, None));
    assert_eq!(stats[2], ("new.rs".to_owned(), Some(3), Some(0)));
}
