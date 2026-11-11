//! Bounded tree-only replay and conflict classification.

use super::*;

pub(super) fn replay(
    repo: &Path,
    onto: &str,
    plan: &[(String, Step)],
    cache: &mut Cache,
    mergiraf: bool,
    requested: impl Fn() -> bool,
) -> Vec<Forecast> {
    let began = Instant::now();
    replay_with_budget(repo, onto, plan, cache, mergiraf, requested, || {
        began.elapsed() < Duration::from_secs(90)
    })
}

pub(super) fn replay_with_budget(
    repo: &Path,
    onto: &str,
    plan: &[(String, Step)],
    cache: &mut Cache,
    mergiraf: bool,
    requested: impl Fn() -> bool,
    within_budget: impl Fn() -> bool,
) -> Vec<Forecast> {
    if !requested() {
        return Vec::new();
    }
    let active = || requested() && within_budget();
    let setup = || -> Result<(String, ForecastGit), String> {
        let context = ForecastGit::new(repo, &active)?;
        let tree = if onto.is_empty() {
            context.run(&["mktree"], &[0], &active)?
        } else {
            context.run(
                &[
                    "rev-parse",
                    "--verify",
                    "--end-of-options",
                    &format!("{onto}^{{tree}}"),
                ],
                &[0],
                &active,
            )?
        };
        Ok((tree.trim().to_owned(), context))
    };
    let (tree, context) = match setup() {
        Ok(value) => value,
        Err(e) => {
            if !requested() {
                return Vec::new();
            }
            if !within_budget() {
                return finish_budget(plan, Vec::new(), &requested, &within_budget);
            }
            return unfinished(plan, &e);
        }
    };
    replay_rows(
        plan,
        cache,
        tree,
        &requested,
        &within_budget,
        |tree, commit, active| apply(&context, tree, commit, mergiraf, active),
    )
}

/// Keep completed rows, but never publish a short current-generation answer when the
/// overall budget runs out. Cancellation belongs to a stale editor/generation instead.
pub(super) fn finish_budget(
    plan: &[(String, Step)],
    mut rows: Vec<Forecast>,
    requested: &dyn Fn() -> bool,
    within_budget: &dyn Fn() -> bool,
) -> Vec<Forecast> {
    if requested() && !within_budget() {
        rows.extend(unfinished(
            &plan[rows.len()..],
            crate::i18n::text("repo-core-3255"),
        ));
    }
    rows
}

pub(super) fn unfinished(plan: &[(String, Step)], reason: &str) -> Vec<Forecast> {
    plan.iter()
        .map(|(_, step)| {
            if *step == Step::Drop {
                Forecast::Dropped
            } else {
                Forecast::Unknown(reason.to_owned())
            }
        })
        .collect()
}

pub(super) fn replay_rows(
    plan: &[(String, Step)],
    cache: &mut Cache,
    mut tree: String,
    requested: &dyn Fn() -> bool,
    within_budget: &dyn Fn() -> bool,
    mut apply_step: impl FnMut(&str, &str, &dyn Fn() -> bool) -> Result<Applied, String>,
) -> Vec<Forecast> {
    let active = || requested() && within_budget();
    let mut rows = Vec::with_capacity(plan.len());
    let mut blocked: Option<String> = None;
    for (commit, step) in plan {
        if !active() {
            break;
        }
        if *step == Step::Drop {
            rows.push(Forecast::Dropped);
            continue;
        }
        if let Some(reason) = &blocked {
            rows.push(Forecast::Unknown(reason.clone()));
            continue;
        }
        let key = (tree.clone(), commit.clone());
        let applied = match cache.get(&key) {
            Some(value) => Ok(value.clone()),
            None => apply_step(&tree, commit, &active).inspect(|value| {
                // Bound a long editing session without affecting correctness.
                if cache.len() >= 2048 {
                    cache.clear();
                }
                cache.insert(key, value.clone());
            }),
        };
        if !active() {
            break;
        }
        match applied {
            Ok(applied) => {
                if matches!(
                    applied.forecast,
                    Forecast::Conflict(_) | Forecast::Unknown(_)
                ) {
                    blocked = Some(crate::i18n::text("repo-core-3256").to_owned());
                }
                tree = applied.tree;
                rows.push(applied.forecast);
            }
            Err(e) => {
                let reason = crate::i18n::message("repo-core-3257", &[("e", e.to_string().into())]);
                blocked = Some(reason.clone());
                rows.push(Forecast::Unknown(reason));
            }
        }
    }
    finish_budget(plan, rows, requested, within_budget)
}

pub(super) fn apply(
    context: &ForecastGit,
    tree: &str,
    commit: &str,
    mergiraf: bool,
    active: &dyn Fn() -> bool,
) -> Result<Applied, String> {
    // Resolve before commands without --end-of-options; only object IDs follow.
    let commit = context.run(
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{commit}^{{commit}}"),
        ],
        &[0],
        active,
    )?;
    let commit = commit.trim();
    let ancestry = context.run(
        &["rev-list", "--parents", "-n", "1", commit, "--"],
        &[0],
        active,
    )?;
    let parent = match ancestry.split_whitespace().nth(1) {
        Some(parent) => parent.to_owned(),
        None => {
            let empty = context.run(&["mktree"], &[0], active)?;
            context.run(
                &[
                    "-c",
                    "user.name=gitcraft forecast",
                    "-c",
                    "user.email=forecast@gitcraft.invalid",
                    "commit-tree",
                    "--no-gpg-sign",
                    empty.trim(),
                    "-m",
                    "empty forecast base",
                ],
                &[0],
                active,
            )?
        }
    };
    let parent = parent.trim();
    let temporary = context.run(
        &[
            "-c",
            "user.name=gitcraft forecast",
            "-c",
            "user.email=forecast@gitcraft.invalid",
            "commit-tree",
            "--no-gpg-sign",
            tree,
            "-m",
            "gitcraft rebase forecast",
        ],
        &[0],
        active,
    )?;
    // Global option: unsupported Git fails explicitly rather than silently ignoring an env
    // variable. All attribute lookups use the simulated parent, even in a dirty checkout.
    let attributes = format!("--attr-source={tree}");
    let base = format!("--merge-base={parent}");
    let output = context.run(
        &[
            "-c",
            "merge.default=text",
            "-c",
            "merge.conflictStyle=diff3",
            "-c",
            "merge.renormalize=false",
            &attributes,
            "merge-tree",
            "--write-tree",
            "--name-only",
            "-z",
            &base,
            temporary.trim(),
            commit,
        ],
        &[0, 1],
        active,
    )?;
    let (result_tree, files) = parse_merge(&output)?;
    let forecast = if files.is_empty() {
        if result_tree == tree
            && !context
                .run(
                    &[
                        "diff-tree",
                        "--no-ext-diff",
                        "--no-textconv",
                        "--name-only",
                        "-r",
                        parent,
                        commit,
                    ],
                    &[0],
                    active,
                )?
                .is_empty()
        {
            Forecast::Empty
        } else {
            Forecast::Clean
        }
    } else {
        let mut conflicts = Vec::new();
        for path in files {
            let checked = if !mergiraf {
                Err(crate::i18n::text("repo-core-3273").to_owned())
            } else {
                context
                    .bytes(
                        &["show", "--end-of-options", &format!("{result_tree}:{path}")],
                        &[0],
                        active,
                    )
                    .and_then(|bytes| {
                        String::from_utf8(bytes)
                            .map_err(|_| crate::i18n::text("repo-core-3275").to_owned())
                    })
                    .and_then(|text| {
                        if !text.lines().any(|line| line.starts_with("<<<<<<< ")) {
                            return Err(crate::i18n::text("repo-core-3276").to_owned());
                        }
                        crate::mergiraf::solve_text(&path, &text, active)
                    })
            };
            if !active() {
                return Err(crate::i18n::text("repo-core-3277").into());
            }
            let result = match checked {
                Ok(text) if !text.lines().any(|line| line.starts_with("<<<<<<< ")) => {
                    crate::i18n::text("repo-core-3278").to_owned()
                }
                Ok(_) => crate::i18n::text("repo-core-3279").to_owned(),
                Err(e) => crate::i18n::message("repo-core-3280", &[("e", e.to_string().into())]),
            };
            conflicts.push((path, result));
        }
        Forecast::Conflict(conflicts)
    };
    Ok(Applied {
        tree: result_tree,
        forecast,
    })
}

pub(super) fn parse_merge(output: &str) -> Result<(String, Vec<String>), String> {
    let mut fields = output.split('\0');
    let tree = fields.next().unwrap_or_default();
    if !matches!(tree.len(), 40 | 64) || !tree.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(crate::i18n::text("repo-core-3281").to_owned());
    }
    let mut files = Vec::new();
    for path in fields.take_while(|path| !path.is_empty()) {
        if !files.iter().any(|p| p == path) {
            files.push(path.to_owned());
        }
    }
    Ok((tree.to_owned(), files))
}
