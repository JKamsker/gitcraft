//! Shelve.

use super::*;

pub(super) fn require_restore_ownership(lists: &Lists, restoring: &List) -> Result<(), String> {
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
                    super::super::permissions::Pin::capture(repo, p, active)?.bits(),
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
        (&current, &target),
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
        if restore {
            "backend-changelist-unshelved"
        } else {
            "backend-changelist-shelved"
        },
        &[
            ("name", (list.name).to_string().into()),
            ("evidence", (evidence).to_string().into()),
        ],
    ))
}
