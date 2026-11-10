//! Execution responsibilities for contracts.
use super::*;

pub(in crate::repo::review_workflow) fn resolve(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "rev-parse",
            "--verify",
            "--end-of-options",
            &format!("{}^{{commit}}", rev.trim()),
        ],
    )
    .map(|s| s.trim().into())
}
pub(in crate::repo::review_workflow) fn observe(
    repo: &Path,
    commit: &str,
    tree: &str,
    command: &str,
    stop: &git::RunStop,
) -> Result<Observation, String> {
    let (end, lines) = git::run_command(repo, command, &stop.command_stopper())?;
    Ok(Observation {
        commit: commit.into(),
        tree: tree.into(),
        command: command.into(),
        code: end.code,
        stopped: end.stopped,
        lines,
        machine: crate::remote::parse(repo).map_or_else(|| "local".into(), |t| t.host.label()),
    })
}
pub(super) fn patch(repo: &Path, rev: &str) -> Result<String, String> {
    git::run(
        Some(repo),
        &[
            "show",
            "--format=",
            "--no-ext-diff",
            "--no-textconv",
            rev,
            "--",
        ],
    )
}
pub(super) fn targets(
    repo: &Path,
    text: &str,
) -> Result<Vec<(String, String, Option<String>)>, String> {
    let mut targets = BTreeMap::new();
    for line in text.lines().map(str::trim).filter(|s| !s.is_empty()) {
        let (target, adaptation) = line
            .split_once('=')
            .map_or((line, None), |(a, b)| (a.trim(), Some(b.trim())));
        targets.insert(
            target.to_owned(),
            (
                resolve(repo, target)?,
                adaptation.map(|rev| resolve(repo, rev)).transpose()?,
            ),
        );
    }
    if targets.is_empty() || targets.len() > 32 {
        return Err("Choose between 1 and 32 target revisions, one per line".into());
    }
    Ok(targets
        .into_iter()
        .map(|(name, (tip, adaptation))| (name, tip, adaptation))
        .collect())
}
pub(super) fn evidence_roots(
    fix: &str,
    target: &str,
    source: &str,
    tree: Option<&str>,
) -> Vec<(String, String)> {
    let mut roots = vec![];
    for oid in [fix, target, source] {
        roots.push((
            format!("refs/gitcraft/backport-objects/commits/{oid}"),
            oid.into(),
        ));
    }
    if let Some(tree) = tree {
        roots.push((
            format!("refs/gitcraft/backport-objects/trees/{tree}"),
            tree.into(),
        ));
    }
    roots
}
pub(super) fn retain_evidence(
    repo: &Path,
    fix: &str,
    target: &str,
    source: &str,
    tree: Option<&str>,
) -> Result<(), String> {
    for (reference, oid) in evidence_roots(fix, target, source, tree) {
        git::run(Some(repo), &["update-ref", &reference, &oid])?;
    }
    Ok(())
}
pub(super) fn run(repo: &Path, plan: &Plan, stop: &git::RunStop) -> Result<Vec<Row>, String> {
    if !plan.trusted || plan.command.trim().is_empty() {
        return Err("Explicitly trust and enter the regression command first".into());
    }
    let fix = resolve(repo, &plan.fix)?;
    let targets = targets(repo, &plan.targets)?;
    let original_patch = patch(repo, &fix)?;
    let mut rows = vec![];
    for (target, tip, adaptation) in targets {
        let source = adaptation.as_ref().unwrap_or(&fix).clone();
        retain_evidence(repo, &fix, &tip, &source, None)?;
        let mut row = Row {
            target,
            tip: tip.clone(),
            original: fix.clone(),
            source: source.clone(),
            applied: None,
            error: None,
            test: None,
            adaptation: None,
            cleanup: None,
        };
        if stop.stopped() {
            row.error = Some("Cancelled before patch application; behavior not tested".into());
            rows.push(row);
            continue;
        }
        if adaptation.is_some() {
            row.adaptation = Some((original_patch.clone(), patch(repo, &source)?));
        }
        let scratch = git::Scratch::add_empty(repo, "rehearse", &tip)?;
        let result = (|| {
            scratch.checkout(&tip)?;
            git::run(
                Some(&scratch.path),
                &[
                    "-c",
                    "core.hooksPath=/dev/null",
                    "cherry-pick",
                    "--no-commit",
                    &source,
                ],
            )?;
            let tree = git::run(Some(&scratch.path), &["write-tree"])?
                .trim()
                .to_owned();
            // A ref directly to the tree is a real Git reachability root. JSON alone is not.
            retain_evidence(repo, &fix, &tip, &source, Some(&tree))?;
            row.applied = Some(tree.clone());
            if adaptation.is_some() {
                row.adaptation = Some((original_patch.clone(), patch(repo, &source)?));
            }
            if !stop.stopped() {
                row.test = Some(observe(&scratch.path, &tip, &tree, &plan.command, stop)?);
            }
            Ok::<_, String>(())
        })();
        if let Err(e) = result {
            row.error = Some(e);
        }
        row.cleanup = scratch.remove().err();
        rows.push(row);
    }
    let evidence = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
    store::save(
        repo,
        "backport-contracts",
        &format!("evidence-{:x}", md5::compute(evidence)),
        &rows,
    )?;
    store::save(repo, "backport-contracts", "latest", &rows)?;
    Ok(rows)
}
