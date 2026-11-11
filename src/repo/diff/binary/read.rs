use super::*;

pub(super) fn read_side(
    repo: &Path,
    rev: Option<&str>,
    path: &str,
) -> Result<Option<Side>, String> {
    match rev {
        None => match crate::remote::read_within(repo, path, MAX_READ) {
            Some(read) => match read {
                Ok(Some(bytes)) => Ok(Some(side(path, bytes.len() as u64, &bytes))),
                // Refused: too big, or not a file there at all (deleted, a link).
                Ok(None) => match crate::remote::try_is_file(&crate::remote::join(repo, path)) {
                    Ok(true) => Ok(Some(too_big(None))),
                    Ok(false) => Ok(None),
                    Err(e) => Err(format!("{path}: {e}")),
                },
                Err(e) => Err(format!("{path}: {e}")),
            },
            None => match read_local(&repo.join(path), MAX_READ) {
                Ok(Some(bytes)) => Ok(Some(side(path, bytes.len() as u64, &bytes))),
                Ok(None) => Ok(Some(too_big(None))),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(format!("{path}: {e}")),
            },
        },
        Some(r) => {
            let spec = if r.is_empty() {
                format!(":0:{path}")
            } else {
                format!("{r}:{path}")
            };
            let size: u64 = match git::run(Some(repo), &["cat-file", "-s", &spec]) {
                Ok(n) => n.trim().parse().unwrap_or(0),
                // Not at that revision: the side without the file. There but unreadable (a
                // partial clone without it, a damaged repository): a failure.
                Err(e) => {
                    return match exists_at(repo, r, path) {
                        Ok(false) => Ok(None),
                        Ok(true) => Err(format!("{path}: {e}")),
                        Err(e) => Err(format!("{path}: {e}")),
                    };
                }
            };
            if size > MAX_READ {
                return Ok(Some(too_big(Some(size))));
            }
            let bytes = git::file_at(repo, rev, path)?;
            Ok(Some(side(path, size, &bytes)))
        }
    }
}

/// Whether `path` is in revision `rev` (`""` the index, stage 0); a revision that doesn't
/// exist (the parent of a first commit) has no files.
pub(super) fn exists_at(repo: &Path, rev: &str, path: &str) -> Result<bool, String> {
    if rev.is_empty() {
        let literal = format!(":(literal){path}");
        let out = git::run(Some(repo), &["ls-files", "-s", "-z", "--", &literal])?;
        return Ok(out.split('\0').any(|e| {
            e.split_once('\t')
                .is_some_and(|(info, p)| p == path && info.ends_with(" 0"))
        }));
    }
    let tree = format!("{rev}^{{tree}}");
    if git::run(Some(repo), &["rev-parse", "--verify", "-q", &tree]).is_err() {
        return Ok(false);
    }
    let out = git::run(
        Some(repo),
        &["ls-tree", "-z", "--full-tree", &tree, "--", path],
    )?;
    Ok(out
        .split('\0')
        .any(|e| e.split_once('\t').is_some_and(|(_, p)| p == path)))
}

/// A file of this machine's working tree as git sees it ([`git::worktree_file`]), read up to
/// `max` bytes and no more: `Ok(None)` when it is larger, though it may have grown since it
/// was listed.
pub(super) fn read_local(file: &Path, max: u64) -> std::io::Result<Option<Vec<u8>>> {
    use std::io::Read;
    let meta = std::fs::symlink_metadata(file)?;
    if meta.file_type().is_symlink() {
        let target = std::fs::read_link(file)?
            .into_os_string()
            .into_encoded_bytes();
        return Ok(Some(target));
    }
    if !meta.is_file() {
        return Err(std::io::Error::other(crate::i18n::text("repo-core-3899")));
    }
    if meta.len() > max {
        return Ok(None);
    }
    // Checked again on what was opened: a link put there since would be followed by `open`.
    let opened = std::fs::File::open(file)?;
    if !opened.metadata()?.is_file() {
        return Err(std::io::Error::other(crate::i18n::text("repo-core-3899")));
    }
    let mut bytes = Vec::new();
    opened.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= max).then_some(bytes))
}

pub(super) fn too_big(size: Option<u64>) -> Side {
    Side {
        size,
        kind: None,
        facts: Vec::new(),
        head: Vec::new(),
        text: None,
        text_gaps: Vec::new(),
        archive: Archive::No,
        failed: false,
    }
}

/// A side that couldn't be read.
pub(super) fn failed() -> Side {
    Side {
        failed: true,
        ..too_big(None)
    }
}

pub(super) fn side(path: &str, size: u64, bytes: &[u8]) -> Side {
    let (kind, facts) = sniff::describe(path, bytes);
    let text = extract::document_text(path, bytes);
    let archive = if !zip::is_zip(bytes) {
        Archive::No
    } else {
        match zip::directory(bytes) {
            Some(d) if !d.members.is_empty() || d.complete => Archive::Listed(d),
            _ => Archive::Unreadable,
        }
    };
    let (text, text_gaps) = match text {
        Some(t) => (Some(t.text), t.gaps),
        None => (None, Vec::new()),
    };
    Side {
        size: Some(size),
        kind: Some(kind),
        facts,
        head: bytes[..bytes.len().min(HEX_BYTES)].to_vec(),
        text,
        text_gaps,
        archive,
        failed: false,
    }
}
