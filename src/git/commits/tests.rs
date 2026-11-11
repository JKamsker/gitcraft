use super::*;

/// Every branch's history leaves out the commits of notes and of `git maintenance`'s
/// prefetching, which are on no branch.
#[test]
fn history_of_branches_has_no_notes_or_prefetched_commits() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-notes-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "one"],
        &["notes", "add", "-m", "a note", "HEAD"],
        &["tag", "v1"],
    ] {
        git(args)?;
    }
    let ahead = git(&[
        "commit-tree",
        "-p",
        "HEAD",
        "-m",
        "fetched ahead",
        "HEAD^{tree}",
    ])?;
    git(&[
        "update-ref",
        "refs/prefetch/remotes/origin/main",
        ahead.trim(),
    ])?;
    let whole = log(&dir, None, Default::default(), 0, 10);
    let found = search(&dir, None, Default::default(), "a");
    let _ = std::fs::remove_dir_all(&dir);
    let summaries: Vec<String> = whole?.into_iter().map(|c| c.summary).collect();
    assert_eq!(summaries, ["one"]);
    assert!(found?.is_empty());
    Ok(())
}

/// A message with the unit and record separators in it (pasted from somewhere) is one
/// commit like any other: in the graph, page after page, its details and a rebase.
#[test]
fn separators_in_a_message_are_only_text() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-separators-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let git = |args: &[&str]| run(Some(&dir), args);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
        &["config", "commit.gpgSign", "false"],
        &["commit", "-q", "--allow-empty", "-m", "base"],
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "odd \x1f one\n\nbody \x1f a \x1e b \x1e",
        ],
        &["commit", "-q", "--allow-empty", "-m", "top"],
    ] {
        git(args)?;
    }
    let odd = git(&["rev-parse", "HEAD~1"])?;
    let odd = odd.trim();
    let whole = log(&dir, None, Default::default(), 0, 10);
    let paged =
        LogStream::start(&dir, None, Default::default(), 0).and_then(|mut s| s.next_page(10));
    let detail = commit_detail(&dir, odd);
    let rebase = rebase_commits(&dir, "HEAD~2");
    let _ = std::fs::remove_dir_all(&dir);
    for commits in [whole?, paged?] {
        let summaries: Vec<&str> = commits.iter().map(|c| c.summary.as_str()).collect();
        assert_eq!(summaries, ["top", "odd \x1f one", "base"]);
        assert_eq!(commits[1].body, "body \x1f a \x1e b \x1e");
        assert_eq!(commits[0].parents, [odd]);
    }
    let detail = detail?;
    assert_eq!((detail.author.as_str(), detail.parents.len()), ("t", 1));
    assert_eq!(detail.message, "odd \x1f one\n\nbody \x1f a \x1e b \x1e");
    let rebase = rebase?;
    assert_eq!(rebase.len(), 2);
    assert_eq!(rebase[0].message, detail.message);
    assert_eq!(rebase[1].summary, "top");
    Ok(())
}

#[test]
fn bodies_fit_on_one_line() {
    assert_eq!(
        one_line(
            "First line
second.

- a
- b
"
        ),
        "First line second. - a - b"
    );
    assert_eq!(one_line(""), "");
    assert_eq!(one_line(&"x ".repeat(500)).chars().count(), 400);
}
