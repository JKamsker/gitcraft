//! Diff.

use super::*;

#[test]
pub(super) fn the_diff_tool_is_shown_each_kind_of_change() -> Result<(), String> {
    let repo = Repo::new("diff-kinds")?;
    let first = repo.commit(FILE, "one\n", "first")?;
    let second = repo.commit(FILE, "two\n", "second")?;
    let log = repo.diff_tool(0)?;
    let shown =
        |target: &DiffTarget| open_diff(repo.path(), &configured(), target).map(|()| seen(&log));

    // A commit: its parent against it; the first commit: nothing against it.
    let commit = commit_target(repo.path(), &second, FILE);
    let commit = commit.and_then(|t| shown(&t));
    assert_eq!(commit, Ok(("one\n".to_owned(), "two\n".to_owned())));
    let root = commit_target(repo.path(), &first, FILE).and_then(|t| shown(&t));
    assert_eq!(root, Ok((String::new(), "one\n".to_owned())));

    // Staged, then unstaged on top.
    repo.write(FILE, "three\n")?;
    repo.git(&["add", "--", FILE])?;
    repo.write(FILE, "four\n")?;
    let staged = DiffTarget::Staged {
        path: FILE.to_owned(),
        old_path: None,
    };
    assert_eq!(
        shown(&staged),
        Ok(("two\n".to_owned(), "three\n".to_owned()))
    );
    let unstaged = DiffTarget::Unstaged {
        path: FILE.to_owned(),
    };
    assert_eq!(
        shown(&unstaged),
        Ok(("three\n".to_owned(), "four\n".to_owned()))
    );

    // A comparison of a commit with the working tree.
    let range = range_target(repo.path(), &first, None, FILE);
    assert_eq!(shown(&range), Ok(("one\n".to_owned(), "four\n".to_owned())));
    Ok(())
}

#[test]
pub(super) fn a_renamed_file_is_shown_against_what_it_was() -> Result<(), String> {
    let repo = Repo::new("diff-rename")?;
    let text = "a line long enough to be found again after the rename\n".repeat(20);
    let first = repo.commit("old name.txt", &text, "first")?;
    repo.git(&["mv", "old name.txt", FILE])?;
    repo.write(FILE, &format!("{text}one more\n"))?;
    repo.git(&["add", "--", FILE])?;
    repo.git(&["commit", "-q", "-m", "renamed"])?;
    let renamed = repo.git(&["rev-parse", "HEAD"])?.trim().to_owned();
    let log = repo.diff_tool(0)?;
    let target = commit_target(repo.path(), &renamed, FILE);
    assert!(
        matches!(&target, Ok(DiffTarget::Revisions { old_path: Some(o), .. }) if o == "old name.txt"),
        "{target:?}"
    );
    let shown = target.and_then(|t| open_diff(repo.path(), &configured(), &t));
    assert_eq!(shown, Ok(()));
    assert_eq!(seen(&log), (text.clone(), format!("{text}one more\n")));
    let range = range_target(repo.path(), &first, Some(&renamed), FILE);
    assert!(matches!(
        range,
        DiffTarget::Revisions {
            old_path: Some(_),
            ..
        }
    ));
    Ok(())
}

#[test]
pub(super) fn a_diff_tool_that_fails_or_has_nothing_to_show_says_so() -> Result<(), String> {
    let repo = Repo::new("diff-fails")?;
    repo.commit(FILE, "one\n", "first")?;
    let unstaged = DiffTarget::Unstaged {
        path: FILE.to_owned(),
    };
    // No tool set anywhere.
    repo.write(FILE, "two\n")?;
    let none = open_diff(repo.path(), &configured(), &unstaged);
    assert!(
        none.as_ref()
            .is_err_and(|e| e.contains("No diff tool is set")),
        "{none:?}"
    );
    // Nothing changed.
    repo.diff_tool(0)?;
    repo.write(FILE, "one\n")?;
    let same = open_diff(repo.path(), &configured(), &unstaged);
    assert!(
        same.as_ref()
            .is_err_and(|e| e.contains("nothing to compare")),
        "{same:?}"
    );
    // The tool exits with an error: git ignores it (Beyond Compare exits so whenever the
    // files differ) unless `difftool.trustExitCode` says otherwise.
    repo.diff_tool(3)?;
    repo.write(FILE, "two\n")?;
    assert_eq!(open_diff(repo.path(), &configured(), &unstaged), Ok(()));
    repo.git(&["config", "difftool.trustExitCode", "true"])?;
    let failed = open_diff(repo.path(), &configured(), &unstaged);
    assert!(
        failed
            .as_ref()
            .is_err_and(|e| e.starts_with("fake could not show")),
        "{failed:?}"
    );
    // A preset that isn't installed.
    let missing = ToolChoice {
        preset: Some(Preset::Araxis),
        program: repo
            .path()
            .join("no such program")
            .to_string_lossy()
            .into_owned(),
    };
    let missing = open_diff(repo.path(), &missing, &unstaged);
    assert!(
        missing
            .as_ref()
            .is_err_and(|e| e.starts_with("Araxis Merge could not show")),
        "{missing:?}"
    );
    Ok(())
}

#[test]
pub(super) fn a_preset_runs_its_program_with_the_files() -> Result<(), String> {
    let repo = Repo::new("diff-preset")?;
    repo.commit(FILE, "one\n", "first")?;
    repo.write(FILE, "two\n")?;
    let args = repo.path().with_extension("args");
    // Records its arguments, one a line, and what the last one holds.
    let program = repo.tool(
        "preset",
        &format!(
            "for a; do printf '%s\\n' \"$a\"; done > '{0}'; eval last=\\${{$#}}; cat \"$last\" >> '{0}'",
            args.display()
        ),
    )?;
    let unstaged = DiffTarget::Unstaged {
        path: FILE.to_owned(),
    };
    for (preset, first) in [
        (Preset::BeyondCompare, None),
        (Preset::VsCode, Some("--wait")),
    ] {
        let choice = ToolChoice {
            preset: Some(preset),
            program: program.clone(),
        };
        assert_eq!(open_diff(repo.path(), &choice, &unstaged), Ok(()));
        let got = std::fs::read_to_string(&args).unwrap_or_default();
        let lines: Vec<&str> = got.lines().collect();
        if let Some(first) = first {
            assert_eq!(lines.first(), Some(&first), "{got}");
        }
        // The working tree's own file is the right side.
        assert!(lines.iter().any(|l| l.ends_with(FILE)), "{preset:?}: {got}");
        assert_eq!(lines.last(), Some(&"two"), "{got}");
    }
    Ok(())
}
