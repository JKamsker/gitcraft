//! Read exactly the two Git sides, distinguishing absent files from read failures.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::git::{self, Area, ChangedFile};
use crate::repo::DiffKey;

const MAX_BYTES: usize = 2 * 1024 * 1024;
type At = Option<(Option<String>, String)>;

pub(super) fn read_sides(repo: &Path, key: &DiffKey) -> Result<(String, String), String> {
    read_sides_active(repo, key, &|| true)
}
pub(super) fn read_sides_active(
    repo: &Path,
    key: &DiffKey,
    active: &dyn Fn() -> bool,
) -> Result<(String, String), String> {
    // In-process when it can be: the reads below start eight git processes.
    if let DiffKey::Commit { hash, path } = key
        && let Ok((old, new)) = crate::git::commit_file_sides(repo, hash, path)
    {
        return Ok((
            side_text(old.unwrap_or_default())?,
            side_text(new.unwrap_or_default())?,
        ));
    }
    let read = Read {
        active,
        repo,
        deadline: Instant::now() + Duration::from_secs(20),
    };
    let (old, new) = locations(&read, key)?;
    Ok((read_side(&read, old)?, read_side(&read, new)?))
}

struct Read<'a> {
    repo: &'a Path,
    active: &'a dyn Fn() -> bool,
    deadline: Instant,
}
impl Read<'_> {
    fn bytes(&self, args: &[&str], limit: usize) -> Result<Vec<u8>, String> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(crate::i18n::text("repo-core-1758").to_owned());
        }
        let bytes = crate::git::file_comparison::raw_output(self.repo, args, self.active)?;
        if bytes.len() > limit {
            return Err(crate::i18n::text("repo-core-1759").into());
        }
        Ok(bytes)
    }
    fn text(&self, args: &[&str]) -> Result<String, String> {
        String::from_utf8(self.bytes(args, MAX_BYTES)?)
            .map_err(|_| crate::i18n::text("repo-core-1760").to_owned())
    }
    fn files(&self, from: Option<&str>, to: Option<&str>) -> Result<Vec<ChangedFile>, String> {
        let mut args = match from {
            Some(from) => vec![
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "-M",
                "--name-status",
                "-z",
                "--end-of-options",
                from,
            ],
            None => vec![
                "diff-tree",
                "--root",
                "--no-commit-id",
                "-r",
                "--no-ext-diff",
                "--no-textconv",
                "--name-status",
                "-z",
                "--end-of-options",
            ],
        };
        args.extend(to);
        args.push("--");
        Ok(git::parse_name_status(&self.text(&args)?))
    }
}

fn locations(read: &Read<'_>, key: &DiffKey) -> Result<(At, At), String> {
    let at = |rev: Option<&str>, path: &str| Some((rev.map(str::to_owned), path.to_owned()));
    let changed = |files: Vec<ChangedFile>, path: &str, old: Option<&str>, new: Option<&str>| {
        let file = files
            .into_iter()
            .find(|f| f.path == path)
            .ok_or_else(|| crate::i18n::text("repo-core-1768").to_owned())?;
        let before = (file.status != 'A' && old.is_some())
            .then(|| at(old, file.old_path.as_deref().unwrap_or(path)))
            .flatten();
        let after = (file.status != 'D').then(|| at(new, path)).flatten();
        Ok((before, after))
    };
    match key {
        DiffKey::Remerge { .. } => Err(crate::i18n::text("repo-core-1769").to_owned()),
        DiffKey::Commit { hash, path } => {
            let parents = read.text(&[
                "show",
                "--no-show-signature",
                "--no-notes",
                "-s",
                "--format=%P",
                "--end-of-options",
                hash,
                "--",
            ])?;
            let parent = parents.split_whitespace().next();
            changed(read.files(parent, Some(hash))?, path, parent, Some(hash))
        }
        DiffKey::Range { from, to, path } => changed(
            read.files(Some(from), to.as_deref())?,
            path,
            Some(from),
            to.as_deref(),
        ),
        DiffKey::Work(file) => match file.area {
            Area::Staged => Ok((
                (file.code != 'A')
                    .then(|| at(Some("HEAD"), file.old_path.as_deref().unwrap_or(&file.path)))
                    .flatten(),
                (file.code != 'D')
                    .then(|| at(Some(""), &file.path))
                    .flatten(),
            )),
            Area::Unstaged => Ok((
                at(
                    Some(""),
                    if matches!(file.code, 'R' | 'C') {
                        file.old_path.as_deref().unwrap_or(&file.path)
                    } else {
                        &file.path
                    },
                ),
                (file.code != 'D').then(|| at(None, &file.path)).flatten(),
            )),
            Area::Untracked => Ok((None, at(None, &file.path))),
            Area::Conflicted => Err(crate::i18n::text("repo-core-1773").to_owned()),
        },
    }
}

fn read_side(read: &Read<'_>, side: At) -> Result<String, String> {
    check_active(read)?;
    let Some((rev, path)) = side else {
        return Ok(String::new());
    };
    let bytes = match rev {
        Some(rev) => {
            let object = if rev.is_empty() {
                format!(":0:{path}")
            } else {
                format!("{rev}:{path}")
            };
            match git::file_comparison::native_blob(read.repo, &object, MAX_BYTES) {
                Some(bytes) => {
                    bytes.map_err(|()| crate::i18n::text("repo-core-1777").to_owned())?
                }
                None => object_by_git(read, &object)?,
            }
        }
        None => {
            let version = crate::git::file_comparison::raw_working(read.repo, &path, read.active)?;
            if version.warning.is_some()
                || !matches!(version.mode.as_deref(), None | Some("100644" | "100755"))
            {
                return Err(crate::i18n::text("repo-core-1778").into());
            }
            // Missing is distinct from a failed read. Only an actual missing side is empty.
            version.bytes
        }
    };
    check_active(read)?;
    side_text(bytes)
}

/// Reject a private diff read once cancelled or past its deadline.
fn check_active(read: &Read<'_>) -> Result<(), String> {
    if !(read.active)() {
        return Err(crate::i18n::text("repo-core-3914").into());
    }
    if Instant::now() >= read.deadline {
        return Err(crate::i18n::text("repo-core-1758").into());
    }
    Ok(())
}

/// Read one immutable Git object with a size bound and CLI fallback.
fn object_by_git(read: &Read<'_>, object: &str) -> Result<Vec<u8>, String> {
    // Bind both reads to one immutable id, even if HEAD or the index moves.
    let blob = read.text(&["rev-parse", "--verify", "--end-of-options", object])?;
    let blob = blob.trim();
    let size = read.text(&["cat-file", "-s", "--", blob])?;
    if size
        .trim()
        .parse::<usize>()
        .map_err(|_| crate::i18n::text("repo-core-1776"))?
        > MAX_BYTES
    {
        return Err(crate::i18n::text("repo-core-1777").to_owned());
    }
    read.bytes(&["cat-file", "blob", blob], MAX_BYTES)
}

/// A side read, as text: refused when too large, binary or not UTF-8.
fn side_text(bytes: Vec<u8>) -> Result<String, String> {
    if bytes.len() > MAX_BYTES {
        return Err(crate::i18n::text("repo-core-1777").to_owned());
    }
    if bytes.contains(&0) {
        return Err(crate::i18n::text("repo-core-1779").to_owned());
    }
    String::from_utf8(bytes).map_err(|_| crate::i18n::text("repo-core-1780").to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::{StatusFile, fixture::Repo};

    /// Construct a worktree diff key for staged or unstaged fixture paths.
    fn key(path: &str, area: Area, code: char, old: Option<&str>) -> DiffKey {
        DiffKey::Work(StatusFile {
            path: path.into(),
            area,
            code,
            old_path: old.map(str::to_owned),
        })
    }

    /// Check that both sides of staged and unstaged private diffs match Git.
    #[test]
    fn staged_and_unstaged_sides_match_private_git_reader() -> Result<(), String> {
        let repo = Repo::new("raw-work-sides")?;
        repo.commit("base", &[("space ü.cs", Some("old\r\n"))])?;
        std::fs::write(repo.path().join("space ü.cs"), "staged\r\n").map_err(|e| e.to_string())?;
        repo.git(&["add", "."])?;
        std::fs::write(repo.path().join("space ü.cs"), "working\n").map_err(|e| e.to_string())?;
        let read = Read {
            repo: repo.path(),
            active: &|| true,
            deadline: Instant::now() + Duration::from_secs(20),
        };
        for area in [Area::Staged, Area::Unstaged] {
            let k = key("space ü.cs", area, 'M', None);
            let (old, new) = locations(&read, &k)?;
            for side in [old, new] {
                let expected = match &side {
                    Some((Some(rev), path)) => side_text(object_by_git(
                        &read,
                        &if rev.is_empty() {
                            format!(":0:{path}")
                        } else {
                            format!("{rev}:{path}")
                        },
                    )?)?,
                    _ => read_side(&read, side.clone())?,
                };
                assert_eq!(read_side(&read, side)?, expected);
            }
        }
        assert_eq!(
            read_sides(repo.path(), &key("space ü.cs", Area::Staged, 'M', None))?,
            ("old\r\n".into(), "staged\r\n".into())
        );
        repo.git(&["checkout", "--", "space ü.cs"])?;
        repo.git(&["mv", "space ü.cs", "renamed.cs"])?;
        assert_eq!(
            read_sides(
                repo.path(),
                &key("renamed.cs", Area::Staged, 'R', Some("space ü.cs"))
            )?,
            ("old\r\n".into(), "staged\r\n".into())
        );
        repo.git(&["reset", "--hard", "HEAD"])?;
        repo.git(&["rm", "space ü.cs"])?;
        assert_eq!(
            read_sides(repo.path(), &key("space ü.cs", Area::Staged, 'D', None))?,
            ("old\r\n".into(), "".into())
        );
        Ok(())
    }

    /// Check private-file probes respect byte limits and cancellation.
    #[test]
    fn bounded_probe_keeps_recognition_limits_and_cancellation() -> Result<(), String> {
        let repo = Repo::new("raw-probe-limits")?;
        let path = "secret.json";
        let k = key(path, Area::Staged, 'A', None);
        let ciphertext = r#"{"secret":"ENC[cipher]","sops":{"mac":"ENC[mac]","version":"3.9.0"}}"#;
        std::fs::write(repo.path().join(path), ciphertext).map_err(|e| e.to_string())?;
        repo.git(&["add", "."])?;
        assert_eq!(read_sides(repo.path(), &k)?, ("".into(), ciphertext.into()));
        assert_eq!(super::super::probe_checked(repo.path(), &k), Some(true));
        for bytes in [vec![0, 1], vec![0xff], vec![b'x'; MAX_BYTES + 1]] {
            std::fs::write(repo.path().join(path), bytes).map_err(|e| e.to_string())?;
            repo.git(&["add", "."])?;
            assert!(read_sides(repo.path(), &k).is_err());
            assert_eq!(super::super::probe_checked(repo.path(), &k), None);
        }
        let cancelled = Read {
            repo: repo.path(),
            active: &|| false,
            deadline: Instant::now() + Duration::from_secs(20),
        };
        assert!(read_side(&cancelled, None).is_err());
        let expired = Read {
            repo: repo.path(),
            active: &|| true,
            deadline: Instant::now() - Duration::from_secs(1),
        };
        assert!(read_side(&expired, None).is_err());
        Ok(())
    }

    /// Read-only parity/performance check against an explicitly selected real repository.
    #[test]
    #[ignore = "set GITCRAFT_PARITY_REPO to a repository with staged text files"]
    fn staged_probe_repository_parity_and_timing() -> Result<(), String> {
        let path =
            std::env::var_os("GITCRAFT_PARITY_REPO").ok_or("GITCRAFT_PARITY_REPO is required")?;
        let repo = Path::new(&path);
        let files = git::status(repo)?.staged;
        assert!(!files.is_empty());
        let mut native_time = Duration::ZERO;
        let mut git_time = Duration::ZERO;
        for (i, file) in files.iter().enumerate() {
            let k = DiffKey::Work(file.clone());
            let start = Instant::now();
            let actual = read_sides(repo, &k)?;
            native_time += start.elapsed();
            let read = Read {
                repo,
                active: &|| true,
                deadline: Instant::now() + Duration::from_secs(20),
            };
            let start = Instant::now();
            let (old, new) = locations(&read, &k)?;
            let legacy = |side: At| match side {
                Some((Some(rev), path)) => side_text(object_by_git(
                    &read,
                    &if rev.is_empty() {
                        format!(":0:{path}")
                    } else {
                        format!("{rev}:{path}")
                    },
                )?),
                side => read_side(&read, side),
            };
            let expected = (legacy(old)?, legacy(new)?);
            git_time += start.elapsed();
            // Avoid including private file contents in test output on failure.
            assert!(actual == expected, "staged side mismatch at sample {i}");
            if (i + 1) % 16 == 0 {
                println!("verified {} staged files", i + 1);
            }
        }
        println!(
            "{} staged files: native probes {:.1} ms total / {:.2} ms mean; legacy Git probes {:.1} ms total / {:.2} ms mean",
            files.len(),
            native_time.as_secs_f64() * 1000.0,
            native_time.as_secs_f64() * 1000.0 / files.len() as f64,
            git_time.as_secs_f64() * 1000.0,
            git_time.as_secs_f64() * 1000.0 / files.len() as f64,
        );
        let mut source: Vec<_> = files.iter().filter(|f| f.path.ends_with(".cs")).collect();
        source.sort_by_key(|f| {
            std::cmp::Reverse(
                std::fs::metadata(repo.join(&f.path))
                    .map(|m| m.len())
                    .unwrap_or(0),
            )
        });
        for (i, file) in source.into_iter().take(3).enumerate() {
            let start = Instant::now();
            let data = crate::repo::diff::load(
                repo,
                &DiffKey::Work(file.clone()),
                git::DiffOptions::default(),
                true,
            )?;
            println!(
                "large C# sample {i}: prepared {} diff lines in {:.1} ms",
                data.lines.len(),
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
        Ok(())
    }
}
