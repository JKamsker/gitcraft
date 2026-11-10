//! Excluded-file overlap proofs and fresh cleanup validation.

use super::*;

/// Plain stash cleanup can overwrite an excluded untracked path when the index has
/// deleted that path (or a file above/below it). Snapshots do not clean the worktree.
pub(super) fn removed_paths(work: &WorkStatus) -> impl Iterator<Item = &str> {
    work.staged
        .iter()
        .chain(
            work.unstaged
                .iter()
                .filter(|f| f.area != git::Area::Untracked),
        )
        .filter_map(|f| match f.code {
            'D' => Some(f.path.as_str()),
            'R' => f.old_path.as_deref(),
            _ => None,
        })
}

pub(super) fn excluded_overlap_problem(work: &WorkStatus, untracked: bool) -> Option<String> {
    if untracked {
        return None;
    }
    let overlaps = |a: &str, b: &str| {
        // Git reports embedded repositories as directory entries ending in `/`.
        // Normalize only the comparison; retain Git's spelling in paths and errors.
        let a = a.trim_end_matches('/');
        let b = b.trim_end_matches('/');
        a == b
            || a.strip_prefix(b).is_some_and(|tail| tail.starts_with('/'))
            || b.strip_prefix(a).is_some_and(|tail| tail.starts_with('/'))
    };
    for deleted in removed_paths(work) {
        if let Some(excluded) = work
            .unstaged
            .iter()
            .find(|f| f.area == git::Area::Untracked && overlaps(deleted, &f.path))
        {
            return Some(crate::i18n::message(
                "repo-workflows-partial_stash-excluded-overlap",
                &[("path", excluded.path.clone().into())],
            ));
        }
    }
    None
}

/// Compare actual directory entries on the repository's machine. This also handles
/// per-directory case rules and Unicode aliases without trusting core.ignorecase.
/// Cached UI eligibility stays lexical; execution is the authoritative guard.
pub(super) fn filesystem_overlap_script(
    work: &WorkStatus,
    untracked: bool,
    ignored: &[&str],
) -> Result<String, String> {
    let excluded = work
        .unstaged
        .iter()
        .filter(|f| !untracked && f.area == git::Area::Untracked)
        .map(|f| f.path.as_str())
        .chain(ignored.iter().copied());
    let removed: Vec<_> = removed_paths(work).collect();
    let mut script = String::from(
        "same() { for path in \"$1\" \"$2\"; do if test -L \"$path\" && ! test -e \"$path\"; then printf '%s\\000' \"$3\"; exit 3; fi; done; if test \"$1\" -ef \"$2\"; then printf '%s\\000' \"$3\"; exit 0; else code=$?; test \"$code\" = 1 || exit \"$code\"; fi; }\n",
    );
    for excluded in excluded {
        let b = excluded.trim_end_matches('/');
        for removed in &removed {
            let a = removed.trim_end_matches('/');
            for (whole, ancestors) in [(a, b), (b, a)] {
                for end in ancestors
                    .match_indices('/')
                    .map(|(i, _)| i)
                    .chain(std::iter::once(ancestors.len()))
                {
                    if end == 0 {
                        continue;
                    }
                    script.push_str(&format!(
                        "same {} {} {}\n",
                        crate::remote::sh_quote(&format!("./{whole}")),
                        crate::remote::sh_quote(&format!("./{}", &ancestors[..end])),
                        crate::remote::sh_quote(excluded)
                    ));
                    if script.len() > 48 * 1024 {
                        return Err("Too many excluded-path comparisons to establish safe stash cleanup. Resolve the replacements or use a snapshot.".into());
                    }
                }
            }
        }
    }
    script.push_str("exit 0\n");
    Ok(script)
}

pub(super) fn check_filesystem_overlaps(
    path: &Path,
    work: &WorkStatus,
    untracked: bool,
    ignored: &[&str],
) -> Result<(), String> {
    let script = filesystem_overlap_script(work, untracked, ignored)?;
    let result = git::run_shell(path, &script)?;
    if result.code == 3 {
        // -ef dereferences links; an unresolved target cannot prove distinct entry
        // identity, even when the status spellings differ only in case.
        let output = String::from_utf8_lossy(&result.stdout);
        return Err(crate::i18n::message(
            "repo-workflows-partial_stash-unresolved-link",
            &[(
                "path",
                output
                    .split('\0')
                    .next()
                    .unwrap_or_default()
                    .to_owned()
                    .into(),
            )],
        ));
    }
    if result.code != 0 {
        return Err(format!(
            "Cannot inspect excluded paths safely before stash cleanup: {}",
            String::from_utf8_lossy(&result.stderr)
        ));
    }
    if let Some(excluded) = String::from_utf8_lossy(&result.stdout)
        .split('\0')
        .find(|p| !p.is_empty())
    {
        return Err(crate::i18n::message(
            "repo-workflows-partial_stash-excluded-overlap",
            &[("path", excluded.to_owned().into())],
        ));
    }
    Ok(())
}

/// Everything removes changes; a snapshot only saves them.
pub(crate) fn push_everything_problem(work: &WorkStatus, untracked: bool) -> Option<String> {
    everything_problem(work, untracked).or_else(|| excluded_overlap_problem(work, untracked))
}

/// Decode the arguments the app saves for Redo without changing their serialization.
/// Message values must never be interpreted as options or path separators.
pub(super) fn push_request(args: &[String]) -> Option<StashRequest> {
    if args.len() < 2 || args[0] != "stash" || args[1] != "push" {
        return None;
    }
    let mut request = StashRequest {
        mode: StashMode::Everything,
        message: String::new(),
        untracked: false,
    };
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "-m" | "--message" => i += 1,
            "--staged" => request.mode = StashMode::Staged,
            "-u" | "--include-untracked" | "-a" | "--all" => request.untracked = true,
            "--" => {
                request.mode = StashMode::Files(
                    args[i + 1..]
                        .iter()
                        .map(|p| p.strip_prefix(":(literal)").unwrap_or(p).to_owned())
                        .collect(),
                );
                break;
            }
            _ => {}
        }
        i += 1;
    }
    Some(request)
}

/// Native pull autostash performs the same excluded-file cleanup as plain stash.
/// Check conservatively even when autostash is disabled rather than duplicating
/// Git's version-dependent configuration precedence. This can refuse an overlap
/// unnecessarily, but never expands the operation's capture scope.
pub(in crate::repo) fn validate_excluded_cleanup(path: &Path) -> Result<(), String> {
    validate_excluded_cleanup_state(path, &git::status(path)?, false)
}

pub(super) fn validate_excluded_cleanup_state(
    path: &Path,
    work: &WorkStatus,
    untracked: bool,
) -> Result<(), String> {
    validate_excluded_cleanup_with(
        work,
        untracked,
        || {
            git::run(
                Some(path),
                &[
                    "ls-files",
                    "--others",
                    "--ignored",
                    "--exclude-standard",
                    "-z",
                ],
            )
        },
        |work, untracked, ignored| check_filesystem_overlaps(path, work, untracked, ignored),
    )
}

pub(super) fn validate_excluded_cleanup_with(
    work: &WorkStatus,
    untracked: bool,
    mut read_ignored: impl FnMut() -> Result<String, String>,
    mut check_aliases: impl FnMut(&WorkStatus, bool, &[&str]) -> Result<(), String>,
) -> Result<(), String> {
    if let Some(problem) = excluded_overlap_problem(work, untracked) {
        return Err(problem);
    }
    // Ordinary status omits ignored replacements. --include-untracked excludes them
    // too, including in the separate agent stash plan. Ask Git for their spelling;
    // do not resolve paths on the host (the repository may be remote).
    if removed_paths(work).next().is_some() {
        let ignored = read_ignored()?;
        let ignored_work = WorkStatus {
            staged: work.staged.clone(),
            unstaged: work
                .unstaged
                .iter()
                .filter(|f| f.area != git::Area::Untracked)
                .cloned()
                .chain(
                    ignored
                        .split('\0')
                        .filter(|p| !p.is_empty())
                        .map(|p| git::StatusFile {
                            path: p.to_owned(),
                            old_path: None,
                            code: '?',
                            area: git::Area::Untracked,
                        }),
                )
                .collect(),
            ..WorkStatus::default()
        };
        if let Some(problem) = excluded_overlap_problem(&ignored_work, false) {
            return Err(problem);
        }
        let ignored: Vec<_> = ignored.split('\0').filter(|p| !p.is_empty()).collect();
        check_aliases(work, untracked, &ignored)?;
    }
    Ok(())
}

/// Run immediately before each stash push, including Redo, using fresh repository state.
pub(in crate::repo) fn validate_push(path: &Path, args: &[String]) -> Result<(), String> {
    let Some(request) = push_request(args) else {
        return Ok(());
    };
    let work = git::status(path)?;
    if request.mode == StashMode::Everything {
        validate_excluded_cleanup_state(path, &work, request.untracked)?;
    }
    if let StashMode::Files(paths) = &request.mode {
        // A queued selection can become completely clean. Preserve the existing
        // no-op result while refusing before Git is asked to mutate anything.
        if work.is_clean() {
            return Err(crate::i18n::text("repo-workflows-partial_stash-run-push-a7f01d16").into());
        }
        if let Some(problem) = files_problem(paths, &work) {
            return Err(problem);
        }
        // Saved operations retain their original scope. Do not silently add -u on
        // Redo when a formerly indexed path has become an untracked file.
        if !request.untracked && has_untracked(paths, &work) {
            return Err(crate::i18n::text("repo-workflows-stash-selection-changed").into());
        }
    }
    Ok(())
}
