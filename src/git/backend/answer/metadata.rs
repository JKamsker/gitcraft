//! Native notes, bisect, and tree-listing answers.

use super::*;

/// The notes ref `name` stands for, as git expands `--ref` and `core.notesRef`.
pub(super) fn notes_ref(name: &str) -> String {
    if name.starts_with("refs/notes/") {
        name.to_owned()
    } else if name.starts_with("notes/") {
        format!("refs/{name}")
    } else {
        format!("refs/notes/{name}")
    }
}

/// `notes get-ref`, and `notes --ref=<ref> list <object>`.
pub(super) fn notes(path: &Path, args: &[&str]) -> Option<Answer> {
    if args == ["get-ref"] {
        let repo = open(path)?;
        let config = repo.config_snapshot();
        let name = match config.string("core.notesRef") {
            Some(name) => notes_ref(name.to_str().ok()?),
            None => "refs/notes/commits".to_owned(),
        };
        return Some(Answer::out(format!("{name}\n")));
    }
    let (reference, rest) = args.split_first()?;
    let name = notes_ref(reference.strip_prefix("--ref=")?);
    let rest = rest.strip_prefix(&["list"])?;
    let rest = rest.strip_prefix(&["--end-of-options"]).unwrap_or(rest);
    let [spec] = rest else { return None };
    let repo = open(path)?;
    plain(&repo)?;
    let id = resolve(&repo, spec).ok()??;
    let found = match reference_id(&repo, &name).ok()? {
        Some(notes) => {
            let tree = resolve(&repo, &format!("{notes}^{{tree}}")).ok()??;
            note_in(&repo, tree, &id.to_string(), 0)?
        }
        None => None,
    };
    Some(match found {
        Some(note) => Answer::out(format!("{note}\n")),
        None => Answer::fail(1, &format!("error: no note found for object {id}.\n")),
    })
}

/// The note of the object whose id ends in `rest` in the notes tree `tree`, through its fan-out
/// folders (`ab/cdef…`).
pub(super) fn note_in(
    repo: &gix::Repository,
    tree: gix::ObjectId,
    rest: &str,
    depth: usize,
) -> Option<Option<gix::ObjectId>> {
    if depth > 20 {
        return None;
    }
    let tree = repo.find_tree(tree).ok()?;
    let entries = tree.decode().ok()?.entries.clone();
    for entry in &entries {
        let name = entry.filename.to_str().ok()?;
        if name == rest && entry.mode.is_blob() {
            return Some(Some(entry.oid.to_owned()));
        }
    }
    for entry in &entries {
        let name = entry.filename.to_str().ok()?;
        if entry.mode.is_tree()
            && name.len() == 2
            && rest.starts_with(name)
            && let Some(found) = note_in(repo, entry.oid.to_owned(), &rest[2..], depth + 1)?
        {
            return Some(Some(found));
        }
    }
    Some(None)
}

/// `bisect log`.
pub(super) fn bisect_log(path: &Path, args: &[&str]) -> Option<Answer> {
    if args != ["log"] {
        return None;
    }
    let repo = open(path)?;
    Some(match std::fs::read(repo.git_dir().join("BISECT_LOG")) {
        Ok(log) => Answer::out(log),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Answer::fail(1, "error: We are not bisecting.\n")
        }
        Err(_) => return None,
    })
}

/// `ls-tree [-z] [--name-only] <tree-ish> -- <path>…` of plain paths (no patterns, no folder's
/// contents), run in the worktree's top folder.
pub(super) fn ls_tree(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["-z", "--name-only"])?;
    let (spec, paths) = match rest {
        [spec, "--", paths @ ..] if !paths.is_empty() && !spec.starts_with('-') => (*spec, paths),
        _ => return None,
    };
    let nul = flags.contains(&"-z");
    let names_only = flags.contains(&"--name-only");
    for (i, p) in paths.iter().enumerate() {
        if !plain_path(p)
            || p.starts_with(':')
            || p.contains(['*', '?', '['])
            || paths[..i].iter().any(|q| {
                q == p || p.starts_with(&format!("{q}/")) || q.starts_with(&format!("{p}/"))
            })
        {
            return None;
        }
    }
    let repo = open(path)?;
    plain(&repo)?;
    // Paths are relative to the folder git runs in.
    let top = repo.workdir()?;
    if std::fs::canonicalize(top).ok()? != std::fs::canonicalize(path).ok()? {
        return None;
    }
    let tree = resolve(&repo, &format!("{spec}^{{tree}}")).ok()??;
    let tree = repo.find_tree(tree).ok()?;
    let mut found = Vec::new();
    for p in paths {
        if let Some(entry) = tree.lookup_entry_by_path(p).ok()? {
            found.push((*p, entry.mode(), entry.object_id()));
        }
    }
    found.sort_by(|a, b| tree_order(a.0, a.1.is_tree(), b.0, b.1.is_tree()));
    let mut out = Vec::new();
    for (p, mode, id) in found {
        // Without -z git quotes names with unusual characters (`core.quotePath`).
        if !nul && p.bytes().any(|b| !(0x20..0x7f).contains(&b) || b == b'"') {
            return None;
        }
        if !names_only {
            let kind = if mode.is_tree() {
                "tree"
            } else if mode.is_commit() {
                "commit"
            } else {
                "blob"
            };
            out.extend_from_slice(format!("{:06o} {kind} {id}\t", mode.value()).as_bytes());
        }
        out.extend_from_slice(p.as_bytes());
        out.push(if nul { 0 } else { b'\n' });
    }
    Some(Answer::out(out))
}

/// The order git walks a tree in, of the entries at paths `a` and `b` (whether each is a tree):
/// by name, a folder's as though it ended in `/`.
pub(super) fn tree_order(a: &str, a_tree: bool, b: &str, b_tree: bool) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.split('/').peekable(), b.split('/').peekable());
    loop {
        match (x.next(), y.next()) {
            (Some(p), Some(q)) if p == q => {}
            (Some(p), Some(q)) => {
                let p_tree = x.peek().is_some() || a_tree;
                let q_tree = y.peek().is_some() || b_tree;
                let p = [p.as_bytes(), if p_tree { b"/" } else { b"" }].concat();
                let q = [q.as_bytes(), if q_tree { b"/" } else { b"" }].concat();
                return p.cmp(&q);
            }
            (p, q) => return p.is_some().cmp(&q.is_some()),
        }
    }
}
