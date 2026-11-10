//! Reads git answers from the repository's own files alone, answered in-process with the bytes
//! and the exit code git gives them: a lookup in the config, `rev-parse --verify`, `cat-file`,
//! `symbolic-ref`, `for-each-ref`, `ls-tree`, `merge-base --is-ancestor`, `notes`, `bisect
//! log`. A reload asks a hundred of these, each a git process of 50 to 300 ms on Windows; here
//! they take a millisecond or two (see `command::execute`, which asks first).
//!
//! Only the forms whose answer is certain are taken. Any other form or option, and any
//! repository whose history is rewritten (grafts, replaced objects, shallow), is `None`: git
//! is run, as before.

use std::path::Path;

use gix::bstr::{BString, ByteSlice};
mod status;

/// What git would have printed, and how it would have ended.
#[derive(Debug, PartialEq, Eq)]
pub(in crate::git) struct Answer {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Answer {
    fn out(stdout: impl Into<Vec<u8>>) -> Self {
        Self {
            code: 0,
            stdout: stdout.into(),
            stderr: Vec::new(),
        }
    }
    fn fail(code: i32, stderr: &str) -> Self {
        Self {
            code,
            stdout: Vec::new(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }
}

/// The answer git would give to `args` in `path`, made in-process; `None` when git is to run.
/// The caller has made sure the repository may be read natively (see `capability`).
pub(in crate::git) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
    // A read of local objects never fetches: the option changes nothing here.
    let args = args.strip_prefix(&["--no-lazy-fetch"]).unwrap_or(args);
    let (command, rest) = args.split_first()?;
    match *command {
        "config" => config::answer(path, rest),
        "rev-parse" => rev_parse(path, rest),
        "cat-file" => cat_file(path, rest),
        "symbolic-ref" => symbolic_ref(path, rest),
        "merge-base" => merge_base(path, rest),
        "notes" => notes(path, rest),
        "bisect" => bisect_log(path, rest),
        "ls-tree" => ls_tree(path, rest),
        "for-each-ref" => refs::answer(path, rest),
        "rev-list" => count(path, rest),
        "status" => status::answer(path, rest),
        _ => None,
    }
}

/// `rev-list --count [--exclude=<glob>…] --all`: the commits reachable from HEAD and every ref
/// but those excluded, in a repository without linked worktrees (whose HEADs `--all` counts
/// too).
fn count(path: &Path, args: &[&str]) -> Option<Answer> {
    let rest = args.strip_prefix(&["--count"])?;
    let (all, excludes) = rest.split_last()?;
    if *all != "--all" {
        return None;
    }
    let mut excluded = Vec::new();
    for exclude in excludes {
        let glob = exclude.strip_prefix("--exclude=")?;
        if glob.is_empty() || glob.contains(['?', '[', '\\']) {
            return None;
        }
        // Git matches these without minding `/`: `refs/gitcraft/*` takes `refs/gitcraft/a/b`.
        let pieces: Vec<String> = glob.split('*').map(regex::escape).collect();
        excluded.push(regex::Regex::new(&format!("^{}$", pieces.join(".*"))).ok()?);
    }
    let repo = open(path)?;
    plain(&repo)?;
    let worktrees = repo.common_dir().join("worktrees");
    if std::fs::read_dir(&worktrees).is_ok_and(|mut d| d.next().is_some()) {
        return None;
    }
    let mut tips = Vec::new();
    if let Some(head) = repo.head().ok()?.id() {
        tips.push(head.detach());
    }
    for reference in repo.references().ok()?.all().ok()? {
        let reference = reference.ok()?;
        let name = reference.name().as_bstr().to_str().ok()?;
        if excluded.iter().any(|e| e.is_match(name)) {
            continue;
        }
        // Refs of a tree or a blob add no commit.
        if let Some(id) = peel_to(
            &repo,
            target_id(&repo, &reference).ok()?,
            gix::object::Kind::Commit,
        )
        .ok()?
        {
            tips.push(id);
        }
    }
    tips.sort();
    tips.dedup();
    let mut commits = 0usize;
    if !tips.is_empty() {
        for info in repo.rev_walk(tips).all().ok()? {
            info.ok()?;
            commits += 1;
        }
    }
    Some(Answer::out(format!("{commits}\n")))
}

/// The name of the command answered, for the backend's metrics: `config`, `rev-parse`…
pub(in crate::git) fn name(args: &[&str]) -> &'static str {
    let args = args.strip_prefix(&["--no-lazy-fetch"]).unwrap_or(args);
    match args.first().copied() {
        Some("config") => "cmd:config",
        Some("rev-parse") => "cmd:rev-parse",
        Some("cat-file") => "cmd:cat-file",
        Some("symbolic-ref") => "cmd:symbolic-ref",
        Some("merge-base") => "cmd:merge-base",
        Some("notes") => "cmd:notes",
        Some("bisect") => "cmd:bisect",
        Some("ls-tree") => "cmd:ls-tree",
        Some("for-each-ref") => "cmd:for-each-ref",
        Some("rev-list") => "cmd:rev-list",
        Some("status") => "cmd:status",
        _ => "cmd:other",
    }
}

fn open(path: &Path) -> Option<gix::Repository> {
    super::native::open(path).ok()
}

/// Raw object bytes for private comparisons. Resolve once, inspect the header before
/// allocating the blob, and never run filters, textconv, or a lazy fetch.
/// None retains Git for unsupported contexts; Err means the size limit was exceeded.
pub(super) fn bounded_blob(path: &Path, spec: &str, limit: usize) -> Option<Result<Vec<u8>, ()>> {
    let repo = open(path)?;
    plain(&repo)?;
    let id = resolve(&repo, spec).ok()??;
    let header = repo.try_find_header(id).ok()??;
    if header.kind() != gix::object::Kind::Blob {
        return None;
    }
    if header.size() > limit as u64 {
        return Some(Err(()));
    }
    let blob = repo.find_blob(id).ok()?;
    Some(if blob.data.len() <= limit {
        Ok(blob.detach().data)
    } else {
        Err(())
    })
}

/// A repository whose history git reads differently from its objects: left to git.
fn plain(repo: &gix::Repository) -> Option<()> {
    super::commits::plain_history(repo).ok()
}

/// Splits off `--end-of-options` and the flags in `allowed` (each at most once, in any order)
/// before the positional arguments; `None` for anything else that looks like an option.
fn split<'a>(args: &'a [&'a str], allowed: &[&str]) -> Option<(Vec<&'a str>, &'a [&'a str])> {
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
fn resolve(repo: &gix::Repository, spec: &str) -> Result<Option<gix::ObjectId>, ()> {
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
fn plain_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn is_full_id(repo: &gix::Repository, text: &str) -> bool {
    text.len() == repo.object_hash().len_in_hex() && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The object the ref `name` (written in full) names, following symbolic refs; none when it
/// doesn't exist. Git would also try `refs/<name>`, `refs/tags/<name>`… after it: when one of
/// those exists, git answers.
fn reference_id(repo: &gix::Repository, name: &str) -> Result<Option<gix::ObjectId>, ()> {
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
fn target_id(repo: &gix::Repository, reference: &gix::Reference<'_>) -> Result<gix::ObjectId, ()> {
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

fn peel_tags(repo: &gix::Repository, mut id: gix::ObjectId) -> Result<Option<gix::ObjectId>, ()> {
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

fn peel_to(
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
fn rev_parse(path: &Path, args: &[&str]) -> Option<Answer> {
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

/// `cat-file -s|-t <object>`, and `cat-file <type> <object>` of an object of that type. An
/// object not found is git's to report.
fn cat_file(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["-s", "-t"])?;
    let (kind, spec) = match (flags.as_slice(), rest) {
        ([flag], [spec]) => (*flag, *spec),
        ([], [kind @ ("blob" | "commit" | "tree" | "tag"), spec]) => (*kind, *spec),
        _ => return None,
    };
    let repo = open(path)?;
    plain(&repo)?;
    let id = resolve(&repo, spec).ok()??;
    let object = repo.try_find_object(id).ok()??;
    Some(match kind {
        "-s" => Answer::out(format!("{}\n", object.data.len())),
        "-t" => Answer::out(format!("{}\n", object.kind)),
        kind if kind.as_bytes() == object.kind.as_bytes() => Answer::out(object.detach().data),
        _ => return None,
    })
}

/// `symbolic-ref --short -q HEAD` (or `-q --short`) of a branch, whose short name nothing else
/// could be read as.
fn symbolic_ref(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["--short", "-q", "--quiet"])?;
    if rest != ["HEAD"] || !flags.contains(&"--short") || flags.len() != 2 {
        return None;
    }
    let repo = open(path)?;
    let head = repo.head().ok()?;
    let Some(name) = head.referent_name() else {
        return Some(Answer::fail(1, ""));
    };
    let full = name.as_bstr().to_str().ok()?;
    let short = full.strip_prefix("refs/heads/")?;
    // Git writes `heads/<name>` when `<name>` alone could be read as another ref.
    if repo.git_dir().join(short).exists() || repo.common_dir().join(short).exists() {
        return None;
    }
    for rule in ["refs/", "refs/tags/"] {
        if repo
            .try_find_reference(format!("{rule}{short}").as_str())
            .ok()?
            .is_some()
        {
            return None;
        }
    }
    Some(Answer::out(format!("{short}\n")))
}

/// `merge-base --is-ancestor <a> <b>` of two commits.
fn merge_base(path: &Path, args: &[&str]) -> Option<Answer> {
    let (flags, rest) = split(args, &["--is-ancestor"])?;
    let ([_], [a, b]) = (flags.as_slice(), rest) else {
        return None;
    };
    let repo = open(path)?;
    plain(&repo)?;
    let a = resolve(&repo, &format!("{a}^{{commit}}")).ok()??;
    let b = resolve(&repo, &format!("{b}^{{commit}}")).ok()??;
    let ancestor = a == b || repo.merge_base(a, b).ok()?.is_some_and(|base| base == a);
    Some(Answer {
        code: if ancestor { 0 } else { 1 },
        stdout: Vec::new(),
        stderr: Vec::new(),
    })
}

/// The notes ref `name` stands for, as git expands `--ref` and `core.notesRef`.
fn notes_ref(name: &str) -> String {
    if name.starts_with("refs/notes/") {
        name.to_owned()
    } else if name.starts_with("notes/") {
        format!("refs/{name}")
    } else {
        format!("refs/notes/{name}")
    }
}

/// `notes get-ref`, and `notes --ref=<ref> list <object>`.
fn notes(path: &Path, args: &[&str]) -> Option<Answer> {
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
fn note_in(
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
        if entry.mode.is_tree() && name.len() == 2 && rest.starts_with(name) {
            if let Some(found) = note_in(repo, entry.oid.to_owned(), &rest[2..], depth + 1)? {
                return Some(Some(found));
            }
        }
    }
    Some(None)
}

/// `bisect log`.
fn bisect_log(path: &Path, args: &[&str]) -> Option<Answer> {
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
fn ls_tree(path: &Path, args: &[&str]) -> Option<Answer> {
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
        if !nul && p.bytes().any(|b| b < 0x20 || b >= 0x7f || b == b'"') {
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
fn tree_order(a: &str, a_tree: bool, b: &str, b_tree: bool) -> std::cmp::Ordering {
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

mod config {
    //! `git config [--local] [-z|--null] [--bool|--type=bool|--int|--type=int]
    //! [--get|--get-all|--get-regexp] <key or pattern>`: the config files git reads (the
    //! installation's, the system's, the user's, the repository's and its worktree's, with
    //! what they include), in git's order. `--local` is the repository's file alone, without
    //! what it includes, as git reads a named file.

    use super::*;

    enum Action {
        Get,
        GetAll,
        GetRegexp,
    }
    enum Type {
        Raw,
        Bool,
        Int,
    }

    pub(super) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
        let mut local = false;
        let mut nul = false;
        let mut kind = Type::Raw;
        let mut action = None;
        let mut positional = Vec::new();
        for arg in args {
            match *arg {
                "--local" if !local => local = true,
                "-z" | "--null" if !nul => nul = true,
                "--bool" | "--type=bool" if matches!(kind, Type::Raw) => kind = Type::Bool,
                "--int" | "--type=int" if matches!(kind, Type::Raw) => kind = Type::Int,
                "--get" if action.is_none() => action = Some(Action::Get),
                "--get-all" if action.is_none() => action = Some(Action::GetAll),
                "--get-regexp" if action.is_none() => action = Some(Action::GetRegexp),
                arg if arg.starts_with('-') => return None,
                arg => positional.push(arg),
            }
        }
        // `git config <key>` gets; with a value it would set.
        let [query] = positional.as_slice() else {
            return None;
        };
        let action = action.unwrap_or(Action::Get);
        let matches: Box<dyn Fn(&str) -> bool> = match action {
            Action::Get | Action::GetAll => {
                let (section, subsection, name) = key_parts(query)?;
                let (section, subsection, name) = (
                    section.to_owned(),
                    subsection.map(str::to_owned),
                    name.to_owned(),
                );
                Box::new(move |key: &str| {
                    key_parts(key).is_some_and(|(s, sub, n)| {
                        s.eq_ignore_ascii_case(&section)
                            && n.eq_ignore_ascii_case(&name)
                            && sub == subsection.as_deref()
                    })
                })
            }
            Action::GetRegexp => {
                // `!pattern` inverts. Git's (POSIX) regexes read `\d`, `\w`… as the letter
                // itself: those are left to it.
                let escaped_word = query
                    .as_bytes()
                    .windows(2)
                    .any(|w| w[0] == b'\\' && w[1].is_ascii_alphanumeric());
                if query.starts_with('!') || escaped_word {
                    return None;
                }
                let pattern = regex::Regex::new(&lowered_ends(query)).ok()?;
                Box::new(move |key: &str| pattern.is_match(key))
            }
        };
        let repo = open(path)?;
        let entries = entries(&repo, local)?;
        let found: Vec<&(String, Option<BString>)> =
            entries.iter().filter(|(key, _)| matches(key)).collect();
        if found.is_empty() {
            return Some(Answer::fail(1, ""));
        }
        let shown = match action {
            Action::Get => &found[found.len() - 1..],
            Action::GetAll | Action::GetRegexp => &found[..],
        };
        let keys = matches!(action, Action::GetRegexp);
        let mut out = Vec::new();
        for (key, value) in shown {
            let value = match (&kind, value) {
                (Type::Raw, Some(value)) => Some(value.to_vec()),
                (Type::Raw, None) => None,
                (Type::Bool, value) => Some(boolean(value.as_ref())?.as_bytes().to_vec()),
                (Type::Int, Some(value)) => Some(integer(value)?.to_string().into_bytes()),
                (Type::Int, None) => return None,
            };
            if keys {
                out.extend_from_slice(key.as_bytes());
                if let Some(value) = &value {
                    out.push(if nul { b'\n' } else { b' ' });
                    out.extend_from_slice(value);
                }
            } else if let Some(value) = &value {
                out.extend_from_slice(value);
            }
            out.push(if nul { 0 } else { b'\n' });
        }
        Some(Answer::out(out))
    }

    /// A key's section, subsection and name: `remote.origin.url`, `user.name`.
    fn key_parts(key: &str) -> Option<(&str, Option<&str>, &str)> {
        let (section, rest) = key.split_once('.')?;
        let (subsection, name) = match rest.rsplit_once('.') {
            Some((sub, name)) => (Some(sub), name),
            None => (None, rest),
        };
        let word =
            |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
        (word(section) && word(name) && name.as_bytes()[0].is_ascii_alphabetic())
            .then_some((section, subsection, name))
    }

    /// The pattern as git matches it: what is before its first dot and after its last one in
    /// lower case, as keys' sections and names are.
    fn lowered_ends(pattern: &str) -> String {
        let first = pattern.find('.').unwrap_or(pattern.len());
        let last = pattern.rfind('.').map_or(0, |i| i + 1);
        pattern
            .char_indices()
            .map(|(i, c)| {
                if i < first || i >= last {
                    c.to_ascii_lowercase()
                } else {
                    c
                }
            })
            .collect()
    }

    /// Every setting git reads, in its order, as `section.Subsection.name` and its value
    /// (`None`: written without `=`). `None` when one can't be read as git reads it.
    fn entries(repo: &gix::Repository, local: bool) -> Option<Vec<(String, Option<BString>)>> {
        use gix::config::Source;
        let snapshot = repo.config_snapshot();
        let file = snapshot.plumbing();
        let mut out = Vec::new();
        // The same file found as the installation's and the system's is read once.
        let mut first_level: Vec<(Source, std::path::PathBuf)> = Vec::new();
        for section in file.sections() {
            let meta = section.meta();
            let wanted = if local {
                meta.source == Source::Local && meta.level == 0
            } else {
                matches!(
                    meta.source,
                    Source::GitInstallation
                        | Source::System
                        | Source::Git
                        | Source::User
                        | Source::Local
                        | Source::Worktree
                )
            };
            if !wanted {
                continue;
            }
            if meta.level == 0
                && let Some(path) = &meta.path
            {
                let seen = first_level
                    .iter()
                    .any(|(source, p)| *source != meta.source && p == path);
                if seen {
                    continue;
                }
                if !first_level
                    .iter()
                    .any(|(s, p)| *s == meta.source && p == path)
                {
                    first_level.push((meta.source, path.clone()));
                }
            }
            let header = section.header();
            if header.is_legacy() {
                return None;
            }
            let name = header.name().to_str().ok()?.to_ascii_lowercase();
            let prefix = match header.subsection_name() {
                Some(sub) => format!("{name}.{}", sub.to_str().ok()?),
                None => name,
            };
            let body = section.body();
            let names: Vec<String> = body.value_names().collect();
            let mut taken: Vec<(String, usize)> = Vec::new();
            for value_name in &names {
                let lower = value_name.to_ascii_lowercase();
                let occurrences = names
                    .iter()
                    .filter(|n| n.eq_ignore_ascii_case(value_name))
                    .count();
                let value = if occurrences == 1 {
                    // `None` inside: written without `=`, which `values` reads as empty.
                    body.value_implicit(value_name)?
                } else {
                    let at = match taken.iter_mut().find(|(n, _)| *n == lower) {
                        Some((_, at)) => {
                            *at += 1;
                            *at - 1
                        }
                        None => {
                            taken.push((lower.clone(), 1));
                            0
                        }
                    };
                    let values = body.values(value_name);
                    // One of them without `=`: which one isn't known here.
                    if values.len() != occurrences || body.value_implicit(value_name)?.is_none() {
                        return None;
                    }
                    Some(values.get(at)?.clone())
                };
                out.push((format!("{prefix}.{lower}"), value));
            }
        }
        Some(out)
    }

    /// `--bool`, as git prints it; `None` for what git refuses.
    fn boolean(value: Option<&BString>) -> Option<&'static str> {
        let Some(value) = value else {
            return Some("true");
        };
        let text = value.to_str().ok()?;
        match text.to_ascii_lowercase().as_str() {
            "true" | "yes" | "on" => Some("true"),
            "false" | "no" | "off" | "" => Some("false"),
            _ => integer(value).map(|n| if n != 0 { "true" } else { "false" }),
        }
    }

    /// `--type=int`: a whole number, with `k`, `m` or `g` after it; `None` for other forms.
    fn integer(value: &BString) -> Option<i64> {
        let text = value.to_str().ok()?;
        let (digits, unit) = match text.char_indices().last()? {
            (i, 'k' | 'K') => (&text[..i], 1 << 10),
            (i, 'm' | 'M') => (&text[..i], 1 << 20),
            (i, 'g' | 'G') => (&text[..i], 1 << 30),
            _ => (text, 1),
        };
        let unsigned = digits.strip_prefix('-').unwrap_or(digits);
        // Git also reads `0x10` and `010` (octal): left to it.
        if unsigned.is_empty()
            || !unsigned.bytes().all(|b| b.is_ascii_digit())
            || (unsigned.len() > 1 && unsigned.starts_with('0'))
        {
            return None;
        }
        digits.parse::<i64>().ok()?.checked_mul(unit)
    }
}

mod refs {
    //! `for-each-ref --format=<format> [--sort=[-]committerdate] [<pattern>…]` with the
    //! atoms `refname`, `objectname`, `objecttype`, `symref`, `parent`, `creatordate:unix`,
    //! `committerdate:unix` and `contents` (of commits), in the main worktree.

    use super::*;

    enum Piece {
        Text(Vec<u8>),
        RefName,
        ObjectName,
        ObjectType,
        SymRef,
        Parent,
        CreatorDate,
        CommitterDate,
        Contents,
    }

    fn format(text: &str) -> Option<Vec<Piece>> {
        let bytes = text.as_bytes();
        let mut pieces = Vec::new();
        let mut literal = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'%' {
                literal.push(bytes[i]);
                i += 1;
                continue;
            }
            match bytes.get(i + 1)? {
                b'%' => {
                    literal.push(b'%');
                    i += 2;
                }
                b'(' => {
                    let end = i + text[i..].find(')')?;
                    let piece = match &text[i + 2..end] {
                        "refname" => Piece::RefName,
                        "objectname" => Piece::ObjectName,
                        "objecttype" => Piece::ObjectType,
                        "symref" => Piece::SymRef,
                        "parent" => Piece::Parent,
                        "creatordate:unix" => Piece::CreatorDate,
                        "committerdate:unix" => Piece::CommitterDate,
                        "contents" => Piece::Contents,
                        _ => return None,
                    };
                    if !literal.is_empty() {
                        pieces.push(Piece::Text(std::mem::take(&mut literal)));
                    }
                    pieces.push(piece);
                    i = end + 1;
                }
                _ => {
                    let hex = text.get(i + 1..i + 3)?;
                    literal.push(u8::from_str_radix(hex, 16).ok()?);
                    i += 3;
                }
            }
        }
        if !literal.is_empty() {
            pieces.push(Piece::Text(literal));
        }
        Some(pieces)
    }

    /// A pattern as `for-each-ref` matches it: in full or up to a `/`, or as a glob of `*`
    /// (not across `/`) and `**` (whole folders).
    struct Pattern {
        text: String,
        glob: Option<regex::Regex>,
    }

    impl Pattern {
        fn new(text: &str) -> Option<Self> {
            if text.is_empty() || text.contains(['?', '[', '\\']) {
                return None;
            }
            let glob = if text.contains('*') {
                let mut re = String::from("^");
                let parts: Vec<&str> = text.split('/').collect();
                for (i, part) in parts.iter().enumerate() {
                    let last = i + 1 == parts.len();
                    if *part == "**" {
                        re.push_str(if last { ".*" } else { "(?:.*/)?" });
                        continue;
                    }
                    if part.contains("**") {
                        return None;
                    }
                    let pieces: Vec<String> = part.split('*').map(regex::escape).collect();
                    re.push_str(&pieces.join("[^/]*"));
                    if !last {
                        re.push('/');
                    }
                }
                re.push('$');
                Some(regex::Regex::new(&re).ok()?)
            } else {
                None
            };
            Some(Self {
                text: text.to_owned(),
                glob,
            })
        }

        fn matches(&self, name: &str) -> bool {
            if let Some(rest) = name.strip_prefix(self.text.as_str())
                && (rest.is_empty() || rest.starts_with('/') || self.text.ends_with('/'))
            {
                return true;
            }
            self.glob.as_ref().is_some_and(|g| g.is_match(name))
        }
    }

    struct Listed {
        name: String,
        id: gix::ObjectId,
        symref: Option<String>,
    }

    /// Which refs `--merged=<commit>` and `--contains=<commit>` keep.
    enum Filter<'a> {
        /// Whose tip `commit` reaches.
        Merged(&'a str),
        /// Whose tip reaches `commit`.
        Contains(&'a str),
    }

    pub(super) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
        let mut pieces = None;
        let mut sort = None;
        let mut patterns = Vec::new();
        let mut filters = Vec::new();
        for arg in args {
            if let Some(rev) = arg.strip_prefix("--merged=") {
                filters.push(Filter::Merged(rev));
            } else if let Some(rev) = arg.strip_prefix("--contains=") {
                filters.push(Filter::Contains(rev));
            } else if let Some(f) = arg.strip_prefix("--format=") {
                if pieces.is_some() {
                    return None;
                }
                pieces = Some(format(f)?);
            } else if let Some(key) = arg.strip_prefix("--sort=") {
                if sort.is_some() {
                    return None;
                }
                sort = Some(match key {
                    "committerdate" => false,
                    "-committerdate" => true,
                    _ => return None,
                });
            } else if arg.starts_with('-') {
                return None;
            } else {
                patterns.push(Pattern::new(arg)?);
            }
        }
        let pieces = pieces?;
        let repo = open(path)?;
        plain(&repo)?;
        // A linked worktree has refs of its own besides the shared ones.
        if repo.git_dir() != repo.common_dir() {
            return None;
        }
        let platform = repo.references().ok()?;
        let mut listed = Vec::new();
        for reference in platform.all().ok()? {
            let reference = reference.ok()?;
            let name = reference.name().as_bstr().to_str().ok()?.to_owned();
            if !patterns.is_empty() && !patterns.iter().any(|p| p.matches(&name)) {
                continue;
            }
            let symref = match reference.target() {
                gix::refs::TargetRef::Symbolic(target) => {
                    Some(target.as_bstr().to_str().ok()?.to_owned())
                }
                gix::refs::TargetRef::Object(_) => None,
            };
            // A dangling symbolic ref is skipped by git with a warning.
            let id = target_id(&repo, &reference).ok()?;
            listed.push(Listed { name, id, symref });
        }
        listed.sort_by(|a, b| a.name.as_bytes().cmp(b.name.as_bytes()));
        if filters.len() > 1 {
            return None;
        }
        if let Some(filter) = filters.first() {
            let (Filter::Merged(rev) | Filter::Contains(rev)) = filter;
            let commit = resolve(&repo, &format!("{rev}^{{commit}}")).ok()??;
            let cache = repo.commit_graph_if_enabled().ok()?;
            let mut graph = repo.revision_graph(cache.as_ref());
            let mut kept = Vec::new();
            for item in listed {
                // Tips that aren't commits (a tag of a tree…) are never listed.
                let Some(tip) = peel_to(&repo, item.id, gix::object::Kind::Commit).ok()? else {
                    continue;
                };
                let (ancestor, descendant) = match filter {
                    Filter::Merged(_) => (tip, commit),
                    Filter::Contains(_) => (commit, tip),
                };
                let reaches = ancestor == descendant
                    || repo
                        .merge_base_with_graph(ancestor, descendant, &mut graph)
                        .ok()?
                        .is_some_and(|base| base == ancestor);
                if reaches {
                    kept.push(item);
                }
            }
            listed = kept;
        }
        if let Some(descending) = sort {
            let mut dated = Vec::new();
            for item in listed {
                let object = repo.find_object(item.id).ok()?;
                if object.kind != gix::object::Kind::Commit {
                    return None;
                }
                let commit =
                    gix::objs::CommitRef::from_bytes(&object.data, repo.object_hash()).ok()?;
                let seconds = commit.committer().ok()?.time().ok()?.seconds;
                dated.push((seconds, item));
            }
            dated.sort_by_key(|(seconds, _)| *seconds);
            // Which of two refs of the same second git lists first is left to it.
            if dated.windows(2).any(|w| w[0].0 == w[1].0) {
                return None;
            }
            if descending {
                dated.reverse();
            }
            listed = dated.into_iter().map(|(_, item)| item).collect();
        }
        let needs_object = pieces.iter().any(|p| {
            matches!(
                p,
                Piece::ObjectType
                    | Piece::Parent
                    | Piece::CreatorDate
                    | Piece::CommitterDate
                    | Piece::Contents
            )
        });
        let mut out = Vec::new();
        for item in &listed {
            let object = if needs_object {
                Some(repo.find_object(item.id).ok()?)
            } else {
                None
            };
            for piece in &pieces {
                match piece {
                    Piece::Text(text) => out.extend_from_slice(text),
                    Piece::RefName => out.extend_from_slice(item.name.as_bytes()),
                    Piece::ObjectName => out.extend_from_slice(item.id.to_string().as_bytes()),
                    Piece::SymRef => {
                        out.extend_from_slice(item.symref.as_deref().unwrap_or("").as_bytes());
                    }
                    piece => {
                        let object = object.as_ref()?;
                        field(&repo, object, piece, &mut out)?;
                    }
                }
            }
            out.push(b'\n');
        }
        Some(Answer::out(out))
    }

    /// What an atom about the object itself prints.
    fn field(
        repo: &gix::Repository,
        object: &gix::Object<'_>,
        piece: &Piece,
        out: &mut Vec<u8>,
    ) -> Option<()> {
        use gix::object::Kind;
        if let Piece::ObjectType = piece {
            out.extend_from_slice(object.kind.as_bytes());
            return Some(());
        }
        match object.kind {
            Kind::Commit => {
                let commit =
                    gix::objs::CommitRef::from_bytes(&object.data, repo.object_hash()).ok()?;
                match piece {
                    Piece::Parent => {
                        let parents: Vec<String> =
                            commit.parents().map(|p| p.to_string()).collect();
                        out.extend_from_slice(parents.join(" ").as_bytes());
                    }
                    Piece::CreatorDate | Piece::CommitterDate => {
                        let seconds = commit.committer().ok()?.time().ok()?.seconds;
                        out.extend_from_slice(seconds.to_string().as_bytes());
                    }
                    Piece::Contents => out.extend_from_slice(commit.message),
                    _ => return None,
                }
            }
            Kind::Tag => {
                let tag = gix::objs::TagRef::from_bytes(&object.data, repo.object_hash()).ok()?;
                match piece {
                    Piece::Parent | Piece::CommitterDate => {}
                    Piece::CreatorDate => {
                        if let Some(tagger) = tag.tagger().ok()? {
                            let seconds = tagger.time().ok()?.seconds;
                            out.extend_from_slice(seconds.to_string().as_bytes());
                        }
                    }
                    // A tag's signature is part of its contents or not depending on the atom.
                    _ => return None,
                }
            }
            Kind::Blob | Kind::Tree => match piece {
                Piece::Parent | Piece::CreatorDate | Piece::CommitterDate => {}
                _ => return None,
            },
        }
        Some(())
    }
}

#[cfg(test)]
#[path = "answer_tests.rs"]
mod tests;
