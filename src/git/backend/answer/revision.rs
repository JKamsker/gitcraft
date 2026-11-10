//! Native revision syntax, reference resolution, and tag peeling.

use super::*;

/// A repository whose history git reads differently from its objects: left to git.
pub(super) fn plain(repo: &gix::Repository) -> Option<()> {
    super::super::commits::plain_history(repo).ok()
}

/// Splits off `--end-of-options` and the flags in `allowed` (each at most once, in any order)
/// before the positional arguments; `None` for anything else that looks like an option.
pub(super) fn split<'a>(
    args: &'a [&'a str],
    allowed: &[&str],
) -> Option<(Vec<&'a str>, &'a [&'a str])> {
    let mut flags = Vec::new();
    let mut rest = args;
    while let Some((first, tail)) = rest.split_first() {
        if *first == "--end-of-options" {
            return Some((flags, tail));
        }
        if !first.starts_with('-') {
            break;
        }
        if !allowed.contains(first) || flags.contains(first) {
            return None;
        }
        flags.push(*first);
        rest = tail;
    }
    Some((flags, rest))
}

/// What `spec` names as `rev-parse --verify` reads it: `Ok(None)` when it names nothing, `Err`
/// for a form left to git. Taken: a full object id (which, as in git, needn't exist), `HEAD`, a
/// full ref name (`refs/…`), each with `^{commit}`, `^{tree}` or `^{}`; `<that>:<path>`; and
/// `:0:<path>` (or `:<path>`), from the index.
pub(super) fn resolve(repo: &gix::Repository, spec: &str) -> Result<Option<gix::ObjectId>, ()> {
    if let Some(rest) = spec.strip_prefix(':') {
        let path = match rest.split_once(':') {
            Some(("0", path)) => path,
            Some(_) => return Err(()),
            None => rest,
        };
        // `:/text` searches messages, `:!…` is magic.
        if !plain_path(path) || path.starts_with('!') {
            return Err(());
        }
        let index = repo.open_index().map_err(drop)?;
        if index.is_sparse() {
            return Err(());
        }
        return Ok(index
            .entry_by_path_and_stage(
                path.as_bytes().as_bstr(),
                gix::index::entry::Stage::Unconflicted,
            )
            .map(|e| e.id));
    }
    if let Some((base, path)) = spec.split_once(':') {
        if !plain_path(path) {
            return Err(());
        }
        let Some(id) = resolve(repo, &format!("{base}^{{tree}}"))? else {
            return Ok(None);
        };
        let tree = repo.find_tree(id).map_err(drop)?;
        return Ok(tree
            .lookup_entry_by_path(path)
            .map_err(drop)?
            .map(|e| e.object_id()));
    }
    let (base, peel) = match spec.find("^{") {
        Some(at) => (&spec[..at], Some(&spec[at..])),
        None => (spec, None),
    };
    let id = if is_full_id(repo, base) {
        gix::ObjectId::from_hex(base.as_bytes()).map_err(drop)?
    } else if base == "HEAD" {
        let head = repo.head().map_err(drop)?;
        match head.id() {
            Some(id) => id.detach(),
            None => return Ok(None),
        }
    } else if base.starts_with("refs/") && !base.contains(['@', '^', '~', '*', '?', '[']) {
        match reference_id(repo, base)? {
            Some(id) => id,
            None => return Ok(None),
        }
    } else {
        return Err(());
    };
    match peel {
        None => Ok(Some(id)),
        Some("^{}") => peel_tags(repo, id),
        Some("^{commit}") => peel_to(repo, id, gix::object::Kind::Commit),
        Some("^{tree}") => peel_to(repo, id, gix::object::Kind::Tree),
        Some(_) => Err(()),
    }
}

/// A path from the top of the worktree, written plainly: no `./` or `../` (which git reads from
/// the folder it runs in), no leading `/`, empty or `.` parts, nor a `\`.
pub(super) fn plain_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Check whether a revision is a full object ID for this repository's hash format.
pub(super) fn is_full_id(repo: &gix::Repository, text: &str) -> bool {
    text.len() == repo.object_hash().len_in_hex() && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The object the ref `name` (written in full) names, following symbolic refs; none when it
/// doesn't exist. Git would also try `refs/<name>`, `refs/tags/<name>`… after it: when one of
/// those exists, git answers.
pub(super) fn reference_id(
    repo: &gix::Repository,
    name: &str,
) -> Result<Option<gix::ObjectId>, ()> {
    if let Some(reference) = repo.try_find_reference(name).map_err(drop)? {
        return target_id(repo, &reference).map(Some);
    }
    for rule in ["refs/", "refs/tags/", "refs/heads/", "refs/remotes/"] {
        if repo
            .try_find_reference(format!("{rule}{name}").as_str())
            .map_err(drop)?
            .is_some()
        {
            return Err(());
        }
    }
    if repo
        .try_find_reference(format!("refs/remotes/{name}/HEAD").as_str())
        .map_err(drop)?
        .is_some()
    {
        return Err(());
    }
    Ok(None)
}

/// The object `reference` points at, through its symbolic refs (not through tags); `Err` for
/// a dangling one.
pub(super) fn target_id(
    repo: &gix::Repository,
    reference: &gix::Reference<'_>,
) -> Result<gix::ObjectId, ()> {
    let mut target = reference.target().into_owned();
    for _ in 0..5 {
        match target {
            gix::refs::Target::Object(id) => return Ok(id),
            gix::refs::Target::Symbolic(name) => {
                let name = name.as_bstr().to_str().map_err(drop)?;
                let next = repo.try_find_reference(name).map_err(drop)?.ok_or(())?;
                target = next.target().into_owned();
            }
        }
    }
    Err(())
}

/// Follow annotated-tag targets with a bounded traversal.
pub(super) fn peel_tags(
    repo: &gix::Repository,
    mut id: gix::ObjectId,
) -> Result<Option<gix::ObjectId>, ()> {
    for _ in 0..16 {
        let Some(object) = repo.try_find_object(id).map_err(drop)? else {
            return Ok(None);
        };
        if object.kind != gix::object::Kind::Tag {
            return Ok(Some(id));
        }
        id = gix::objs::TagRef::from_bytes(&object.data, repo.object_hash())
            .map_err(drop)?
            .target();
    }
    Err(())
}

/// Peel tags until an object of the requested kind is reached.
pub(super) fn peel_to(
    repo: &gix::Repository,
    id: gix::ObjectId,
    kind: gix::object::Kind,
) -> Result<Option<gix::ObjectId>, ()> {
    let Some(id) = peel_tags(repo, id)? else {
        return Ok(None);
    };
    let object = repo.find_object(id).map_err(drop)?;
    match (object.kind, kind) {
        (found, wanted) if found == wanted => Ok(Some(id)),
        (gix::object::Kind::Commit, gix::object::Kind::Tree) => Ok(Some(
            gix::objs::CommitRef::from_bytes(&object.data, repo.object_hash())
                .map_err(drop)?
                .tree(),
        )),
        _ => Ok(None),
    }
}

/// `rev-parse --verify [-q] [--end-of-options] <spec>`.
pub(super) fn rev_parse(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["--verify", "-q", "--quiet"])?;
    let [spec] = rest else { return None };
    if !flags.contains(&"--verify") || spec.starts_with('-') {
        return None;
    }
    let quiet = flags.contains(&"-q") || flags.contains(&"--quiet");
    let repo = open(path)?;
    plain(&repo)?;
    Some(match resolve(&repo, spec).ok()? {
        Some(id) => Answer::out(format!("{id}\n")),
        None if quiet => Answer::fail(1, ""),
        None => Answer::fail(128, "fatal: Needed a single revision\n"),
    })
}
