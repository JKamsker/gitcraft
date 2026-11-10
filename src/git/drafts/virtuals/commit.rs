//! Virtual branch commit preparation, publication, and undo.

use super::*;

#[derive(Clone)]
pub struct Prepared {
    pub view: View,
    pub name: String,
    pub patch: String,
    pub(super) tree: String,
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
