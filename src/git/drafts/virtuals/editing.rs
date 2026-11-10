//! Virtual branch edits and publication with existing undo records.

use super::*;

pub(super) fn publish(
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

pub(super) fn undo_record(
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
                                super::super::permissions::Pin::capture(repo, &h.path, active)?
                                    .bits(),
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

pub(super) fn unchanged<'a>(view: &'a View, name: &str) -> Result<&'a Branch, String> {
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
