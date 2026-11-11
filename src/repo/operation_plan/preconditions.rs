use super::*;

/// Captures only the witness from the same validator invocation; no additional Git call.
pub(in crate::repo) fn check_stack_captured(
    base: &str,
    head: &str,
    control: Option<&Arc<git::OpControl>>,
    run: impl FnMut(&[&str]) -> Result<String, String>,
) -> Result<(), String> {
    let (result, source) = crate::gerrit::check_stack_presented_with(base, head, run);
    if let Some(control) = control {
        control.stack_checked(source);
    }
    result
}

pub(in crate::repo) fn check_expected(
    path: &Path,
    ops: &[Vec<String>],
    control: Option<&Arc<git::OpControl>>,
) -> Result<(), String> {
    let plan = model::Plan::decode(ops);
    for op in ops {
        if let [mark, base, head] = op.as_slice()
            && mark == crate::gerrit::CHECK
        {
            check_stack_captured(base, head, control, |args| {
                if let Some(control) = control {
                    git::run_watched(Some(path), args, None, true, control)
                } else {
                    git::run(Some(path), args)
                }
            })?;
        }
    }
    for (rev, tree) in plan.trees() {
        let now = git::run(
            Some(path),
            &["rev-parse", "--verify", "-q", &format!("{rev}^{{tree}}")],
        )
        .map(|s| s.trim().to_owned())
        .unwrap_or_default();
        if now != tree {
            return Err(crate::i18n::message(
                "repo-core-4178",
                &[("rev", rev.to_string().into())],
            ));
        }
    }
    for (name, target, dir) in plan.symrefs() {
        let mut args = Vec::new();
        if let Some(dir) = dir {
            args.extend(["-C", dir]);
        }
        args.extend(["symbolic-ref", "-q", name]);
        // Exit 1, nothing printed: detached.
        let now = git::run_ok_codes(Some(path), &args, &[0, 1]).map(|s| s.trim().to_owned());
        if now.as_deref() != Ok(target) {
            let says = |r: &str| {
                if r.is_empty() {
                    crate::i18n::text("repo-core-4179").to_owned()
                } else {
                    r.strip_prefix("refs/heads/").unwrap_or(r).to_owned()
                }
            };
            let now = now.map_or_else(
                |_| crate::i18n::text("repo-core-4180").to_owned(),
                |r| says(&r),
            );
            let place = dir.map_or(String::new(), |d| {
                crate::i18n::message("repo-core-4181", &[("d", d.to_string().into())])
            });
            return Err(crate::i18n::message(
                "repo-core-4182",
                &[
                    ("now", now.to_string().into()),
                    ("place", place.to_string().into()),
                    ("target", says(target).to_string().into()),
                ],
            ));
        }
    }
    for (name, value) in plan.refs() {
        let now = git::resolve(path, name).unwrap_or_default();
        if now != value {
            let what = name.strip_prefix("refs/heads/").unwrap_or(name);
            return Err(if name == "refs/stash" {
                crate::i18n::text("repo-core-4183").to_owned()
            } else if value.is_empty() {
                crate::i18n::message("repo-core-4184", &[("what", what.to_string().into())])
            } else {
                crate::i18n::message(
                    "repo-core-4185",
                    &[
                        ("what", what.to_string().into()),
                        ("now", crate::repo::short(&now).to_string().into()),
                        ("value", crate::repo::short(value).to_string().into()),
                    ],
                )
            });
        }
    }
    Ok(())
}

pub(in crate::repo) fn check_local_expected(
    path: &Path,
    ops: &[Vec<String>],
    mutation: &[String],
    control: Option<&Arc<git::OpControl>>,
) -> Result<(), String> {
    let reset = mutation.first().is_some_and(|s| s == "reset");
    let names: Vec<String> = if reset {
        git::run(Some(path), &["symbolic-ref", "-q", "HEAD"])
            .ok()
            .map(|s| vec![s.trim().to_owned()])
            .unwrap_or_default()
    } else {
        mutation
            .iter()
            .skip(2)
            .filter(|s| s.as_str() != "--")
            .map(|name| format!("refs/heads/{name}"))
            .collect()
    };
    let local: Vec<Vec<String>> = ops
        .iter()
        .filter(|op| {
            expected_ref(op).is_some_and(|(name, _)| names.iter().any(|n| n == name))
                || (reset && expected_symref(op).is_some())
        })
        .cloned()
        .collect();
    check_expected(path, &local, control)
}

// A network push may have allowed an unrelated local commit. Prefer the result this
// worker actually produced; disk is only a fallback for refs the operation did not write.
pub(in crate::repo) fn fill_expected(
    path: &Path,
    ops: &mut [Vec<String>],
    owned: &[(String, String)],
) {
    for op in ops {
        if let Some((name, _)) = expected_ref(op) {
            let now = owned.iter().rev().find(|(r, _)| r == name).map_or_else(
                || git::resolve(path, name).unwrap_or_default(),
                |(_, tip)| tip.clone(),
            );
            if let Some(value) = op.get_mut(2) {
                *value = now;
            }
        }
    }
}

/// How the app stashes everything, untracked files included.
pub(in crate::repo) fn stash_push_args() -> [&'static str; 3] {
    ["stash", "push", "--include-untracked"]
}
