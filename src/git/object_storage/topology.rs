//! Alternate topology is explicit, including broken paths. No donor is ever modified.
use super::{files, line};
use crate::remote;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Store {
    pub path: PathBuf,
    pub donor_of: Option<PathBuf>,
    pub available: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Topology {
    pub stores: Vec<Store>,
    pub worktrees: String,
    pub promisor: bool,
    pub alternates: Option<Vec<u8>>,
    pub warnings: Vec<String>,
}
fn normalize(path: &Path) -> PathBuf {
    let (base, remote_host) = match remote::parse(path) {
        Some(t) => (PathBuf::from(t.path), Some(t.host)),
        None => (path.to_owned(), None),
    };
    let mut clean = PathBuf::new();
    for part in base.components() {
        match part {
            Component::CurDir => (),
            Component::ParentDir => {
                clean.pop();
            }
            _ => clean.push(part),
        }
    }
    match remote_host {
        Some(host) => remote::encode(&host, &clean.to_string_lossy()),
        None => clean,
    }
}
fn unquote(line: &[u8]) -> Result<String, String> {
    if line.first() != Some(&b'"') {
        return String::from_utf8(line.to_vec())
            .map_err(|_| "Alternate paths must be UTF-8 for this inspector".into());
    }
    if line.last() != Some(&b'"') || line.len() < 2 {
        return Err("Unterminated quoted alternate path".into());
    }
    let mut out = Vec::new();
    let mut i = 1;
    while i < line.len() - 1 {
        let c = line[i];
        i += 1;
        if c != b'\\' {
            out.push(c);
            continue;
        }
        let b = *line.get(i).ok_or("Incomplete alternate path escape")?;
        i += 1;
        match b {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'r' => out.push(b'\r'),
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'v' => out.push(11),
            b'a' => out.push(7),
            b'\\' | b'"' => out.push(b),
            b'0'..=b'7' => {
                let digits = line
                    .get(i - 1..i + 2)
                    .ok_or("Incomplete octal alternate escape")?;
                if !digits.iter().all(|b| matches!(b, b'0'..=b'7')) {
                    return Err("Invalid octal alternate escape".into());
                }
                let n = u16::from(digits[0] - b'0') * 64
                    + u16::from(digits[1] - b'0') * 8
                    + u16::from(digits[2] - b'0');
                out.push(u8::try_from(n).map_err(|_| "Alternate escape is out of range")?);
                i += 2;
            }
            _ => return Err("Unsupported alternate path escape".into()),
        }
    }
    let path =
        String::from_utf8(out).map_err(|_| "Alternate paths must be UTF-8 for this inspector")?;
    if path.contains('\0') {
        return Err("Alternate path contains NUL".into());
    }
    Ok(path)
}
pub(super) fn read(
    repo: &Path,
    primary: &Path,
    active: &dyn Fn() -> bool,
) -> Result<Topology, String> {
    let worktrees = line(repo, &["worktree", "list", "--porcelain"], active)?;
    let config = super::super::command::object_storage_output(
        repo,
        &[
            "config",
            "--get-regexp",
            "^(remote\\..*\\.promisor|extensions\\.partialclone)$",
        ],
        Vec::new(),
        64 * 1024,
        active,
    )?;
    if !matches!(config.code, 0 | 1) {
        return Err("Could not inspect promisor configuration".into());
    }
    let promisor = config.stdout.split(|b| *b == b'\n').any(|line| {
        line.split(|b| b.is_ascii_whitespace())
            .nth(1)
            .is_some_and(|v| v != b"false" && v != b"0")
    });
    let alternates = files::read(&remote::join(primary, "info/alternates"), 64 * 1024, active)?;
    let mut topology = Topology {
        stores: Vec::new(),
        worktrees,
        promisor,
        alternates,
        warnings: Vec::new(),
    };
    let mut pending = vec![(normalize(primary), None)];
    let mut seen = BTreeSet::new();
    while let Some((path, parent)) = pending.pop() {
        if !active() {
            return Err("Object inspection cancelled".into());
        }
        if !seen.insert(path.clone()) {
            topology.warnings.push(format!(
                "Repeated or cyclic alternate: {}",
                remote::display(&path)
            ));
            continue;
        }
        if topology.stores.len() >= 16 {
            return Err("More than 16 object stores; inspect this topology with Git.".into());
        }
        let mut store = Store {
            path: path.clone(),
            donor_of: parent,
            available: false,
            error: None,
        };
        match remote::try_is_dir(&path) {
            Ok(found) => store.available = found,
            Err(e) => store.error = Some(e.to_string()),
        }
        if store.available {
            match files::read(&remote::join(&path, "info/alternates"), 64 * 1024, active) {
                Ok(Some(data)) => {
                    for line in data
                        .split(|b| *b == b'\n')
                        .filter(|s| !s.is_empty() && !s.starts_with(b"#"))
                    {
                        match unquote(line.strip_suffix(b"\r").unwrap_or(line)) {
                            Ok(donor) if !donor.is_empty() => pending.push((
                                normalize(&remote::resolve(&path, &donor)),
                                Some(path.clone()),
                            )),
                            Ok(_) => (),
                            Err(e) => {
                                store.error = Some(e);
                            }
                        }
                    }
                }
                Ok(None) => (),
                Err(e) => store.error = Some(e),
            }
            if files::read(
                &remote::join(&path, "info/http-alternates"),
                64 * 1024,
                active,
            )?
            .is_some_and(|s| !s.is_empty())
            {
                topology.warnings.push(format!(
                    "{} has HTTP alternates; remote objects are not downloaded or enumerated.",
                    remote::display(&path)
                ));
            }
        }
        topology.stores.push(store);
    }
    Ok(topology)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn git_quoted_alternate_paths_and_invalid_escapes() -> Result<(), String> {
        assert_eq!(unquote(br#""/donor\040with\011tab""#)?, "/donor with\ttab");
        assert_eq!(
            unquote(br#""/donor\\slash\"quote""#)?,
            "/donor\\slash\"quote"
        );
        assert!(unquote(br#""/bad\999""#).is_err());
        assert_eq!(
            normalize(Path::new("/a/../b/./objects")),
            PathBuf::from("/b/objects")
        );
        Ok(())
    }
}
