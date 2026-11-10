//! Commit.

use super::*;

pub(in crate::git::drafts) fn data(
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

pub(in crate::git::drafts) fn raw_entries(
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

pub(super) fn selected_index_records(
    selected: &BTreeMap<String, Option<Entry>>,
    oid_len: usize,
) -> Vec<u8> {
    selected
        .iter()
        .flat_map(|(path, entry)| match entry {
            Some(entry) => format!("{} {}\t{path}\0", entry.mode, entry.oid).into_bytes(),
            None => format!("0 {}\t{path}\0", "0".repeat(oid_len)).into_bytes(),
        })
        .collect()
}

pub(super) fn final_index(
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
        "Before committing a changelist",
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
