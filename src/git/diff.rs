//! Patches and the files they touch: of a commit, between two revisions, of an uncommitted file.

use std::path::{Path, PathBuf};

use super::status::{Area, StatusFile};
use super::{END_OPTS, LITERAL_PATHS, NO_SIGNATURE, now, run, run_ok_codes};

#[derive(Clone, Debug)]
pub struct ChangedFile {
    /// `A`, `M`, `D`, `R`, `C`, `T`…
    pub status: char,
    pub path: String,
    /// Original path for renames and copies.
    pub old_path: Option<String>,
    pub added: Option<u32>,
    pub removed: Option<u32>,
}

/// `--name-status -z`: `STATUS\0path\0` or, for renames/copies, `R100\0old\0new\0`.
pub(crate) fn parse_name_status(out: &str) -> Vec<ChangedFile> {
    let mut files = Vec::new();
    let mut it = out.split('\0').filter(|s| !s.is_empty());
    while let Some(status) = it.next() {
        let code = status.chars().next().unwrap_or('?');
        let Some(first) = it.next() else { break };
        let (path, old_path) = if matches!(code, 'R' | 'C') {
            let Some(second) = it.next() else { break };
            (second, Some(first))
        } else {
            (first, None)
        };
        files.push(ChangedFile {
            status: code,
            path: path.to_owned(),
            old_path: old_path.map(str::to_owned),
            added: None,
            removed: None,
        });
    }
    files
}

/// `--numstat -z`: `add\tdel\tpath\0` or, for renames, `add\tdel\t\0old\0new\0`.
fn parse_numstat(out: &str) -> Vec<(String, Option<u32>, Option<u32>)> {
    let mut result = Vec::new();
    let mut it = out.split('\0');
    while let Some(entry) = it.next() {
        let entry = entry.trim_start_matches('\n');
        if entry.is_empty() {
            continue;
        }
        let mut parts = entry.splitn(3, '\t');
        let (Some(a), Some(d), Some(path)) = (parts.next(), parts.next(), parts.next()) else {
            continue;
        };
        let path = if path.is_empty() {
            // Rename: old path, then new path.
            let _old = it.next();
            it.next().unwrap_or_default().to_owned()
        } else {
            path.to_owned()
        };
        // Binary files report `-`.
        result.push((path, a.parse().ok(), d.parse().ok()));
    }
    result
}

/// Fills in the line counts of `files` from `--numstat -z` output.
fn add_line_counts(files: &mut [ChangedFile], numstat: &str) {
    // By path: a commit can touch tens of thousands of files.
    let mut counts: std::collections::HashMap<String, (Option<u32>, Option<u32>)> =
        parse_numstat(numstat)
            .into_iter()
            .map(|(path, added, removed)| (path, (added, removed)))
            .collect();
    for file in files.iter_mut() {
        if let Some((added, removed)) = counts.remove(&file.path) {
            file.added = added;
            file.removed = removed;
        }
    }
}

/// `show` arguments that list the files `hash` changed (`kind` is `--name-status` or
/// `--numstat`). Merges are shown against their first parent, like most GUIs do; the first
/// commit as adding its files, whatever `log.showRoot` says.
fn commit_files_args<'a>(kind: &'a str, hash: &'a str) -> [&'a str; 10] {
    [
        "show",
        NO_SIGNATURE,
        "--format=",
        "--diff-merges=first-parent",
        "--root",
        "-M",
        kind,
        "-z",
        END_OPTS,
        hash,
    ]
}

/// The files commit `hash` changed, with their line counts.
pub(super) fn commit_files(repo: &Path, hash: &str) -> Result<Vec<ChangedFile>, String> {
    super::backend::dispatch(
        repo,
        super::backend::Read::CommitFiles,
        |repo| match super::backend::commit_files(repo, &[hash.to_owned()])?.pop() {
            Some((_, Some(files))) => Ok(files),
            _ => Err("listed by git".into()),
        },
        || commit_files_by_git(repo, hash),
    )
}

/// [`commit_files`] as git reads them.
pub(super) fn commit_files_by_git(repo: &Path, hash: &str) -> Result<Vec<ChangedFile>, String> {
    // Both at once: two process starts in the time of one.
    let (status_out, numstat_out) = std::thread::scope(|s| {
        let inherited = crate::tasks::Inherited::capture();
        let numstat = s.spawn(move || {
            inherited.run(|| run(Some(repo), &commit_files_args("--numstat", hash)))
        });
        let status = run(Some(repo), &commit_files_args("--name-status", hash));
        let numstat = numstat.join().unwrap_or_else(|_| {
            Err(crate::i18n::text("interaction-teleport-error-stopped").into())
        });
        (status, numstat)
    });
    let (status_out, numstat_out) = (status_out?, numstat_out?);
    let mut files = parse_name_status(&status_out);
    add_line_counts(&mut files, &numstat_out);
    Ok(files)
}

/// The files each of `hashes` changed, as [`commit_files`] lists them, read for all of them by
/// two processes (the names, then the line counts) rather than two each: for reading ahead.
pub fn commit_files_batch(
    repo: &Path,
    hashes: &[String],
) -> Result<Vec<(String, Vec<ChangedFile>)>, String> {
    super::backend::dispatch(
        repo,
        super::backend::Read::CommitFiles,
        |repo| {
            let read = super::backend::commit_files(repo, hashes)?;
            // Those only git lists faithfully, all by one process.
            let by_git: Vec<String> = read
                .iter()
                .filter(|(_, files)| files.is_none())
                .map(|(hash, _)| hash.clone())
                .collect();
            let mut listed: std::collections::HashMap<String, Vec<ChangedFile>> =
                if by_git.is_empty() {
                    Default::default()
                } else {
                    commit_files_batch_by_git(repo, &by_git)?
                        .into_iter()
                        .collect()
                };
            Ok(read
                .into_iter()
                .filter_map(|(hash, files)| {
                    let files = files.or_else(|| listed.remove(&hash))?;
                    Some((hash, files))
                })
                .collect())
        },
        || commit_files_batch_by_git(repo, hashes),
    )
}

/// [`commit_files_batch`] as git reads them.
pub(super) fn commit_files_batch_by_git(
    repo: &Path,
    hashes: &[String],
) -> Result<Vec<(String, Vec<ChangedFile>)>, String> {
    let input = hashes.join("\n") + "\n";
    let args = |kind: &'static str| {
        [
            "log",
            NO_SIGNATURE,
            "--no-walk=unsorted",
            "--stdin",
            "-z",
            // A mark before each commit's hash: its files follow.
            "--format=%x01%H",
            "--diff-merges=first-parent",
            "--root",
            "-M",
            kind,
        ]
    };
    let (names, counts) = std::thread::scope(|s| {
        let inherited = crate::tasks::Inherited::capture();
        let input = &input;
        let counts = s.spawn(move || {
            inherited.run(|| super::run_input(Some(repo), &args("--numstat"), input))
        });
        let names = super::run_input(Some(repo), &args("--name-status"), input);
        let counts = counts.join().unwrap_or_else(|_| {
            Err(crate::i18n::text("interaction-teleport-error-stopped").into())
        });
        (names, counts)
    });
    let (names, counts) = (names?, counts?);
    let counts: std::collections::HashMap<&str, &str> = per_commit(&counts).collect();
    Ok(per_commit(&names)
        .map(|(hash, names)| {
            let mut files = parse_name_status(names);
            if let Some(counts) = counts.get(hash) {
                add_line_counts(&mut files, counts);
            }
            (hash.to_owned(), files)
        })
        .collect())
}

/// `git log -z --format=%x01%H`'s output, by commit: its hash and what follows it.
fn per_commit(out: &str) -> impl Iterator<Item = (&str, &str)> {
    out.split('\u{1}').filter_map(|part| {
        let (hash, rest) = part.split_once('\0')?;
        Some((hash, rest.strip_prefix('\n').unwrap_or(rest)))
    })
}

/// What makes git print a plain patch.
const PATCH_ARGS: [&str; 4] = [
    "--no-color",
    "--no-ext-diff",
    "--src-prefix=a/",
    "--dst-prefix=b/",
];

/// How a patch is read.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffOptions {
    /// `-w`.
    pub ignore_ws: bool,
    /// The whole file as context, not just the lines around the changes.
    pub whole_file: bool,
    /// How lines are paired; `None`: as the user's git config says (`diff.algorithm`).
    pub algorithm: Option<super::DiffAlgorithm>,
    /// `--no-textconv`: the file's own bytes, not what a textconv driver makes of them.
    pub no_textconv: bool,
}

impl DiffOptions {
    fn push_args(self, args: &mut Vec<&str>) {
        // A patch as gitcraft reads it and `git apply` takes it back, whatever the user's
        // config says (`color.ui=always`, `diff.external`, `diff.noprefix`).
        args.extend(PATCH_ARGS);
        if self.ignore_ws {
            args.push("-w");
        }
        if self.no_textconv {
            args.push("--no-textconv");
        }
        if self.whole_file {
            // More context than any file has lines: git shows each file as one hunk.
            args.push("--unified=999999999");
        }
        // On the command line, never in the repository's config.
        args.extend(self.algorithm.map(super::DiffAlgorithm::arg));
    }
}

/// Lines picked from a patch go back through `git apply`, which refuses a hunk without lines
/// of context around it: with `diff.context=0` in the user's config, every one. Asks for the
/// usual three then.
fn keep_context(repo: &Path, opts: DiffOptions, args: &mut Vec<&str>) {
    let context = super::backend::dispatch(
        repo,
        super::backend::Read::DiffContext,
        super::backend::diff_context,
        || {
            Ok(run(Some(repo), &["config", "--type=int", "diff.context"])
                .ok()
                .and_then(|n| n.trim().parse().ok()))
        },
    );
    let no_context = matches!(context, Ok(Some(0)));
    if no_context && !opts.whole_file {
        args.push("--unified=3");
    }
}

/// The patch of one file in a commit.
pub fn file_diff(repo: &Path, hash: &str, path: &str, opts: DiffOptions) -> Result<String, String> {
    super::backend::dispatch(
        repo,
        super::backend::Read::FileDiff,
        |repo| super::backend::file_patch(repo, hash, path, opts),
        || file_diff_by_git(repo, hash, path, opts),
    )
}

/// [`file_diff`] as git prints it.
pub(super) fn file_diff_by_git(
    repo: &Path,
    hash: &str,
    path: &str,
    opts: DiffOptions,
) -> Result<String, String> {
    let mut args = vec![
        LITERAL_PATHS,
        "show",
        NO_SIGNATURE,
        "--format=",
        "--diff-merges=first-parent",
        "--root",
        "-M",
    ];
    opts.push_args(&mut args);
    // Its hunks and lines can be reverted, through `git apply`.
    keep_context(repo, opts, &mut args);
    args.extend([END_OPTS, hash, "--", path]);
    // Limited to its new name, a file renamed in the commit looks new from top to bottom.
    let old_path = commit_files(repo, hash)
        .ok()
        .and_then(|files| files.into_iter().find(|f| f.path == path))
        .and_then(|file| file.old_path);
    args.extend(old_path.as_deref());
    run(Some(repo), &args)
}

/// The files that differ from `from` to `to` (a commit, or the working tree when `None`), with
/// their line counts.
pub fn diff_files(repo: &Path, from: &str, to: Option<&str>) -> Result<Vec<ChangedFile>, String> {
    let mut files = range_name_status(repo, from, to)?;
    add_line_counts(
        &mut files,
        &run(Some(repo), &range_args("--numstat", from, to))?,
    );
    Ok(files)
}

pub(crate) fn investigation_diff_files(
    reader: &super::investigations::Reader<'_>,
    from: &str,
    to: &str,
) -> Result<Vec<ChangedFile>, String> {
    if !super::investigations::oid(from) || !super::investigations::oid(to) {
        return Err(crate::i18n::text("backend-invalid-pinned-diff-request-18c062").into());
    }
    let read = |kind| -> Result<String, String> {
        let mut args = range_args(kind, from, Some(to));
        args.splice(1..1, ["--no-ext-diff", "--no-textconv"]);
        String::from_utf8(reader.run(&args, vec![], &[0])?).map_err(|_| {
            crate::i18n::text("backend-local-diff-returned-non-utf-8-paths-c5fd7b").into()
        })
    };
    let mut files = parse_name_status(&read("--name-status")?);
    add_line_counts(&mut files, &read("--numstat")?);
    Ok(files)
}

/// `diff` arguments that list the files that differ from `from` to `to` (`kind` is
/// `--name-status` or `--numstat`).
fn range_args<'a>(kind: &'a str, from: &'a str, to: Option<&'a str>) -> Vec<&'a str> {
    let mut args = vec!["diff", "-M", kind, "-z", END_OPTS, from];
    args.extend(to);
    args.push("--");
    args
}

/// The files that differ from `from` to `to`, without their line counts.
fn range_name_status(
    repo: &Path,
    from: &str,
    to: Option<&str>,
) -> Result<Vec<ChangedFile>, String> {
    Ok(parse_name_status(&run(
        Some(repo),
        &range_args("--name-status", from, to),
    )?))
}

/// The patch of one file from `from` to `to` (the working tree when `None`).
pub fn range_file_diff(
    repo: &Path,
    from: &str,
    to: Option<&str>,
    path: &str,
    opts: DiffOptions,
) -> Result<String, String> {
    let mut args = vec![LITERAL_PATHS, "diff", "-M"];
    opts.push_args(&mut args);
    args.extend([END_OPTS, from]);
    args.extend(to);
    args.extend(["--", path]);
    // Limited to its new name, a renamed file looks new from top to bottom.
    let old_path = range_name_status(repo, from, to)
        .ok()
        .and_then(|files| files.into_iter().find(|file| file.path == path))
        .and_then(|file| file.old_path);
    args.extend(old_path.as_deref());
    run(Some(repo), &args)
}

/// The patch of one uncommitted file.
pub fn work_diff(repo: &Path, file: &StatusFile, opts: DiffOptions) -> Result<String, String> {
    let path = file.path.as_str();
    if file.area == Area::Untracked {
        // An untracked file has no history: show it as all added.
        let mut args = vec!["diff"];
        opts.push_args(&mut args);
        args.extend(["--no-index", "--", "/dev/null", path]);
        return run_ok_codes(Some(repo), &args, &[0, 1]);
    }
    let mut args: Vec<&str> = match file.area {
        Area::Staged => vec![LITERAL_PATHS, "diff", "--cached", "-M"],
        _ => vec![LITERAL_PATHS, "diff"],
    };
    opts.push_args(&mut args);
    keep_context(repo, opts, &mut args);
    args.extend(["--", path]);
    // A staged rename limited to its new name would look like a new file.
    if file.area == Area::Staged {
        args.extend(file.old_path.as_deref());
    }
    run(Some(repo), &args)
}

#[cfg(test)]
mod tests;

mod patch_export;
#[allow(unused_imports)] // Compatibility re-export.
pub use patch_export::save_patch;
pub use patch_export::save_patch_for;
