//! Parallel real branch refs, one materialized checkout. HEAD remains the anchor; its
//! real index is never staged/reset by this mode. Every hunk has at most one owner.
use super::working_transaction::Contents;
use super::*;
#[cfg(test)]
pub(crate) mod tests;
use crate::agent::bakeoff::hunks;
use std::sync::Arc;
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub mode: Option<Mode>,
    undo: Option<Undo>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Mode {
    pub anchor: String,
    pub base: String,
    pub branches: Vec<Branch>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Branch {
    pub name: String,
    pub tip: String,
    pub active: bool,
    pub message: String,
    pub owned: Vec<Hunk>,
    permissions: BTreeMap<String, u32>,
    #[serde(default)]
    access: native_files::Policies,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Hunk {
    pub key: String,
    pub path: String,
    pub patch: String,
    base: Option<Entry>,
    target: Option<Entry>,
    whole: bool,
}
#[derive(Clone, Serialize, Deserialize)]
struct Undo {
    previous: String,
    moves: Vec<Move>,
    before: BTreeMap<String, Option<Entry>>,
    after: BTreeMap<String, Option<Entry>>,
    permissions: BTreeMap<String, u32>,
    #[serde(default)]
    access: native_files::Policies,
}
#[derive(Clone, Serialize, Deserialize)]
struct Move {
    reference: String,
    old: String,
    new: String,
}
#[derive(Clone)]
pub struct View {
    pub state: State,
    pub hunks: Vec<Hunk>,
    pub stale: Option<String>,
    pub version: String,
    reference: String,
    source: lists::View,
}
fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn state(ctx: &Context, active: &dyn Fn() -> bool) -> Result<(String, String, State), String> {
    publication::guard(ctx)?;
    let reference = format!("refs/gitcraft/virtual-branches/{}/state", ctx.id);
    if !ctx
        .source(&["symbolic-ref", "-q", &reference], vec![], &[0, 1], active)?
        .is_empty()
    {
        return Err(crate::i18n::text(
            "backend-virtual-state-must-be-a-direct-application-ref-dae732",
        )
        .into());
    }
    let old = String::from_utf8(ctx.source(
        &["rev-parse", "--verify", "-q", &reference],
        vec![],
        &[0, 1],
        active,
    )?)
    .map_err(|_| crate::i18n::text("backend-invalid-virtual-state-ref-684762"))?
    .trim()
    .to_owned();
    let value = if old.is_empty() {
        State::default()
    } else {
        load(ctx, &old, active)?
    };
    Ok((reference, old, value))
}
fn load(ctx: &Context, oid: &str, active: &dyn Fn() -> bool) -> Result<State, String> {
    if !oid_valid(oid) {
        return Err(crate::i18n::text("backend-invalid-virtual-state-object-d53722").into());
    }
    let spec = format!("{oid}:state.json");
    let size = ctx
        .text(&["cat-file", "-s", &spec], vec![], active)?
        .parse::<usize>()
        .map_err(|_| crate::i18n::text("backend-invalid-virtual-state-size-d42b0c"))?;
    if size > 4 * 1024 * 1024 {
        return Err(crate::i18n::text("backend-virtual-state-exceeds-4-mib-d12262").into());
    }
    let value: State =
        serde_json::from_slice(&ctx.run(&["cat-file", "blob", &spec], vec![], &[0], active)?)
            .map_err(|_| crate::i18n::text("backend-invalid-virtual-state-5a68df"))?;
    let entry = |e: &Entry| -> Result<(), String> {
        if !matches!(e.mode.as_str(), "100644" | "100755") || !oid_valid(&e.oid) {
            return Err(crate::i18n::text(
                "backend-virtual-state-contains-an-invalid-regular-file-entry-e3e734",
            )
            .into());
        }
        Ok(())
    };
    if let Some(m) = &value.mode {
        if !oid_valid(&m.base)
            || !m.anchor.starts_with("refs/heads/")
            || m.anchor.chars().any(char::is_control)
            || m.branches.len() > 16
        {
            return Err(
                crate::i18n::text("backend-invalid-virtual-anchor-or-branch-count-b479a3").into(),
            );
        }
        let mut names = BTreeSet::new();
        let mut active_ownership = BTreeSet::new();
        for b in &m.branches {
            if b.name.is_empty()
                || b.name.len() > 200
                || b.name.chars().any(char::is_control)
                || !oid_valid(&b.tip)
                || !names.insert(&b.name)
                || b.message.len() > 64 * 1024
                || b.owned.len() > 10000
            {
                return Err(
                    crate::i18n::text("backend-invalid-virtual-branch-metadata-462728").into(),
                );
            }
            let mut keys = BTreeSet::new();
            for h in &b.owned {
                valid_path(&h.path)?;
                if h.key.len() != 64
                    || !h.key.bytes().all(|b| b.is_ascii_hexdigit())
                    || !keys.insert(&h.key)
                    || (b.active && !active_ownership.insert(&h.key))
                {
                    return Err(crate::i18n::text(
                        "backend-invalid-or-duplicated-hunk-ownership-e02c5e",
                    )
                    .into());
                }
                for e in h.base.iter().chain(h.target.iter()) {
                    entry(e)?;
                }
            }
            for (path, bits) in &b.permissions {
                valid_path(path)?;
                if *bits > 0o777 {
                    return Err(
                        crate::i18n::text("backend-invalid-saved-file-permissions-310724").into(),
                    );
                }
            }
        }
    }
    if let Some(u) = &value.undo {
        if !u.previous.is_empty() && !oid_valid(&u.previous) {
            return Err(crate::i18n::text("backend-invalid-previous-virtual-state-3e8a6c").into());
        }
        for m in &u.moves {
            if !m.reference.starts_with("refs/heads/")
                || m.reference.chars().any(char::is_control)
                || (!m.old.is_empty() && !oid_valid(&m.old))
                || !oid_valid(&m.new)
            {
                return Err(crate::i18n::text("backend-invalid-undo-reference-move-eb6533").into());
            }
        }
        for (p, e) in u.before.iter().chain(u.after.iter()) {
            valid_path(p)?;
            if let Some(e) = e {
                entry(e)?;
            }
        }
        for (p, bits) in &u.permissions {
            valid_path(p)?;
            if *bits > 0o777 {
                return Err(crate::i18n::text("backend-invalid-undo-permissions-3e4c4e").into());
            }
        }
    }
    Ok(value)
}
fn root(ctx: &Context, value: &State, active: &dyn Fn() -> bool) -> Result<String, String> {
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(crate::i18n::text("backend-virtual-ownership-exceeds-4-mib-d8678b").into());
    }
    let blob = ctx.blob(bytes, active)?;
    let mut input = format!("100644 blob {blob}\tstate.json\0").into_bytes();
    let mut n = 0;
    let mut keep = |e: &Entry| {
        input.extend_from_slice(format!("{} blob {}\tkeep-{n}\0", e.mode, e.oid).as_bytes());
        n += 1;
    };
    if let Some(m) = &value.mode {
        for b in &m.branches {
            for h in &b.owned {
                for e in h.base.iter().chain(h.target.iter()) {
                    keep(e);
                }
            }
        }
    }
    if let Some(u) = &value.undo {
        for e in u.before.values().chain(u.after.values()).flatten() {
            keep(e);
        }
        if !u.previous.is_empty() {
            input.extend_from_slice(format!("040000 tree {}\tprevious\0", u.previous).as_bytes());
        }
    }
    ctx.text(&["mktree", "-z"], input, active)
}
fn snapshot(
    ctx: &Context,
    view: &View,
    files: bool,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    if state(ctx, active)?.1 != view.version || source(ctx, active)? != *view.source.source {
        return Err(crate::i18n::text(
            "backend-virtual-ownership-head-or-staged-contents-changed-refresh-before-ab0f18",
        )
        .into());
    }
    if files {
        if native_files::capture_policies(&ctx.repo, &view.source.work, active)?
            != *view.source.policies
        {
            return Err(crate::i18n::text(
                "backend-working-full-access-policy-changed-since-virtual-preview-fddcf4",
            )
            .into());
        }
        for (p, before) in view.source.work.iter() {
            if read_file(ctx, p, before.as_ref().map_or("100644", |(m, _)| m), active)? != *before {
                return Err(crate::i18n::message(
                    "backend-p-changed-since-the-ownership-preview-refresh-50dede",
                    &[("pointer", (p).to_string().into())],
                ));
            }
        }
    }
    Ok(())
}
fn anchor(view: &View) -> Result<&Mode, String> {
    if let Some(e) = &view.stale {
        return Err(e.clone());
    }
    view.state.mode.as_ref().ok_or_else(|| {
        crate::i18n::text("backend-start-virtual-mode-by-making-a-new-branch-f04b5f").to_owned()
    })
}
fn assemble(ctx: &Context, owned: &[&Hunk], active: &dyn Fn() -> bool) -> Result<Contents, String> {
    let mut by_path: BTreeMap<&str, Vec<&Hunk>> = BTreeMap::new();
    for h in owned {
        by_path.entry(&h.path).or_default().push(h);
    }
    by_path
        .into_iter()
        .map(|(path, hs)| {
            let first = hs
                .first()
                .ok_or(crate::i18n::text("backend-no-hunks-b248b6"))?;
            if hs.iter().any(|h| h.base != first.base) {
                return Err(crate::i18n::text(
                    "backend-hunks-use-different-immutable-bases-4a0705",
                )
                .into());
            }
            let value = if hs.iter().any(|h| h.whole) {
                if hs.len() != 1 {
                    return Err(crate::i18n::message(
                        "backend-whole-file-ownership-overlaps-another-change-in-path-675474",
                        &[("path", (path).to_string().into())],
                    ));
                }
                lists::data(
                    ctx,
                    &[(path.into(), first.target.clone())].into_iter().collect(),
                    active,
                )?
                .remove(path)
                .ok_or(crate::i18n::text("backend-missing-whole-file-0e4fed"))?
            } else {
                let base = lists::data(
                    ctx,
                    &[(path.into(), first.base.clone())].into_iter().collect(),
                    active,
                )?
                .remove(path)
                .flatten()
                .ok_or(crate::i18n::text("backend-missing-text-base-900646"))?;
                let text = std::str::from_utf8(&base.1).map_err(|_| {
                    crate::i18n::text("backend-text-ownership-requires-utf-8-5b4ff6")
                })?;
                let parsed = hs
                    .iter()
                    .map(|h| {
                        let mut p = hunks::parse(&h.patch);
                        if p.len() != 1 {
                            return Err(crate::i18n::text(
                                "backend-invalid-owned-text-hunk-2b4a3a",
                            )
                            .to_owned());
                        }
                        Ok(p.remove(0))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let picked = parsed.iter().collect::<Vec<_>>();
                let result = hunks::apply(text, &picked)?;
                Some((base.0, result.into_bytes()))
            };
            Ok((path.into(), value))
        })
        .collect()
}
fn live(ctx: &Context, view: &lists::View, active: &dyn Fn() -> bool) -> Result<Vec<Hunk>, String> {
    let mut work = view.source.index.clone();
    let changes = lists::raw_entries(ctx, &view.work, active)?;
    for (p, e) in changes {
        match e {
            Some(e) => {
                work.insert(p, e);
            }
            None => {
                work.remove(&p);
            }
        }
    }
    let work_tree = tree(ctx, &work, active)?;
    let base = view
        .source
        .head
        .as_deref()
        .ok_or(crate::i18n::text("backend-no-head-f43258"))?;
    let raw = ctx.run(
        &[
            "diff",
            "--name-only",
            "-z",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            base,
            &work_tree,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    let paths = untracked_paths(raw)?;
    let mut out = Vec::new();
    let mut total = 0;
    for path in paths {
        let before = view.head.get(&path).cloned();
        let after = work.get(&path).cloned();
        let data = lists::data(
            ctx,
            &[(path.clone(), before.clone())].into_iter().collect(),
            active,
        )?
        .remove(&path)
        .flatten();
        let now = view.work.get(&path).cloned().flatten();
        let whole = before
            .as_ref()
            .zip(after.as_ref())
            .is_none_or(|(a, b)| a.mode != b.mode)
            || data
                .iter()
                .chain(now.iter())
                .any(|(_, b)| b.contains(&0) || std::str::from_utf8(b).is_err());
        let patch = ctx.run(
            &[
                "--literal-pathspecs",
                "diff",
                "--binary",
                "--full-index",
                "--no-ext-diff",
                "--no-textconv",
                "--no-renames",
                "-U0",
                base,
                &work_tree,
                "--",
                &path,
            ],
            vec![],
            &[0],
            active,
        )?;
        total += patch.len();
        if total > TOTAL_LIMIT {
            return Err(crate::i18n::text("backend-virtual-diff-exceeds-32-mib-712c03").into());
        }
        let patch = String::from_utf8(patch)
            .map_err(|_| crate::i18n::text("backend-cannot-display-virtual-patch-beb98b"))?;
        if whole {
            let key = hash(format!("{path}\0{before:?}\0{after:?}").as_bytes());
            out.push(Hunk {
                key,
                path,
                patch,
                base: before,
                target: after,
                whole: true,
            });
        } else {
            let header = patch.split("@@ ").next().unwrap_or("");
            let mut fragments = Vec::new();
            let mut body = String::new();
            for line in patch.split_inclusive('\n') {
                if line.starts_with("@@ ") {
                    if !body.is_empty() {
                        fragments.push(std::mem::take(&mut body));
                    }
                    body.push_str(line);
                } else if !body.is_empty() {
                    body.push_str(line);
                }
            }
            if !body.is_empty() {
                fragments.push(body);
            }
            for body in fragments {
                let full = format!("{header}{body}");
                let parsed = hunks::parse(&full);
                let h = parsed.first().ok_or(crate::i18n::text(
                    "backend-cannot-parse-ownership-hunk-7f0f20",
                ))?;
                let key = hash(
                    format!(
                        "{path}\0{}\0{:?}\0{:?}\0{}\0{}",
                        h.old_start, h.removed, h.added, h.old_no_newline, h.new_no_newline
                    )
                    .as_bytes(),
                );
                out.push(Hunk {
                    key,
                    path: path.clone(),
                    patch: full,
                    base: before.clone(),
                    target: after.clone(),
                    whole: false,
                });
            }
        }
        if out.len() > 10000 {
            return Err(
                crate::i18n::text("backend-virtual-view-exceeds-10000-hunks-70b76e").into(),
            );
        }
    }
    Ok(out)
}
pub fn read(repo: &Path, active: &dyn Fn() -> bool) -> Result<View, String> {
    let ctx = Context::new(repo, active)?;
    let (reference, version, state) = state(&ctx, active)?;
    let mut source = lists::read(repo, active)?;
    if let Some(m) = &state.mode {
        let mut budget = lists::SnapshotBudget::from_sizes(
            source.work.values().flatten().map(|(_, bytes)| bytes.len()),
        )?;
        for path in m
            .branches
            .iter()
            .flat_map(|b| b.owned.iter().map(|h| &h.path))
        {
            if !source.work.contains_key(path) {
                let mode = source
                    .source
                    .index
                    .get(path)
                    .map_or("100644", |e| e.mode.as_str());
                if source.work.len() >= MAX_FILES {
                    return Err(
                        crate::i18n::text("backend-virtual-snapshot-exceeds-2000-paths-before-the-next-file-read-9ca27e").into(),
                    );
                }
                let allowance = budget.allowance()?;
                let bytes = read_file_limited(&ctx, path, mode, allowance, active)?;
                budget.retain(bytes.as_ref().map_or(0, |(_, bytes)| bytes.len()))?;
                Arc::make_mut(&mut source.work).insert(path.clone(), bytes);
                source.paths.push(path.clone());
            }
        }
    }

    source.policies = Arc::new(native_files::capture_policies(repo, &source.work, active)?);
    let stale=state.mode.as_ref().and_then(|m|{
        if source.source.head.as_deref()!=Some(&m.base)||source.source.branch.as_deref()!=Some(&m.anchor){Some(crate::i18n::text("backend-anchor-checkout-changed-return-to-its-exact-base-or-leave-virtual-f58cf9").into())}else{None}
    });
    let hunks = if stale.is_none() {
        live(&ctx, &source, active)?
    } else {
        Vec::new()
    };
    Ok(View {
        state,
        hunks,
        stale,
        version,
        reference,
        source,
    })
}
fn publish(
    ctx: &Context,
    view: &View,
    value: &State,
    moves: &[Move],
    evidence: &str,
    active: &dyn Fn() -> bool,
) -> Result<native_files::CommitOutcome, String> {
    let new = root(ctx, value, active)?;
    let head = view
        .source
        .source
        .head
        .as_deref()
        .ok_or(crate::i18n::text("backend-no-head-f43258"))?;
    let branch = view
        .source
        .source
        .branch
        .as_deref()
        .ok_or(crate::i18n::text(
            "backend-use-a-local-anchor-branch-d5a124",
        ))?;
    if !branch.starts_with("refs/heads/") {
        return Err(crate::i18n::text("backend-use-a-local-anchor-branch-d5a124").into());
    }
    let worktrees = ctx.source(
        &["worktree", "list", "--porcelain", "-z"],
        vec![],
        &[0],
        active,
    )?;
    for m in moves {
        if !m.reference.starts_with("refs/heads/")
            || m.reference == branch
            || worktrees
                .split(|b| *b == 0)
                .any(|r| r == format!("branch {}", m.reference).as_bytes())
        {
            return Err(crate::i18n::text(
                "backend-a-virtual-branch-is-checked-out-elsewhere-or-is-the-anchor-switch-5f6eb4",
            )
            .into());
        }
        ctx.source(&["check-ref-format", &m.reference], vec![], &[0], active)?;
        if !ctx
            .source(
                &["symbolic-ref", "-q", &m.reference],
                vec![],
                &[0, 1],
                active,
            )?
            .is_empty()
        {
            return Err(crate::i18n::text(
                "backend-virtual-branches-must-be-direct-local-refs-ba98b1",
            )
            .into());
        }
    }
    let zero = "0".repeat(new.len());
    let mut tx = format!("start\nupdate HEAD {head} {head}\n");
    tx.push_str(&lists::direct_update(
        &view.reference,
        &new,
        if view.version.is_empty() {
            &zero
        } else {
            &view.version
        },
    ));
    for m in moves {
        tx.push_str(&lists::direct_update(
            &m.reference,
            &m.new,
            if m.old.is_empty() { &zero } else { &m.old },
        ));
    }
    if value.mode.is_some()
        && let Some(mode) = &view.state.mode
    {
        for b in &mode.branches {
            let reference = format!("refs/heads/{}", b.name);
            if moves.iter().all(|m| m.reference != reference) {
                tx.push_str(&lists::direct_verify(&reference, &b.tip));
            }
        }
    }
    tx.push_str("prepare\ncommit\n");
    std::fs::write(ctx.path.join("expected-worktrees"), &worktrees).map_err(|e| e.to_string())?;
    let mut direct_refs = vec![view.reference.clone()];
    direct_refs.extend(moves.iter().map(|m| m.reference.clone()));
    if value.mode.is_some()
        && let Some(mode) = &view.state.mode
    {
        direct_refs.extend(
            mode.branches
                .iter()
                .map(|b| format!("refs/heads/{}", b.name)),
        );
    }
    let script = format!(
        "#!/bin/sh\nif [ \"$1\" = prepared ]; then test \"$(git symbolic-ref -q HEAD)\" = {} || exit 1; git worktree list --porcelain -z | cmp -s - {} || exit 1; {}fi\ncat >/dev/null\n",
        remote::sh_quote(branch),
        remote::sh_quote(&remote::git_path(&ctx.path.join("expected-worktrees"))),
        lists::direct_ref_guard(direct_refs.iter().map(String::as_str))
    );
    let hook = ctx.path.join("hooks/reference-transaction");
    std::fs::write(&hook, script).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(hook, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let pending = publication::begin(
        ctx,
        "virtuals",
        &view.reference,
        (&view.version, &new),
        &view.source.source,
        evidence,
        active,
    )?;
    pending.append_receipt(&mut tx);
    let result = ctx.source(
        &["update-ref", "-m", "gitcraft virtual branches", "--stdin"],
        tx.into_bytes(),
        &[0],
        active,
    );
    Ok(pending.finish(ctx, result))
}

fn undo_record(
    view: &View,
    moves: Vec<Move>,
    before: BTreeMap<String, Option<Entry>>,
    after: BTreeMap<String, Option<Entry>>,
    permissions: BTreeMap<String, u32>,
) -> Undo {
    Undo {
        previous: view.version.clone(),
        moves,
        access: view
            .source
            .policies
            .iter()
            .filter(|(p, _)| before.contains_key(*p))
            .map(|(p, policy)| (p.clone(), policy.clone()))
            .collect(),
        before,
        after,
        permissions,
    }
}
pub enum Edit {
    Create(String),
    Message(String, String),
    Assign(String, String),
    Release(String, String),
    Leave,
}
pub fn edit(
    repo: &Path,
    view: &View,
    edit: Edit,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-virtual-branch-mutation-requires-a-native-checkout-remote-files-w-bfedcb",
        )
        .into());
    }
    let ctx = Context::new(repo, active)?;
    snapshot(&ctx, view, true, active)?;
    let mut next = view.state.clone();
    let mut moves = Vec::new();
    if !matches!(&edit, Edit::Leave) && view.stale.is_some() {
        return Err(view.stale.clone().unwrap_or_default());
    }
    match edit {
        Edit::Create(name) => {
            if name.is_empty() || name.len() > 200 || name.chars().any(char::is_control) {
                return Err(crate::i18n::text(
                    "backend-choose-a-local-branch-name-up-to-200-bytes-f101ae",
                )
                .into());
            }
            let reference = format!("refs/heads/{name}");
            ctx.source(&["check-ref-format", &reference], vec![], &[0], active)?;
            let base = view
                .source
                .source
                .head
                .as_deref()
                .ok_or(crate::i18n::text("backend-make-an-initial-commit-20a10b"))?
                .to_owned();
            let mode = next.mode.get_or_insert_with(|| Mode {
                anchor: view.source.source.branch.clone().unwrap_or_default(),
                base: base.clone(),
                branches: Vec::new(),
            });
            if mode.branches.len() >= 16 {
                return Err(crate::i18n::text(
                    "backend-at-most-16-virtual-branches-are-supported-4e99fa",
                )
                .into());
            }
            mode.branches.push(Branch {
                name: name.clone(),
                tip: base.clone(),
                active: true,
                message: String::new(),
                owned: Vec::new(),
                permissions: BTreeMap::new(),
                access: BTreeMap::new(),
            });
            moves.push(Move {
                reference,
                old: String::new(),
                new: base,
            });
        }
        Edit::Message(name, message) => {
            anchor(view)?;
            if message.len() > 64 * 1024 {
                return Err(crate::i18n::text("backend-message-exceeds-64-kib-8bc64e").into());
            }
            next.mode
                .as_mut()
                .ok_or(crate::i18n::text("backend-no-virtual-mode-b84eb6"))?
                .branches
                .iter_mut()
                .find(|b| b.name == name)
                .ok_or(crate::i18n::text("backend-no-branch-db114a"))?
                .message = message;
        }
        Edit::Assign(name, key) => {
            anchor(view)?;
            let h = view
                .hunks
                .iter()
                .find(|h| h.key == key)
                .ok_or(crate::i18n::text(
                    "backend-hunk-changed-refresh-and-review-it-again-42f077",
                ))?
                .clone();
            let mode = next
                .mode
                .as_mut()
                .ok_or(crate::i18n::text("backend-no-mode-6a4854"))?;
            if !name.is_empty() && mode.branches.iter().all(|b| b.name != name || !b.active) {
                return Err(
                    crate::i18n::text("backend-choose-an-active-virtual-branch-ccf0b4").into(),
                );
            }
            for b in &mut mode.branches {
                if b.active {
                    b.owned.retain(|h| h.key != key);
                    if b.name == name {
                        b.owned.push(h.clone());
                        if view.source.work.get(&h.path).is_some_and(Option::is_some) {
                            if let Some(policy) = view.source.policies.get(&h.path) {
                                b.access.insert(h.path.clone(), policy.clone());
                            } else if native_files::policy_supported(repo) {
                                return Err(crate::i18n::text(
                                    "backend-full-working-access-policy-missing-refresh-ce10b5",
                                )
                                .into());
                            }
                            b.permissions.insert(
                                h.path.clone(),
                                super::permissions::Pin::capture(repo, &h.path, active)?.bits(),
                            );
                        }
                    }
                }
            }
        }
        Edit::Release(name, key) => {
            anchor(view)?;
            let b = next
                .mode
                .as_mut()
                .ok_or(crate::i18n::text("backend-no-mode-6a4854"))?
                .branches
                .iter_mut()
                .find(|b| b.name == name && b.active)
                .ok_or(crate::i18n::text("backend-no-active-branch-08071f"))?;
            b.owned.retain(|h| h.key != key);
        }
        Edit::Leave => {
            next.mode = None;
        }
    }
    next.undo = Some(undo_record(
        view,
        moves.clone(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    ));
    publish(&ctx, view, &next, &moves, "", active)?.confirmed()?;
    Ok(crate::i18n::text(
        "backend-virtual-ownership-saved-anchor-real-index-and-working-files-retai-4626a4",
    )
    .into())
}
fn unchanged<'a>(view: &'a View, name: &str) -> Result<&'a Branch, String> {
    let b = anchor(view)?
        .branches
        .iter()
        .find(|b| b.name == name)
        .ok_or(crate::i18n::text(
            "backend-virtual-branch-disappeared-36a8f5",
        ))?;
    if b.active
        && b.owned
            .iter()
            .any(|h| view.hunks.iter().all(|now| now.key != h.key))
    {
        return Err(crate::i18n::text(
            "backend-owned-hunks-changed-release-stale-ownership-and-explicitly-assign-d0aaa9",
        )
        .into());
    }
    Ok(b)
}
pub fn toggle(
    repo: &Path,
    view: &View,
    name: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-virtual-activation-requires-a-native-checkout-remote-files-were-n-a1ac2b",
        )
        .into());
    }
    let ctx = Context::new(repo, active)?;
    snapshot(&ctx, view, true, active)?;
    let b = unchanged(view, name)?;
    let paths = b
        .owned
        .iter()
        .map(|h| h.path.clone())
        .collect::<BTreeSet<_>>();
    if paths
        .iter()
        .any(|p| view.source.source.index.get(p) != view.source.head.get(p))
    {
        return Err(crate::i18n::text(
            "backend-unstage-the-affected-paths-before-changing-their-materialization-c471e0",
        )
        .into());
    }
    if view
        .source
        .lists
        .lists
        .iter()
        .any(|l| l.shelf.is_none() && l.paths.iter().any(|p| paths.contains(p)))
    {
        return Err(crate::i18n::text(
            "backend-an-affected-file-belongs-to-a-named-changelist-unassign-it-before-e55349",
        )
        .into());
    }
    let mut wanted = view
        .hunks
        .iter()
        .filter(|h| !b.active || b.owned.iter().all(|own| own.key != h.key))
        .filter(|h| paths.contains(&h.path))
        .collect::<Vec<_>>();
    if !b.active {
        for h in &b.owned {
            if wanted.iter().any(|now| now.key == h.key) {
                return Err(crate::i18n::text("backend-an-inactive-hunk-is-already-present-resolve-ownership-explicitly-d7d9cf").into());
            }
            wanted.push(h);
        }
    }
    let mut target = assemble(&ctx, &wanted, active)?;
    for p in &paths {
        if !target.contains_key(p) {
            let base = lists::data(
                &ctx,
                &[(p.clone(), view.source.head.get(p).cloned())]
                    .into_iter()
                    .collect(),
                active,
            )?
            .remove(p)
            .ok_or(crate::i18n::text("backend-missing-base-path-a3839b"))?;
            target.insert(p.clone(), base);
        }
    }
    let before = paths
        .iter()
        .map(|p| {
            Ok((
                p.clone(),
                view.source
                    .work
                    .get(p)
                    .ok_or(crate::i18n::text(
                        "backend-path-missing-from-current-snapshot-refresh-5dab69",
                    ))?
                    .clone(),
            ))
        })
        .collect::<Result<Contents, String>>()?;
    let before_entries = lists::raw_entries(&ctx, &before, active)?;
    let after_entries = lists::raw_entries(&ctx, &target, active)?;
    let permissions = paths
        .iter()
        .filter(|p| before.get(*p).is_some_and(Option::is_some))
        .map(|p| {
            Ok((
                p.clone(),
                super::permissions::Pin::capture(repo, p, active)?.bits(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut next = view.state.clone();
    next.mode
        .as_mut()
        .ok_or(crate::i18n::text("backend-no-mode-6a4854"))?
        .branches
        .iter_mut()
        .find(|a| a.name == name)
        .ok_or(crate::i18n::text("backend-no-branch-db114a"))?
        .active = !b.active;
    next.undo = Some(undo_record(
        view,
        Vec::new(),
        before_entries,
        after_entries,
        permissions,
    ));
    capture_preserving_paths(
        repo,
        "Before changing virtual branch materialization",
        None,
        &paths.iter().cloned().collect::<Vec<_>>(),
        active,
    )?;
    snapshot(&ctx, view, true, active)?;
    let evidence = working_transaction::apply_with_commit(
        repo,
        (&before, &target),
        &view.source.policies,
        &|| snapshot(&ctx, view, false, active),
        &|evidence| publish(&ctx, view, &next, &[], evidence, active),
        if !b.active { Some(&b.access) } else { None },
        active,
    )?;
    Ok(crate::i18n::message(
        if b.active {
            "backend-virtual-deactivated"
        } else {
            "backend-virtual-activated"
        },
        &[
            ("name", (name).to_string().into()),
            ("evidence", (evidence).to_string().into()),
        ],
    ))
}
#[derive(Clone)]
pub struct Prepared {
    pub view: View,
    pub name: String,
    pub patch: String,
    tree: String,
}
pub fn prepare_commit(
    repo: &Path,
    view: &View,
    name: &str,
    active: &dyn Fn() -> bool,
) -> Result<Prepared, String> {
    let ctx = Context::new(repo, active)?;
    snapshot(&ctx, view, true, active)?;
    let b = unchanged(view, name)?;
    if !b.active || b.owned.is_empty() || b.message.trim().is_empty() {
        return Err(crate::i18n::text(
            "backend-choose-an-active-branch-with-owned-hunks-and-a-draft-message-58d992",
        )
        .into());
    }
    let selected = assemble(&ctx, &b.owned.iter().collect::<Vec<_>>(), active)?;
    let owned = lists::raw_entries(&ctx, &selected, active)?;
    let mut entries = view.source.head.as_ref().clone();
    for (p, e) in owned {
        match e {
            Some(e) => {
                entries.insert(p, e);
            }
            None => {
                entries.remove(&p);
            }
        }
    }
    let tree = tree(&ctx, &entries, active)?;
    let patch = ctx.run(
        &[
            "diff",
            "--binary",
            "--no-ext-diff",
            "--no-textconv",
            &b.tip,
            &tree,
            "--",
        ],
        vec![],
        &[0],
        active,
    )?;
    if patch.is_empty() {
        return Err(crate::i18n::text(
            "backend-branch-tree-already-contains-these-owned-changes-c99e3a",
        )
        .into());
    }
    if patch.len() > TOTAL_LIMIT {
        return Err(
            crate::i18n::text("backend-virtual-commit-preview-exceeds-32-mib-6c21cf").into(),
        );
    }
    Ok(Prepared {
        view: view.clone(),
        name: name.into(),
        tree,
        patch: String::from_utf8(patch)
            .map_err(|_| crate::i18n::text("backend-cannot-display-commit-preview-a2e9e0"))?,
    })
}
pub fn commit(repo: &Path, plan: &Prepared, active: &dyn Fn() -> bool) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-virtual-commit-requires-a-native-checkout-remote-refs-were-not-ch-7c3e18",
        )
        .into());
    }
    let ctx = Context::new(repo, active)?;
    let view = &plan.view;
    snapshot(&ctx, view, true, active)?;
    let b = unchanged(view, &plan.name)?;
    let new_plan = prepare_commit(repo, view, &plan.name, active)?;
    if new_plan.tree != plan.tree || new_plan.patch != plan.patch {
        return Err(crate::i18n::text(
            "backend-virtual-commit-preview-changed-prepare-it-again-02cafb",
        )
        .into());
    }
    let setting = |key: &str| -> Result<String, String> {
        String::from_utf8(ctx.source(&["config", "--get", key], vec![], &[0], active)?)
            .map(|s| format!("{key}={}", s.trim()))
            .map_err(|_| crate::i18n::text("backend-invalid-author-configuration-183c3a").into())
    };
    let new = ctx.text(
        &[
            "-c",
            &setting("user.name")?,
            "-c",
            &setting("user.email")?,
            "commit-tree",
            &plan.tree,
            "-p",
            &b.tip,
        ],
        b.message.as_bytes().to_vec(),
        active,
    )?;
    let moves = vec![Move {
        reference: format!("refs/heads/{}", b.name),
        old: b.tip.clone(),
        new: new.clone(),
    }];
    let mut next = view.state.clone();
    next.mode
        .as_mut()
        .ok_or(crate::i18n::text("backend-no-mode-6a4854"))?
        .branches
        .iter_mut()
        .find(|a| a.name == b.name)
        .ok_or(crate::i18n::text("backend-no-branch-db114a"))?
        .tip = new.clone();
    next.undo = Some(undo_record(
        view,
        moves.clone(),
        BTreeMap::new(),
        BTreeMap::new(),
        BTreeMap::new(),
    ));
    snapshot(&ctx, view, true, active)?;
    publish(&ctx, view, &next, &moves, "", active)?.confirmed()?;
    Ok(crate::i18n::message(
        "backend-committed-only-s-owned-hunks-as-new-anchor-working-files-and-stag-c6eafa",
        &[
            ("name", (b.name).to_string().into()),
            ("new", (new).to_string().into()),
        ],
    ))
}
pub fn undo(repo: &Path, view: &View, active: &dyn Fn() -> bool) -> Result<String, String> {
    if remote::is_remote(repo) {
        return Err(crate::i18n::text(
            "backend-virtual-undo-requires-a-native-checkout-remote-files-were-not-cha-118fef",
        )
        .into());
    }
    let ctx = Context::new(repo, active)?;
    snapshot(&ctx, view, true, active)?;
    let u = view.state.undo.as_ref().ok_or(crate::i18n::text(
        "backend-no-virtual-operation-to-undo-fd58c8",
    ))?;
    let mut previous = if u.previous.is_empty() {
        State::default()
    } else {
        load(&ctx, &u.previous, active)?
    };
    // Creating a virtual branch is undone by dropping its ownership registration. Keep
    // its published real ref for recovery; do not delete branches implicitly.
    let moves = u
        .moves
        .iter()
        .filter(|m| !m.old.is_empty())
        .map(|m| Move {
            reference: m.reference.clone(),
            old: m.new.clone(),
            new: m.old.clone(),
        })
        .collect::<Vec<_>>();
    let before = lists::data(&ctx, &u.after, active)?;
    let target = lists::data(&ctx, &u.before, active)?;
    for (p, expected) in &before {
        if read_file(
            &ctx,
            p,
            expected.as_ref().map_or("100644", |(m, _)| m),
            active,
        )? != *expected
        {
            return Err(crate::i18n::text(
                "backend-files-changed-since-the-virtual-operation-nothing-was-undone-0dcdb6",
            )
            .into());
        }
    }
    if u.before
        .keys()
        .any(|p| view.source.source.index.get(p) != view.source.head.get(p))
    {
        return Err(crate::i18n::text(
            "backend-an-affected-file-has-staged-edits-unstage-it-before-virtual-undo-b61a1a",
        )
        .into());
    }
    previous.undo = None;
    let evidence = working_transaction::apply_with_commit(
        repo,
        (&before, &target),
        &view.source.policies,
        &|| snapshot(&ctx, view, false, active),
        &|evidence| publish(&ctx, view, &previous, &moves, evidence, active),
        Some(&u.access),
        active,
    )?;
    Ok(crate::i18n::message(
        "backend-undid-the-last-virtual-operation-unrelated-edits-and-recovery-bra-240ae2",
        &[("evidence", (evidence).to_string().into())],
    ))
}
