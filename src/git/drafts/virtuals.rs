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

mod commit;
pub use commit::Prepared;
pub use commit::commit;
pub use commit::prepare_commit;
pub use commit::undo;

mod editing;
pub use editing::Edit;
pub use editing::edit;
use editing::publish;
use editing::unchanged;
use editing::undo_record;

mod snapshot;
use snapshot::anchor;
use snapshot::assemble;
use snapshot::live;
use snapshot::root;
use snapshot::snapshot;
