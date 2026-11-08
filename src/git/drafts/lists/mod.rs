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
    #[serde(default)]
    access: native_files::Policies,
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
    pub(super) policies: Arc<native_files::Policies>,
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
        let row = std::str::from_utf8(row)
            .map_err(|_| crate::i18n::text("backend-changelist-paths-must-be-utf-8-4d79f3"))?;
        let (info, path) = row
            .split_once('\t')
            .ok_or(crate::i18n::text("backend-invalid-tree-c5c836"))?;
        valid_path(path)?;
        let p = info.split_whitespace().collect::<Vec<_>>();
        if p.len() != 3 {
            return Err(crate::i18n::text("backend-invalid-tree-entry-8d7b6f").into());
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
    publication::guard(ctx)?;
    let reference = format!("refs/gitcraft/changelists/{}/state", ctx.id);
    if !ctx
        .source(&["symbolic-ref", "-q", &reference], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-changelist-state-must-be-a-direct-application-ref-no-state-write-6592ce",
        )
        .into());
    }
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-list-ref-bd1015"))?
    .trim()
    .to_owned();
    let lists = if old.is_empty() {
        Lists::default()
    } else {
        let spec = format!("{old}:lists.json");
        let size = ctx
            .text(&["cat-file", "-s", &spec], vec![], active)?
            .parse::<usize>()
            .map_err(|_| crate::i18n::text("backend-invalid-lists-size-334524"))?;
        if size > 2 * 1024 * 1024 {
            return Err(
                crate::i18n::text("backend-changelist-registry-exceeds-2-mib-cd36e1").into(),
            );
        }
        serde_json::from_slice(&ctx.run(&["cat-file", "blob", &spec], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-invalid-changelist-registry-8193c3"))?
    };
    Ok((reference, old, lists))
}
/// The allowance is decided before invoking a reader, not after retaining its bytes.
/// Synthetic tests exercise this contract with lengths, without allocating payloads.
pub(super) struct SnapshotBudget {
    remaining: usize,
}
impl SnapshotBudget {
    pub(super) fn from_sizes(sizes: impl IntoIterator<Item = usize>) -> Result<Self, String> {
        let mut out = Self {
            remaining: TOTAL_LIMIT,
        };
        for size in sizes {
            out.remaining = out.remaining.checked_sub(size).ok_or(crate::i18n::text(
                "backend-working-snapshot-exceeds-32-mib-71d863",
            ))?;
        }
        Ok(out)
    }
    pub(super) fn allowance(&self) -> Result<usize, String> {
        if self.remaining == 0 {
            Err(crate::i18n::text(
                "backend-working-snapshot-exhausted-its-32-mib-allowance-before-the-next-f-c0be03",
            )
            .into())
        } else {
            Ok(self.remaining.min(FILE_LIMIT))
        }
    }
    pub(super) fn retain(&mut self, size: usize) -> Result<(), String> {
        if size > self.remaining.min(FILE_LIMIT) {
            return Err(crate::i18n::text(
                "backend-file-reader-exceeded-its-remaining-snapshot-allowance-4576a2",
            )
            .into());
        }
        self.remaining = self.remaining.checked_sub(size).ok_or(crate::i18n::text(
            "backend-working-snapshot-overflow-8e6d8f",
        ))?;
        Ok(())
    }
}
pub fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<View, String> {
    let ctx = Context::new(repo, active)?;
    let (reference, version, lists) = state(&ctx, active)?;
    let now = source(&ctx, active)?;
    let head = entries(
        &ctx,
        now.head.as_deref().ok_or(crate::i18n::text(
            "backend-make-an-initial-commit-before-using-changelists-e503e2",
        ))?,
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
        return Err(crate::i18n::text(
            "backend-changelists-support-up-to-2000-changed-paths-f28952",
        )
        .into());
    }
    let mut work = BTreeMap::new();
    let mut budget = SnapshotBudget::from_sizes([])?;
    for p in &paths {
        super::restore::included(&ctx, &now, p, active)?;
        let mode = now.index.get(p).map_or("100644", |e| e.mode.as_str());
        if !matches!(mode, "100644" | "100755") {
            return Err(crate::i18n::message(
                "backend-changelists-require-regular-files-p-5d56fd",
                &[("pointer", (p).to_string().into())],
            ));
        }
        let limit = budget.allowance()?;
        let data = read_file_limited(&ctx, p, mode, limit, active)?;
        budget.retain(data.as_ref().map_or(0, |(_, b)| b.len()))?;
        work.insert(p.clone(), data);
    }
    if source(&ctx, active)? != now {
        return Err(
            crate::i18n::text("backend-index-moved-while-reading-lists-reload-5ea823").into(),
        );
    }
    Ok(View {
        lists,
        paths: paths.into_iter().collect(),
        reference,
        version,
        source: Arc::new(now),
        policies: Arc::new(native_files::capture_policies(repo, &work, active)?),
        work: Arc::new(work),
        head: Arc::new(head),
    })
}
fn verify(ctx: &Context, view: &View, work: bool, active: &dyn Fn() -> bool) -> Result<(), String> {
    if state(ctx, active)?.1 != view.version || source(ctx, active)? != *view.source {
        return Err(crate::i18n::text(
            "backend-changelists-head-or-staged-contents-changed-reopen-the-lists-f336ff",
        )
        .into());
    }
    if work {
        if native_files::capture_policies(&ctx.repo, &view.work, active)? != *view.policies {
            return Err(crate::i18n::text(
                "backend-working-file-full-access-policy-changed-since-preview-5345af",
            )
            .into());
        }
        for (p, expected) in view.work.iter() {
            if read_file(
                ctx,
                p,
                expected.as_ref().map_or("100644", |(m, _)| m),
                active,
            )? != *expected
            {
                return Err(crate::i18n::message(
                    "backend-p-changed-since-preview-reopen-the-lists-520f7d",
                    &[("pointer", (p).to_string().into())],
                ));
            }
        }
    }
    Ok(())
}
fn state_tree(ctx: &Context, lists: &Lists, active: &dyn Fn() -> bool) -> Result<String, String> {
    if lists.lists.len() > 100 {
        return Err(
            crate::i18n::text("backend-at-most-100-changelists-are-supported-2e4f0a").into(),
        );
    }
    let mut tree = BTreeMap::new();
    let data = serde_json::to_vec(lists).map_err(|e| e.to_string())?;
    if data.len() > 2 * 1024 * 1024 {
        return Err(crate::i18n::text("backend-changelist-metadata-exceeds-2-mib-d8e8dd").into());
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
pub(super) fn direct_update(reference: &str, new: &str, old: &str) -> String {
    format!("option no-deref\nupdate {reference} {new} {old}\n")
}
pub(super) fn direct_verify(reference: &str, old: &str) -> String {
    format!("option no-deref\nverify {reference} {old}\n")
}
pub(super) fn direct_ref_guard<'a>(references: impl IntoIterator<Item = &'a str>) -> String {
    references
        .into_iter()
        .map(|reference| {
            format!(
                "git symbolic-ref -q {} >/dev/null 2>&1; code=$?; [ \"$code\" = 1 ] || exit 1; ",
                remote::sh_quote(reference)
            )
        })
        .collect()
}
fn publish(
    ctx: &Context,
    view: &View,
    lists: &Lists,
    commit: Option<&str>,
    evidence: &str,
    active: &dyn Fn() -> bool,
) -> Result<native_files::CommitOutcome, String> {
    let new = state_tree(ctx, lists, active)?;
    let zero = "0".repeat(new.len());
    let old = if view.version.is_empty() {
        &zero
    } else {
        &view.version
    };
    let branch = view.source.branch.as_deref().ok_or(crate::i18n::text(
        "backend-check-out-a-local-branch-first-5c009c",
    ))?;
    if !branch.starts_with("refs/heads/") {
        return Err(crate::i18n::text(
            "backend-changelist-publication-requires-a-local-branch-4633d4",
        )
        .into());
    }
    let head = view
        .source
        .head
        .as_deref()
        .ok_or(crate::i18n::text("backend-no-initial-commit-df294a"))?;
    let mut tx = "start\n".to_owned();
    tx.push_str(&format!("update HEAD {} {head}\n", commit.unwrap_or(head)));
    tx.push_str(&direct_update(&view.reference, &new, old));
    tx.push_str("prepare\ncommit\n");
    {
        // Git locks symbolic HEAD and its referent natively. The fixed prepared hook
        // binds that referent, so a concurrent checkout cannot redirect the operation.
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = prepared ]; then test \"$(git symbolic-ref -q HEAD)\" = {} || exit 1; {}fi\ncat >/dev/null\n",
            remote::sh_quote(branch),
            direct_ref_guard([view.reference.as_str()])
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
    let pending = publication::begin(
        ctx,
        "changelists",
        &view.reference,
        &view.version,
        &new,
        &view.source,
        evidence,
        active,
    )?;
    pending.append_receipt(&mut tx);
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
    Ok(pending.finish(ctx, result))
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
                return Err(crate::i18n::text(
                    "backend-use-a-name-up-to-128-bytes-and-message-up-to-64-kib-dfaeda",
                )
                .into());
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
                return Err(crate::i18n::text("backend-message-exceeds-64-kib-8bc64e").into());
            }
            lists
                .lists
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or(crate::i18n::text("backend-list-disappeared-27ce5f"))?
                .message = message;
        }
        Edit::Rename(id, name) => {
            if name.trim().is_empty() || name.len() > 128 {
                return Err(crate::i18n::text("backend-use-a-name-up-to-128-bytes-dda20c").into());
            }
            lists
                .lists
                .iter_mut()
                .find(|l| l.id == id)
                .ok_or(crate::i18n::text("backend-list-disappeared-27ce5f"))?
                .name = name;
        }
        Edit::Assign(id, path) => {
            if !view.paths.contains(&path) {
                return Err(
                    crate::i18n::text("backend-path-is-not-in-this-snapshot-d80a18").into(),
                );
            }
            if !id.is_empty() && lists.lists.iter().all(|l| l.id != id || l.shelf.is_some()) {
                return Err(crate::i18n::text("backend-choose-an-active-list-be27a5").into());
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
    publish(&ctx, view, &lists, None, "", active)?.confirmed()?;
    Ok(crate::i18n::text("backend-changelist-saved-f554ef").into())
}
pub(super) fn data(
    ctx: &Context,
    entries: &BTreeMap<String, Option<Entry>>,
    active: &dyn Fn() -> bool,
) -> Result<Contents, String> {
    if entries.len() > MAX_FILES {
        return Err(crate::i18n::text("backend-snapshot-exceeds-2000-paths-daa6c1").into());
    }
    let mut budget = SnapshotBudget::from_sizes([])?;
    entries
        .iter()
        .map(|(p, e)| {
            let bytes = if let Some(e) = e {
                if !matches!(e.mode.as_str(), "100644" | "100755") {
                    return Err(crate::i18n::text("backend-only-regular-changelist-files-are-supported-552983").into());
                }
                let size = ctx
                    .text(&["cat-file", "-s", &e.oid], vec![], active)?
                    .parse::<usize>()
                    .map_err(|_| crate::i18n::text("backend-invalid-blob-0dd843"))?;
                if size > budget.allowance()? {
                    return Err(crate::i18n::text("backend-changelist-blob-exceeds-the-remaining-32-mib-8-mib-per-file-allow-8db5e7").into());
                }
                let bytes = ctx.run(&["cat-file", "blob", &e.oid], vec![], &[0], active)?;
                if bytes.len() != size {
                    return Err(crate::i18n::text("backend-snapshot-blob-size-changed-be9c65").into());
                }
                budget.retain(bytes.len())?;
                Some((e.mode.clone(), bytes))
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
fn selected_index_records(selected: &BTreeMap<String, Option<Entry>>, oid_len: usize) -> Vec<u8> {
    selected
        .iter()
        .flat_map(|(path, entry)| match entry {
            Some(entry) => format!("{} {}\t{path}\0", entry.mode, entry.oid).into_bytes(),
            None => format!("0 {}\t{path}\0", "0".repeat(oid_len)).into_bytes(),
        })
        .collect()
}
fn final_index(
    ctx: &Context,
    source: &Source,
    selected: &BTreeMap<String, Option<Entry>>,
    active: &dyn Fn() -> bool,
) -> Result<Vec<u8>, String> {
    // Start with the entire captured cache, including intent-to-add and unrelated
    // entry flags. Tree reconstruction cannot represent those semantics.
    private_index_file(ctx, "index", &ctx.index_file, &source.cache, active)?;
    if let Some((name, bytes)) = &source.shared {
        private_index_file(ctx, name, &ctx.shared_file, bytes, active)?;
    }
    if ctx.run(&["ls-files", "-v", "--stage", "-z"], vec![], &[0], active)? != source.raw {
        return Err(crate::i18n::text(
            "backend-captured-index-could-not-be-reproduced-no-public-index-was-change-526a6b",
        )
        .into());
    }
    // Materialize split entries in the private cache; transient daemon caches must
    // not be published. Git rejects index formats/extensions it cannot interpret.
    ctx.run(
        &[
            "update-index",
            "--no-fsmonitor",
            "--no-untracked-cache",
            "--no-split-index",
        ],
        vec![],
        &[0],
        active,
    )?;
    let input = selected_index_records(
        selected,
        source
            .head
            .as_ref()
            .ok_or(crate::i18n::text("backend-no-head-f43258"))?
            .len(),
    );
    if !input.is_empty() {
        ctx.run(&["update-index", "-z", "--index-info"], input, &[0], active)?;
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
        return Err(crate::i18n::text(
            "backend-changelist-commit-requires-a-native-checkout-remote-files-were-no-245a50",
        )
        .into());
    }
    let ctx = Context::new(repo, active)?;
    verify(&ctx, view, true, active)?;
    let list = view
        .lists
        .lists
        .iter()
        .find(|l| l.id == id)
        .ok_or(crate::i18n::text("backend-list-disappeared-27ce5f"))?;
    if list.shelf.is_some() || list.paths.is_empty() || list.message.trim().is_empty() {
        return Err(crate::i18n::text(
            "backend-use-an-active-nonempty-list-and-draft-commit-message-345cd8",
        )
        .into());
    }
    let work = list
        .paths
        .iter()
        .map(|p| {
            Ok((
                p.clone(),
                view.work
                    .get(p)
                    .ok_or(crate::i18n::text(
                        "backend-path-missing-from-snapshot-dfcb5f",
                    ))?
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
                &format!(
                    "{}^{{tree}}",
                    view.source
                        .head
                        .as_deref()
                        .ok_or(crate::i18n::text("backend-no-head-f43258"))?
                ),
            ],
            vec![],
            active,
        )?
    {
        return Err(crate::i18n::text("backend-list-has-no-changes-against-head-b979cc").into());
    }
    let name =
        String::from_utf8(ctx.source(&["config", "--get", "user.name"], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-invalid-author-96cc99"))?;
    let email =
        String::from_utf8(ctx.source(&["config", "--get", "user.email"], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-invalid-author-96cc99"))?;
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
            view.source
                .head
                .as_deref()
                .ok_or(crate::i18n::text("backend-no-head-f43258"))?,
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
        .ok_or(crate::i18n::text("backend-list-disappeared-27ce5f"))?
        .paths
        .clear();
    capture_preserving_paths(
        repo,
        crate::i18n::text("backend-before-committing-a-changelist-f40288"),
        None,
        &list.paths.iter().cloned().collect::<Vec<_>>(),
        active,
    )?;
    verify(&ctx, view, true, active)?;
    let mut index = index::Index::new(&ctx, &view.source, bytes)?;
    match publish(&ctx, view, &lists, Some(&new), &index.evidence(), active) {
        Ok(native_files::CommitOutcome::Committed) => {
            let evidence=index.accept().map_err(|e|crate::i18n::message("backend-refs-committed-index-release-needs-attention-and-no-rollback-was-f6b682", &[("detail", (e).to_string().into())]))?;
            Ok(crate::i18n::message(
                "backend-committed-only-as-new-other-lists-and-staged-contents-retained-in-d6a6b8",
                &[
                    ("name", (list.name).to_string().into()),
                    ("new", (new).to_string().into()),
                    ("evidence", (evidence).to_string().into()),
                ],
            ))
        }
        Ok(native_files::CommitOutcome::Unknown(reason)) => {
            Err(format!("{reason}. {}", index.uncertain()))
        }
        Err(error) => {
            let rollback = index.abort();
            Err(crate::i18n::message(
                "backend-error-index-abort-rollback-db7f35",
                &[
                    ("detail", (error).to_string().into()),
                    ("rollback", format!("{:?}", rollback).into()),
                ],
            ))
        }
    }
}

fn require_restore_ownership(lists: &Lists, restoring: &List) -> Result<(), String> {
    if let Some((other, path)) = lists
        .lists
        .iter()
        .filter(|other| other.id != restoring.id && other.shelf.is_none())
        .find_map(|other| {
            other
                .paths
                .intersection(&restoring.paths)
                .next()
                .map(|path| (other, path))
        })
    {
        return Err(crate::i18n::message(
            "backend-cannot-unshelve-path-belongs-to-active-changelist-release-that-ow-dd2312",
            &[
                ("name", (restoring.name).to_string().into()),
                ("path", (path).to_string().into()),
                ("name3", (other.name).to_string().into()),
            ],
        ));
    }
    Ok(())
}

pub fn shelve(
    repo: &Path,
    view: &View,
    id: &str,
    restore: bool,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-changelist-shelving-requires-a-native-checkout-remote-files-were-7c1c58",
        )
        .into());
    }
    let ctx = Context::new(repo, active)?;
    verify(&ctx, view, true, active)?;
    let list = view
        .lists
        .lists
        .iter()
        .find(|l| l.id == id)
        .ok_or(crate::i18n::text("backend-list-disappeared-27ce5f"))?;
    if list.paths.is_empty() {
        return Err(crate::i18n::text("backend-choose-a-nonempty-changelist-9521ca").into());
    }
    if restore {
        require_restore_ownership(&view.lists, list)?;
    }
    let current = list
        .paths
        .iter()
        .map(|p| {
            Ok((
                p.clone(),
                view.work
                    .get(p)
                    .ok_or(crate::i18n::text(
                        "backend-path-missing-from-snapshot-dfcb5f",
                    ))?
                    .clone(),
            ))
        })
        .collect::<Result<Contents, String>>()?;
    let mut lists = view.lists.clone();
    let owned = lists
        .lists
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or(crate::i18n::text("backend-list-disappeared-27ce5f"))?;
    let (target, next_index) = if restore {
        let shelf = list
            .shelf
            .as_ref()
            .ok_or(crate::i18n::text("backend-list-is-not-shelved-a61b98"))?;
        let base = data(&ctx, &shelf.base, active)?;
        if current != base
            || shelf
                .base
                .iter()
                .any(|(p, e)| view.source.index.get(p) != e.as_ref())
        {
            return Err(crate::i18n::text(
                "backend-shelved-paths-have-new-staged-or-working-edits-resolve-those-sepa-399352",
            )
            .into());
        }
        let target = data(&ctx, &shelf.work, active)?;
        owned.shelf = None;
        (target, shelf.index.clone())
    } else {
        if list.shelf.is_some() {
            return Err(crate::i18n::text("backend-list-is-already-shelved-898c2c").into());
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
            access: view
                .policies
                .iter()
                .filter(|(p, _)| list.paths.contains(*p))
                .map(|(p, policy)| (p.clone(), policy.clone()))
                .collect(),
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
            crate::i18n::text("backend-before-unshelving-one-changelist-32b46b")
        } else {
            crate::i18n::text("backend-before-shelving-one-changelist-f97389")
        },
        None,
        &list.paths.iter().cloned().collect::<Vec<_>>(),
        active,
    )?;
    verify(&ctx, view, true, active)?;
    let check = || verify(&ctx, view, false, active);
    let commit = |working_evidence: &str| {
        let mut index = index::Index::new(&ctx, &view.source, bytes.clone())?;
        let evidence = format!("{working_evidence}; index: {}", index.evidence());
        match publish(&ctx, view, &lists, None, &evidence, active) {
            Ok(native_files::CommitOutcome::Committed) => match index.accept() {
                Ok(_) => Ok(native_files::CommitOutcome::Committed),
                Err(error) => Ok(native_files::CommitOutcome::Unknown(crate::i18n::message(
                    "backend-refs-committed-but-index-release-needs-attention-error-6be47a",
                    &[
                        ("detail", (error).to_string().into()),
                        ("index", (index.uncertain()).to_string().into()),
                    ],
                ))),
            },
            Ok(native_files::CommitOutcome::Unknown(reason)) => Ok(
                native_files::CommitOutcome::Unknown(format!("{reason}; {}", index.uncertain())),
            ),
            Err(error) => {
                let rollback = index.abort();
                Err(crate::i18n::message(
                    "backend-error-index-abort-rollback-db7f35",
                    &[
                        ("detail", (error).to_string().into()),
                        ("rollback", format!("{:?}", rollback).into()),
                    ],
                ))
            }
        }
    };
    let evidence = working_transaction::apply_with_commit(
        repo,
        &current,
        &target,
        &view.policies,
        &check,
        &commit,
        if restore {
            list.shelf.as_ref().map(|s| &s.access)
        } else {
            None
        },
        active,
    )?;
    Ok(crate::i18n::message(
        "backend-other-lists-retained-evidence-cab32f",
        &[
            (
                "if",
                (if restore { "Unshelved" } else { "Shelved" })
                    .to_string()
                    .into(),
            ),
            ("name", (list.name).to_string().into()),
            ("evidence", (evidence).to_string().into()),
        ],
    ))
}
