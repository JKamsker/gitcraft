//! Overwritten files.

use super::*;

/// The uncommitted files `files` loses, sorted.
pub(super) fn lost_files(repo: &Path, files: &LostFiles) -> Result<Vec<String>, String> {
    let mut paths = match files {
        LostFiles::None => return Ok(Vec::new()),
        LostFiles::Listed(paths) => paths.clone(),
        LostFiles::HardReset(to) => {
            // Not what is in submodules: a reset leaves their working trees as they are.
            let out = run(
                Some(repo),
                &[
                    "status",
                    "--porcelain=v1",
                    "-z",
                    "--untracked-files=no",
                    "--ignore-submodules=all",
                ],
            )?;
            let status = parse_status(&out);
            let mut paths: Vec<String> = (status.staged.into_iter())
                .chain(status.unstaged)
                .chain(status.conflicted)
                .map(|f| f.path)
                .collect();
            if let Some(to) = to {
                paths.extend(written_over(repo, to)?);
            }
            paths
        }
    };
    paths.sort_unstable();
    paths.dedup();
    Ok(paths)
}

/// The untracked or ignored files `reset --hard <to>` writes over: those at a path `to` has a
/// file at and HEAD hasn't (one HEAD has is tracked, and counted as a change).
pub(super) fn written_over(repo: &Path, to: &str) -> Result<Vec<String>, String> {
    let added = run(
        Some(repo),
        &[
            "diff",
            "--no-ext-diff",
            "--no-renames",
            "--name-only",
            "-z",
            "--diff-filter=A",
            "HEAD",
            END_OPTS,
            to,
            "--",
        ],
    );
    // `--end-of-options` there is git 2.30+: older git reads `to` the same without it.
    let added = match added {
        Ok(out) => out,
        Err(_) => run(
            Some(repo),
            &[
                "diff",
                "--no-ext-diff",
                "--no-renames",
                "--name-only",
                "-z",
                "--diff-filter=A",
                "HEAD",
                super::super::not_an_option(to)?,
                "--",
            ],
        )?,
    };
    let added: Vec<&str> = added.split('\0').filter(|p| !p.is_empty()).collect();
    if added.is_empty() {
        return Ok(Vec::new());
    }
    // Every file not tracked, ignored ones too; a folder git tracks nothing in as one entry.
    let untracked = run(Some(repo), &["ls-files", "-z", "-o", "--directory"])?;
    let untracked: Vec<&str> = untracked.split('\0').filter(|p| !p.is_empty()).collect();
    Ok(added
        .into_iter()
        .filter(|path| {
            untracked
                .iter()
                .any(|u| u == path || (u.ends_with('/') && path.starts_with(u)))
        })
        .map(str::to_owned)
        .collect())
}
