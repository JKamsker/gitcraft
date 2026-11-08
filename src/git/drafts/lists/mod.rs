//! File changelists keep independent messages and raw staged/work layers. Publication
//! uses a private index, an owned native index exchange, and one HEAD/state ref transaction.
mod index;
#[cfg(test)]
mod tests;
use super::working_transaction::Contents;
use super::*;
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Lists {
    pub lists: Vec<List>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct List {
    pub id: String,
    pub name: String,
    pub message: String,
    pub paths: BTreeSet<String>,
    pub shelf: Option<Shelf>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Shelf {
    permissions: BTreeMap<String, u32>,
    base: BTreeMap<String, Option<Entry>>,
    index: BTreeMap<String, Option<Entry>>,
    work: BTreeMap<String, Option<Entry>>,
}
#[derive(Clone)]
pub struct View {
    pub lists: Lists,
    pub paths: Vec<String>,
    pub version: String,
    pub reference: String,
    pub(super) source: Arc<Source>,
    pub(super) work: Arc<Contents>,
    pub(super) head: Arc<BTreeMap<String, Entry>>,
}
pub(super) fn entries(
    ctx: &Context,
    rev: &str,
    active: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Entry>, String> {
    let raw = ctx.run(&["ls-tree", "-rz", rev], vec![], &[0], active)?;
    let mut out = BTreeMap::new();
    for row in raw.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let row = std::str::from_utf8(row).map_err(|_| "Changelist paths must be UTF-8")?;
        let (info, path) = row.split_once('\t').ok_or("Invalid tree")?;
        valid_path(path)?;
        let p = info.split_whitespace().collect::<Vec<_>>();
        if p.len() != 3 {
            return Err("Invalid tree entry".into());
        }
        out.insert(
            path.into(),
            Entry {
                mode: p[0].into(),
                oid: p[2].into(),
            },
        );
        if out.len() > MAX_INDEX {
            return Err("Tree exceeds 20000 paths".into());
        }
    }
    Ok(out)
}
fn state(ctx: &Context, active: &dyn Fn() -> bool) -> Result<(String, String, Lists), String> {
    let reference = format!("refs/gitcraft/changelists/{}/state", ctx.id);
    if !ctx
        .source(&["symbolic-ref", "-q", &reference], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err(
            "Changelist state must be a direct application ref; no state write was attempted"
                .into(),
        );
    }
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| "Invalid list ref")?
    .trim()
    .to_owned();
    let lists = if old.is_empty() {
        Lists::default()
    } else {
        let spec = format!("{old}:lists.json");
        let size = ctx
            .text(&["cat-file", "-s", &spec], vec![], active)?
            .parse::<usize>()
            .map_err(|_| "Invalid lists size")?;
        if size > 2 * 1024 * 1024 {
            return Err("Changelist registry exceeds 2 MiB".into());
        }
        serde_json::from_slice(&ctx.run(&["cat-file", "blob", &spec], vec![], &[0], active)?)
            .map_err(|_| "Invalid changelist registry")?
    };
    Ok((reference, old, lists))
}
pub fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<View, String> {
    let ctx = Context::new(repo, active)?;
    let (reference, version, lists) = state(&ctx, active)?;
    let now = source(&ctx, active)?;
    let head = entries(
        &ctx,
        now.head
            .as_deref()
            .ok_or("Make an initial commit before using changelists")?,
        active,
    )?;
    let index_tree = tree(&ctx, &now.index, active)?;
    cached_index(&ctx, &now, active)?;
    let mut paths = changed(&ctx, &now, &index_tree, active)?;
    paths.extend(
        now.index
            .keys()
            .chain(head.keys())
            .filter(|p| now.index.get(*p) != head.get(*p))
            .cloned(),
    );
    paths.extend(untracked_paths(ctx.source(
        &["ls-files", "--others", "--exclude-standard", "-z"],
        vec![],
        &[0],
        active,
    )?)?);
    for list in &lists.lists {
        paths.extend(list.paths.iter().cloned());
    }
    if paths.len() > MAX_FILES {
        return Err("Changelists support up to 2000 changed paths".into());
    }
    let mut work = BTreeMap::new();
    let mut total = 0;
    for p in &paths {
        super::restore::included(&ctx, &now, p, active)?;
        let mode = now.index.get(p).map_or("100644", |e| e.mode.as_str());
        if !matches!(mode, "100644" | "100755") {
            return Err(format!("Changelists require regular files: {p}"));
        }
        let data = read_file(&ctx, p, mode, active)?;
        total += data.as_ref().map_or(0, |(_, b)| b.len());
        work.insert(p.clone(), data);
    }
    if total > TOTAL_LIMIT {
        return Err("Changelists exceed 32 MiB".into());
    }
    if source(&ctx, active)? != now {
        return Err("Index moved while reading lists; reload".into());
    }
    Ok(View {
        lists,
        paths: paths.into_iter().collect(),
        reference,
        version,
        source: Arc::new(now),
        work: Arc::new(work),
        head: Arc::new(head),
    })
}
fn verify(ctx: &Context, view: &View, work: bool, active: &dyn Fn() -> bool) -> Result<(), String> {
    if state(ctx, active)?.1 != view.version || source(ctx, active)? != *view.source {
        return Err("Changelists, HEAD or staged contents changed; reopen the lists".into());
    }
    if work {
        for (p, expected) in view.work.iter() {
            if read_file(
                ctx,
                p,
                expected.as_ref().map_or("100644", |(m, _)| m),
                active,
            )? != *expected
            {
                return Err(format!("{p} changed since preview; reopen the lists"));
            }
        }
    }
    Ok(())
}
fn state_tree(ctx: &Context, lists: &Lists, active: &dyn Fn() -> bool) -> Result<String, String> {
    if lists.lists.len() > 100 {
        return Err("At most 100 changelists are supported".into());
    }
    let mut tree = BTreeMap::new();
    let data = serde_json::to_vec(lists).map_err(|e| e.to_string())?;
    if data.len() > 2 * 1024 * 1024 {
        return Err("Changelist metadata exceeds 2 MiB".into());
    }
    tree.insert(
        "lists.json".into(),
        Entry {
            mode: "100644".into(),
            oid: ctx.blob(data, active)?,
        },
    );
    let mut n = 0;
    for l in &lists.lists {
        if let Some(s) = &l.shelf {
            for e in s
                .base
                .values()
                .chain(s.index.values())
                .chain(s.work.values())
                .flatten()
            {
                tree.insert(format!("keep-{n}"), e.clone());
                n += 1;
            }
        }
    }
    super::tree(ctx, &tree, active)
}
fn publish(
    ctx: &Context,
    view: &View,
    lists: &Lists,
    commit: Option<&str>,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    let new = state_tree(ctx, lists, active)?;
    let zero = "0".repeat(new.len());
    let old = if view.version.is_empty() {
        &zero
    } else {
        &view.version
    };
    let branch = view
        .source
        .branch
        .as_deref()
        .ok_or("Check out a local branch first")?;
    if !branch.starts_with("refs/heads/") {
        return Err("Changelist publication requires a local branch".into());
    }
    let head = view.source.head.as_deref().ok_or("No initial commit")?;
    let mut tx = "start\n".to_owned();
    tx.push_str(&format!("update HEAD {} {head}\n", commit.unwrap_or(head)));
    tx.push_str(&format!(
        "update {} {new} {old}\nprepare\ncommit\n",
        view.reference
    ));
    {
        // Git locks symbolic HEAD and its referent natively. The fixed prepared hook
        // binds that referent, so a concurrent checkout cannot redirect the operation.
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = prepared ]; then test \"$(git symbolic-ref -q HEAD)\" = {} || exit 1; fi\ncat >/dev/null\n",
            remote::sh_quote(branch)
        );
        let path = ctx.path.join("hooks/reference-transaction");
        std::fs::write(&path, script).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
    }
    let result = ctx.source(
        &[
            "update-ref",
            "-m",
            "gitcraft changelist publication",
            "--stdin",
        ],
        tx.into_bytes(),
        &[0],
        active,
    );
    // A cancelled child can have committed before its acknowledgement was lost.
    // Inspect the exact unique state object before permitting an index rollback.
    match result {
        Ok(_) => Ok(()),
        Err(e) => {
            if state(ctx, &|| true)?.1 == new {
                Ok(())
            } else {
                Err(e)
            }
        }
    }
}
pub enum Edit {
    Create(String, String),
    Message(String, String),
    Assign(String, String),
    Rename(String, String),
}
pub fn edit(
    repo: &Path,
    view: &View,
    edit: Edit,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    let ctx = Context::new(repo, active)?;
    verify(&ctx, view, false, active)?;
    let mut lists = view.lists.clone();
    match edit {
        Edit::Create(name, message) => {
            if name.trim().is_empty() || name.len() > 128 || message.len() > 64 * 1024 {
                return Err("Use a name up to 128 bytes and message up to 64 KiB".into());
            }
            static IDS: AtomicU64 = AtomicU64::new(1);
            lists.lists.push(List {
                id: format!(
                    "{}-{}-{}",
                    git::now(),
                    std::process::id(),
                    IDS.fetch_add(1, Ordering::Relaxed)
                ),
                name,
                message,
                paths: BTreeSet::new(),
                shelf: None,
            });
        }
        Edit::Message(id, message) => {
            if message.len() > 64 * 1024 {
                return Err("Message exceeds 64 KiB".into());
            }
            lists
                .lists
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or("List disappeared")?
                .message = message;
        }
        Edit::Rename(id, name) => {
            if name.trim().is_empty() || name.len() > 128 {
                return Err("Use a name up to 128 bytes".into());
            }
            lists
                .lists
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or("List disappeared")?
                .name = name;
        }
        Edit::Assign(id, path) => {
            if !view.paths.contains(&path) {
                return Err("Path is not in this snapshot".into());
            }
            if !id.is_empty() && lists.lists.iter().all(|l| l.id != id || l.shelf.is_some()) {
                return Err("Choose an active list".into());
            }
            for list in &mut lists.lists {
                if list.shelf.is_none() {
                    list.paths.remove(&path);
                    if list.id == id {
                        list.paths.insert(path.clone());
                    }
                }
            }
        }
    }
    publish(&ctx, view, &lists, None, active)?;
    Ok("Changelist saved".into())
}
pub(super) fn data(
    ctx: &Context,
    entries: &BTreeMap<String, Option<Entry>>,
    active: &dyn Fn() -> bool,
) -> Result<Contents, String> {
    entries
        .iter()
        .map(|(p, e)| {
            let bytes = if let Some(e) = e {
                if !matches!(e.mode.as_str(), "100644" | "100755") {
                    return Err("Only regular changelist files are supported".into());
                }
                let size = ctx
                    .text(&["cat-file", "-s", &e.oid], vec![], active)?
                    .parse::<usize>()
                    .map_err(|_| "Invalid blob")?;
                if size > FILE_LIMIT {
                    return Err("Changelist blob exceeds 8 MiB".into());
                }
                Some((
                    e.mode.clone(),
                    ctx.run(&["cat-file", "blob", &e.oid], vec![], &[0], active)?,
                ))
            } else {
                None
            };
            Ok((p.clone(), bytes))
        })
        .collect()
}
pub(super) fn raw_entries(
    ctx: &Context,
    data: &Contents,
    active: &dyn Fn() -> bool,
) -> Result<BTreeMap<String, Option<Entry>>, String> {
    data.iter()
        .map(|(p, data)| {
            Ok((
                p.clone(),
                match data {
                    Some((mode, bytes)) => Some(Entry {
                        mode: mode.clone(),
                        oid: ctx.blob(bytes.clone(), active)?,
                    }),
                    None => None,
                },
            ))
        })
        .collect()
}
fn final_index(
    ctx: &Context,
    source: &Source,
    selected: &BTreeMap<String, Option<Entry>>,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    let mut next = source.index.clone();
    for (p, e) in selected {
        match e {
            Some(e) => {
                next.insert(p.clone(), e.clone());
            }
            None => {
                next.remove(p);
            }
        }
    }
    tree(ctx, &next, active)?;
    for (flag, paths) in [
        (
            "--skip-worktree",
            source.skip.iter().cloned().collect::<Vec<_>>(),
        ),
        (
            "--assume-unchanged",
            source
                .raw
                .split(|b| *b == 0)
                .filter(|r| r.first().is_some_and(u8::is_ascii_lowercase))
                .filter_map(|r| {
                    std::str::from_utf8(r)
                        .ok()?
                        .split_once('\t')
                        .map(|(_, p)| p.to_owned())
                })
                .collect(),
        ),
    ] {
        let input = paths
            .into_iter()
            .filter(|p| !selected.contains_key(p) && next.contains_key(p))
            .flat_map(|p| p.into_bytes().into_iter().chain([0]))
            .collect::<Vec<_>>();
        if !input.is_empty() {
            ctx.run(
                &["update-index", flag, "-z", "--stdin"],
                input,
                &[0],
                active,
            )?;
        }
    }
    std::fs::read(ctx.path.join("index")).map_err(|e| e.to_string())
}
pub fn commit(
    repo: &Path,
    view: &View,
    id: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(
            "Changelist commit requires a native checkout; remote files were not changed".into(),
        );
    }
    let ctx = Context::new(repo, active)?;
    verify(&ctx, view, true, active)?;
    let list = view
        .lists
        .lists
        .iter()
        .find(|l| l.id == id)
        .ok_or("List disappeared")?;
    if list.shelf.is_some() || list.paths.is_empty() || list.message.trim().is_empty() {
        return Err("Use an active nonempty list and draft commit message".into());
    }
    let work = list
        .paths
        .iter()
        .map(|p| {
            Ok((
                p.clone(),
                view.work
                    .get(p)
                    .ok_or("Path missing from snapshot")?
                    .clone(),
            ))
        })
        .collect::<Result<Contents, String>>()?;
    let selected = raw_entries(&ctx, &work, active)?;
    let mut commit_entries = view.head.as_ref().clone();
    for (p, e) in &selected {
        match e {
            Some(e) => {
                commit_entries.insert(p.clone(), e.clone());
            }
            None => {
                commit_entries.remove(p);
            }
        }
    }
    let tree = tree(&ctx, &commit_entries, active)?;
    if tree
        == ctx.text(
            &[
                "rev-parse",
                &format!("{}^{{tree}}", view.source.head.as_deref().ok_or("No HEAD")?),
            ],
            vec![],
            active,
        )?
    {
        return Err("List has no changes against HEAD".into());
    }
    let name =
        String::from_utf8(ctx.source(&["config", "--get", "user.name"], vec![], &[0], active)?)
            .map_err(|_| "Invalid author")?;
    let email =
        String::from_utf8(ctx.source(&["config", "--get", "user.email"], vec![], &[0], active)?)
            .map_err(|_| "Invalid author")?;
    let name = format!("user.name={}", name.trim());
    let email = format!("user.email={}", email.trim());
    let new = ctx.text(
        &[
            "-c",
            &name,
            "-c",
            &email,
            "commit-tree",
            &tree,
            "-p",
            view.source.head.as_deref().ok_or("No HEAD")?,
        ],
        list.message.as_bytes().to_vec(),
        active,
    )?;
    let bytes = final_index(&ctx, &view.source, &selected, active)?;
    // The state object builder also uses the private index, so snapshot final bytes first.
    let mut lists = view.lists.clone();
    lists
        .lists
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or("List disappeared")?
        .paths
        .clear();
    capture_preserving_paths(
        repo,
        "Before committing a changelist",
        None,
        &list.paths.iter().cloned().collect::<Vec<_>>(),
        active,
    )?;
    verify(&ctx, view, true, active)?;
    let mut index = index::Index::new(&ctx, &view.source, bytes)?;
    match publish(&ctx, view, &lists, Some(&new), active) {
        Ok(()) => {
            index.accept();
            Ok(format!(
                "Committed only {} as {new}; other lists and staged contents retained",
                list.name
            ))
        }
        Err(e) => {
            let head = ctx.source(&["rev-parse", "HEAD"], vec![], &[0], &|| true)?;
            if head != format!("{}\n", view.source.head.as_deref().unwrap_or_default()).as_bytes() {
                index.uncertain();
                return Err(format!(
                    "Publication outcome needs inspection; index.lock retained: {e}"
                ));
            }
            Err(e)
        }
    }
}

pub fn shelve(
    repo: &Path,
    view: &View,
    id: &str,
    restore: bool,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(
            "Changelist shelving requires a native checkout; remote files were not changed".into(),
        );
    }
    let ctx = Context::new(repo, active)?;
    verify(&ctx, view, true, active)?;
    let list = view
        .lists
        .lists
        .iter()
        .find(|l| l.id == id)
        .ok_or("List disappeared")?;
    if list.paths.is_empty() {
        return Err("Choose a nonempty changelist".into());
    }
    let current = list
        .paths
        .iter()
        .map(|p| {
            Ok((
                p.clone(),
                view.work
                    .get(p)
                    .ok_or("Path missing from snapshot")?
                    .clone(),
            ))
        })
        .collect::<Result<Contents, String>>()?;
    let mut lists = view.lists.clone();
    let owned = lists
        .lists
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or("List disappeared")?;
    let (target, next_index) = if restore {
        let shelf = list.shelf.as_ref().ok_or("List is not shelved")?;
        let base = data(&ctx, &shelf.base, active)?;
        if current != base
            || shelf
                .base
                .iter()
                .any(|(p, e)| view.source.index.get(p) != e.as_ref())
        {
            return Err("Shelved paths have new staged or working edits. Resolve those separately; nothing was overwritten".into());
        }
        let target = data(&ctx, &shelf.work, active)?;
        owned.shelf = None;
        (target, shelf.index.clone())
    } else {
        if list.shelf.is_some() {
            return Err("List is already shelved".into());
        }
        let base = list
            .paths
            .iter()
            .map(|p| (p.clone(), view.head.get(p).cloned()))
            .collect::<BTreeMap<_, _>>();
        let index = list
            .paths
            .iter()
            .map(|p| (p.clone(), view.source.index.get(p).cloned()))
            .collect();
        let work = raw_entries(&ctx, &current, active)?;
        let permissions = list
            .paths
            .iter()
            .filter(|p| current.get(*p).is_some_and(Option::is_some))
            .map(|p| {
                Ok((
                    p.clone(),
                    super::permissions::Pin::capture(repo, p, active)?.bits(),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, String>>()?;
        owned.shelf = Some(Shelf {
            permissions,
            base: base.clone(),
            index,
            work,
        });
        (data(&ctx, &base, active)?, base)
    };
    let bytes = final_index(&ctx, &view.source, &next_index, active)?;
    capture_preserving_paths(
        repo,
        if restore {
            "Before unshelving one changelist"
        } else {
            "Before shelving one changelist"
        },
        None,
        &list.paths.iter().cloned().collect::<Vec<_>>(),
        active,
    )?;
    verify(&ctx, view, true, active)?;
    let check = || verify(&ctx, view, false, active);
    let commit = || {
        let mut index = index::Index::new(&ctx, &view.source, bytes.clone())?;
        match publish(&ctx, view, &lists, None, active) {
            Ok(()) => {
                index.accept();
                Ok(())
            }
            Err(e) => Err(e),
        }
    };
    working_transaction::apply_with_commit(
        repo,
        &current,
        &target,
        &check,
        &commit,
        if restore {
            list.shelf.as_ref().map(|s| &s.permissions)
        } else {
            None
        },
        active,
    )?;
    Ok(format!(
        "{} {}; other lists retained",
        if restore { "Unshelved" } else { "Shelved" },
        list.name
    ))
}
