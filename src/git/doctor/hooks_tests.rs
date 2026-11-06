//! Hooks of real repositories: the folder in effect, problems found, managers recognised, and
//! what a hook printed, recorded as git ran it.

use super::hook_log;
use super::hooks::{self, Manager, Problem};
use super::tests::{git, repo, write};

/// Writes a hook named `name` in `dir` (the hooks folder), executable or not.
fn hook(dir: &std::path::Path, name: &str, text: &[u8], executable: bool) -> Result<(), String> {
    let file = dir.join(name);
    write(&file, text)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    let _ = executable;
    Ok(())
}

#[test]
fn hooks_and_their_problems_are_found() -> Result<(), String> {
    let dir = repo("hooks", &[])?;
    let result = (|| {
        let folder = dir.join(".git").join("hooks");
        hook(
            &folder,
            "pre-commit",
            b"#!/bin/sh\necho checked >&2\nexit 0\n",
            true,
        )?;
        hook(&folder, "commit-msg", b"#!/bin/sh\r\nexit 0\r\n", true)?;
        hook(&folder, "pre-push", b"#!/bin/sh\nexit 0\n", false)?;
        hook(
            &folder,
            "post-merge",
            b"#!/usr/bin/env no-such-interpreter-gc\n",
            true,
        )?;
        hook(&folder, "pre-commit.old", b"#!/bin/sh\n", true)?;
        hook(&folder, "post-checkout.sample", b"#!/bin/sh\n", false)?;
        let found = hooks::hooks(&dir)?;
        assert!(found.folder.ends_with("hooks"), "{}", found.folder);
        assert!(found.hooks_path.is_none() && !found.missing);
        // git init leaves its own samples there too.
        assert!(found.samples >= 1);
        let problems = |name: &str| {
            found
                .active
                .iter()
                .chain(&found.others)
                .find(|h| h.name == name)
                .map(|h| h.problems.clone())
        };
        assert!(found.others.iter().all(|h| h.runs_on.is_none()));
        assert_eq!(problems("pre-commit"), Some(Vec::new()));
        assert_eq!(problems("commit-msg"), Some(vec![Problem::CrlfShebang]));
        if cfg!(unix) {
            assert_eq!(problems("pre-push"), Some(vec![Problem::NotExecutable]));
        }
        assert_eq!(
            problems("post-merge"),
            Some(vec![Problem::MissingInterpreter(
                "no-such-interpreter-gc".into()
            )])
        );
        assert_eq!(problems("pre-commit.old"), Some(vec![Problem::UnknownName]));

        // The one-click fixes.
        let location = |name: &str| crate::remote::resolve(&dir, &found.folder).join(name);
        hooks::to_lf(&location("commit-msg"))?;
        hooks::make_executable(&location("pre-push"))?;
        let again = hooks::hooks(&dir)?;
        let fixed = |name: &str| {
            again
                .active
                .iter()
                .find(|h| h.name == name)
                .is_some_and(|h| h.problems.is_empty())
        };
        assert!(fixed("commit-msg") && fixed("pre-push"), "{again:?}");

        // What the hooks printed is kept: the CRLF commit-msg really stops the commit.
        if cfg!(unix) {
            hook(&folder, "commit-msg", b"#!/bin/sh\r\nexit 0\r\n", true)?;
            let failed = crate::git::run_with(
                Some(&dir),
                &["commit", "-q", "--allow-empty", "-m", "x"],
                None,
            );
            assert!(failed.is_err());
            let runs = hook_log::runs(&dir);
            let last = runs.first().ok_or("no run recorded")?.clone();
            assert_eq!(last.command, "commit");
            assert!(!last.ok && last.output.contains("checked"), "{last:?}");
            assert!(last.output.contains("commit-msg"), "{last:?}");
            // Fixed, the commit goes through, and what pre-commit said is kept all the same.
            hook(&folder, "commit-msg", b"#!/bin/sh\nexit 0\n", true)?;
            crate::git::run_with(
                Some(&dir),
                &["commit", "-q", "--allow-empty", "-m", "x"],
                None,
            )?;
            let runs = hook_log::runs(&dir);
            let last = runs.first().ok_or("no run recorded")?;
            assert!(last.ok && last.output.contains("checked"), "{last:?}");
        }
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn a_hooks_path_and_its_manager() -> Result<(), String> {
    let dir = repo("hooks-path", &[(".husky/pre-commit", b"npx lint-staged\n")])?;
    let result = (|| {
        git(&dir, &["config", "core.hooksPath", ".husky/_"])?;
        let found = hooks::hooks(&dir)?;
        assert_eq!(found.hooks_path.as_deref(), Some(".husky/_"));
        // Not made yet (husky makes it on install): said so, not an error. The script husky's
        // stub would run is checked all the same.
        assert!(found.missing);
        assert_eq!(found.managers, [Manager::Husky]);
        let script = found.active.iter().find(|h| h.name == "pre-commit");
        assert!(
            script.is_some_and(
                |h| h.file.as_deref() == Some(".husky/pre-commit") && h.problems.is_empty()
            ),
            "{found:?}"
        );

        git(&dir, &["config", "core.hooksPath", "my hooks"])?;
        hook(
            &dir.join("my hooks"),
            "pre-push",
            b"#!/bin/sh\nexit 0\n",
            true,
        )?;
        write(&dir.join(".pre-commit-config.yaml"), b"repos: []\n")?;
        let found = hooks::hooks(&dir)?;
        assert!(!found.missing);
        assert_eq!(found.active.len(), 1);
        assert!(found.managers.contains(&Manager::PreCommit));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn hooks_turned_off_and_a_folder_named_as_a_hook() -> Result<(), String> {
    let dir = repo("hooks-off", &[("README.md", b"x\n")])?;
    let result = (|| {
        // Set to nothing: git runs no hook, and the work tree isn't taken for a hooks folder.
        git(&dir, &["config", "core.hooksPath", ""])?;
        let off = hooks::hooks(&dir)?;
        assert!(
            off.disabled && off.active.is_empty() && off.others.is_empty(),
            "{off:?}"
        );
        // Not a folder: no hook runs, not an error.
        if cfg!(unix) {
            git(&dir, &["config", "core.hooksPath", "/dev/null"])?;
            let null = hooks::hooks(&dir)?;
            assert!(null.missing && !null.disabled);
        }
        git(&dir, &["config", "--unset", "core.hooksPath"])?;
        std::fs::create_dir_all(dir.join(".git/hooks/pre-commit")).map_err(|e| e.to_string())?;
        let found = hooks::hooks(&dir)?;
        let folder = found.active.iter().find(|h| h.name == "pre-commit");
        assert_eq!(
            folder.map(|h| h.problems.clone()),
            Some(vec![Problem::IsFolder])
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
