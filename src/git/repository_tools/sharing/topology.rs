//! Alternate topology is explicit, including broken paths. No donor is ever modified.
use super::super::guard::text;
use super::super::object::files;
fn line(repo: &std::path::Path, args: &[&str], _: &dyn Fn() -> bool) -> Result<String, String> {
    Ok(text(repo, args)?.trim_end().into())
}
use crate::remote;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Store {
    pub path: PathBuf,
    pub available: bool,
    pub error: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct Topology {
    pub stores: Vec<Store>,
    pub edges: Vec<(PathBuf, PathBuf)>,
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
pub(crate) fn read(
    repo: &Path,
    primary: &Path,
    active: &dyn Fn() -> bool,
) -> Result<Topology, String> {
    let worktrees = line(repo, &["worktree", "list", "--porcelain"], active)?;
    let config = text(repo, &["config", "--null", "--list", "--includes"])?;
    let keys: BTreeSet<_> = config
        .split('\0')
        .map(|row| row.split_once('\n').map_or(row, |(key, _)| key))
        .filter(|key| !key.is_empty())
        .collect();
    let mut promisor = keys
        .iter()
        .any(|key| key.eq_ignore_ascii_case("extensions.partialclone"));
    for key in keys.iter().filter(|key| {
        key.to_ascii_lowercase().starts_with("remote.")
            && key.to_ascii_lowercase().ends_with(".promisor")
    }) {
        promisor |= super::super::remote_config::boolean(repo, key, false)?;
    }
    let alternates = files::read(&remote::join(primary, "info/alternates"), 64 * 1024, active)?;
    let mut topology = Topology {
        stores: Vec::new(),
        edges: Vec::new(),
        worktrees,
        promisor,
        alternates,
        warnings: Vec::new(),
    };
    let mut pending = vec![(normalize(primary), None, Vec::new())];
    let mut walk = Walk::default();
    while let Some((path, parent, ancestors)) = pending.pop() {
        if !active() {
            return Err("Object inspection cancelled".into());
        }
        match walk.enter(&path, parent.as_deref(), &ancestors)? {
            Visit::Cycle => {
                topology
                    .warnings
                    .push(format!("Cyclic alternate: {}", remote::display(&path)));
                continue;
            }
            Visit::Duplicate => continue,
            Visit::New => (),
        }
        let mut descendants = ancestors;
        descendants.push(path.clone());
        if topology.stores.len() >= 16 {
            return Err("More than 16 object stores; inspect this topology with Git.".into());
        }
        let mut store = Store {
            path: path.clone(),
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
                                descendants.clone(),
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
    topology.edges = walk.edges.into_iter().collect();
    Ok(topology)
}

#[derive(Default)]
struct Walk {
    seen: BTreeSet<PathBuf>,
    edges: BTreeSet<(PathBuf, PathBuf)>,
}
#[derive(Debug, PartialEq, Eq)]
enum Visit {
    New,
    Duplicate,
    Cycle,
}
impl Walk {
    fn enter(
        &mut self,
        path: &Path,
        parent: Option<&Path>,
        ancestors: &[PathBuf],
    ) -> Result<Visit, String> {
        if let Some(parent) = parent {
            self.edges.insert((parent.to_owned(), path.to_owned()));
            if self.edges.len() > 256 {
                return Err("More than 256 alternate edges; inspect this topology with Git".into());
            }
        }
        if ancestors.iter().any(|p| p == path) {
            return Ok(Visit::Cycle);
        }
        Ok(if self.seen.insert(path.to_owned()) {
            Visit::New
        } else {
            Visit::Duplicate
        })
    }
}
#[cfg(test)]
mod dag_pure {
    use super::*;
    #[test]
    fn diamond_retains_both_edges_and_only_path_repetition_is_a_cycle() -> Result<(), String> {
        let mut walk = Walk::default();
        let a = PathBuf::from("a");
        let b = PathBuf::from("b");
        let c = PathBuf::from("c");
        let d = PathBuf::from("d");
        assert_eq!(walk.enter(&a, None, &[])?, Visit::New);
        assert_eq!(
            walk.enter(&b, Some(&a), std::slice::from_ref(&a))?,
            Visit::New
        );
        assert_eq!(
            walk.enter(&d, Some(&b), &[a.clone(), b.clone()])?,
            Visit::New
        );
        assert_eq!(
            walk.enter(&c, Some(&a), std::slice::from_ref(&a))?,
            Visit::New
        );
        assert_eq!(
            walk.enter(&d, Some(&c), &[a.clone(), c.clone()])?,
            Visit::Duplicate
        );
        assert_eq!(walk.edges.len(), 4);
        assert!(walk.edges.contains(&(b.clone(), d.clone())));
        assert!(walk.edges.contains(&(c.clone(), d.clone())));
        assert_eq!(
            walk.enter(&d, Some(&c), &[a.clone(), c.clone()])?,
            Visit::Duplicate
        );
        assert_eq!(walk.edges.len(), 4);
        assert_eq!(
            walk.enter(&a, Some(&d), &[a.clone(), b, d.clone()])?,
            Visit::Cycle
        );
        Ok(())
    }
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
