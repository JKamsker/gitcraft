//! Publication.

use super::*;

pub(super) fn state_tree(
    ctx: &Context,
    lists: &Lists,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
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
    super::super::tree(ctx, &tree, active)
}

pub(in crate::git::drafts) fn direct_update(reference: &str, new: &str, old: &str) -> String {
    format!("option no-deref\nupdate {reference} {new} {old}\n")
}

pub(in crate::git::drafts) fn direct_verify(reference: &str, old: &str) -> String {
    format!("option no-deref\nverify {reference} {old}\n")
}

pub(in crate::git::drafts) fn direct_ref_guard<'a>(
    references: impl IntoIterator<Item = &'a str>,
) -> String {
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

pub(super) fn publish(
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
    let pending = super::super::publication::begin(
        ctx,
        "changelists",
        &view.reference,
        (&view.version, &new),
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
