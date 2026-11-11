//! Virtual branch snapshot construction and live hunk assembly.

use super::*;

pub(super) fn root(
    ctx: &Context,
    value: &State,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
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

pub(super) fn snapshot(
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

pub(super) fn anchor(view: &View) -> Result<&Mode, String> {
    if let Some(e) = &view.stale {
        return Err(e.clone());
    }
    view.state.mode.as_ref().ok_or_else(|| {
        crate::i18n::text("backend-start-virtual-mode-by-making-a-new-branch-f04b5f").to_owned()
    })
}

pub(super) fn assemble(
    ctx: &Context,
    owned: &[&Hunk],
    active: &dyn Fn() -> bool,
) -> Result<Contents, String> {
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

pub(super) fn live(
    ctx: &Context,
    view: &lists::View,
    active: &dyn Fn() -> bool,
) -> Result<Vec<Hunk>, String> {
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
