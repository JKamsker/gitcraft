//! Evidence responsibilities for radar.
use super::*;

pub(super) fn history(repo: &Path, tip: &str) -> Result<HistoryEvidence, String> {
    let limit = format!("--max-count={}", HISTORY + 1);
    let count = git::run(Some(repo), &["rev-list", "--no-merges", &limit, tip, "--"])?
        .lines()
        .count();
    let limit = format!("--max-count={HISTORY}");
    let patches = git::run(
        Some(repo),
        &[
            "log",
            "--no-merges",
            &limit,
            "--format=medium",
            "--src-prefix=a/",
            "--dst-prefix=b/",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "-p",
            tip,
            "--",
        ],
    )?;
    let ids = git::run_input(Some(repo), &["patch-id", "--stable"], &patches)?
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?.into(), words.next()?.into()))
        })
        .collect();
    Ok((ids, count.min(HISTORY), count <= HISTORY))
}
pub(super) fn status(
    ids: &[(String, String)],
    fix: &str,
    reverse: &str,
    adaptation: Option<&str>,
    repo: &Path,
    tip: &str,
) -> Patch {
    let adaptation = adaptation
        .filter(|rev| git::is_ancestor(repo, rev, tip))
        .and_then(|rev| {
            Some((
                rev,
                patch_id(repo, rev, false).ok()?,
                patch_id(repo, rev, true).ok()?,
            ))
        });
    patch_status(
        ids,
        fix,
        reverse,
        adaptation
            .as_ref()
            .map(|(rev, id, reverse)| (*rev, id.as_str(), reverse.as_str())),
    )
}
pub(super) fn patch_status(
    ids: &[(String, String)],
    fix: &str,
    reverse: &str,
    adaptation: Option<(&str, &str, &str)>,
) -> Patch {
    // History is newest first. Positive and reverse witnesses for BOTH patch identities
    // participate in one ordering; ancestry by itself is never survival evidence.
    for (id, commit) in ids {
        if id == reverse {
            return Patch::Reverted(commit.clone());
        }
        if id == fix {
            return Patch::Same(commit.clone());
        }
        if let Some((adapted, patch, reverse)) = adaptation {
            if id == reverse {
                return Patch::Reverted(commit.clone());
            }
            if id == patch {
                return Patch::Adapted(adapted.into());
            }
        }
    }
    Patch::NotChecked
}

pub(super) fn check(repo: &Path, pins: &[Pin]) -> Result<Vec<Row>, String> {
    let evidence = store::load::<BTreeMap<String, Observation>>(repo, "fix-tests", "results")?
        .unwrap_or_default();
    let mut rows = vec![];
    for pin in pins {
        let (id, reverse) = (
            patch_id(repo, &pin.fix, false)?,
            patch_id(repo, &pin.fix, true)?,
        );
        for (target, adaptation) in &pin.targets {
            let tip = contracts::resolve(repo, target)?;
            let (ids, coverage, complete) = history(repo, &tip)?;
            let tree = git::run(Some(repo), &["rev-parse", &format!("{tip}^{{tree}}")])?
                .trim()
                .to_owned();
            let key = evidence_key(&pin.fix, &tree, &pin.command);
            rows.push(Row {
                name: pin.name.clone(),
                fix: pin.fix.clone(),
                target: target.clone(),
                tip: tip.clone(),
                patch: status(&ids, &id, &reverse, adaptation.as_deref(), repo, &tip),
                test: evidence.get(&key).cloned(),
                coverage,
                complete,
                error: None,
            });
        }
    }
    Ok(rows)
}
pub(super) fn evidence_key(fix: &str, tree: &str, command: &str) -> String {
    format!("{:x}", md5::compute(format!("{fix}\0{tree}\0{command}")))
}
pub(super) fn test(repo: &Path, pin: &Pin, stop: &git::RunStop) -> Result<(), String> {
    if pin.command.trim().is_empty() {
        return Err("Give the pinned fix a regression command".into());
    }
    let mut evidence = store::load::<BTreeMap<String, Observation>>(repo, "fix-tests", "results")?
        .unwrap_or_default();
    for (target, _) in &pin.targets {
        if stop.stopped() {
            break;
        }
        let tip = contracts::resolve(repo, target)?;
        git::run(
            Some(repo),
            &[
                "update-ref",
                &format!("refs/gitcraft/fix-test-inputs/{}/{tip}", pin.fix),
                &tip,
            ],
        )?;
        let scratch = git::Scratch::add_empty(repo, "test", &tip)?;
        scratch.checkout(&tip)?;
        let tree = git::run(Some(&scratch.path), &["rev-parse", "HEAD^{tree}"])?
            .trim()
            .to_owned();
        let observation = contracts::observe(&scratch.path, &tip, &tree, &pin.command, stop)?;
        scratch.remove()?;
        // A stopped command remains explicitly stopped; it never becomes a passing witness.
        let key = evidence_key(&pin.fix, &tree, &pin.command);
        store::save(repo, "fix-test-evidence", &key, &observation)?;
        evidence.insert(key, observation);
        store::save(repo, "fix-tests", "results", &evidence)?;
    }
    Ok(())
}
