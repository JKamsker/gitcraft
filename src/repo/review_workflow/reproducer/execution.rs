//! Execution responsibilities for reproducer.
use super::*;

pub(super) fn token() -> String {
    format!(
        "{:x}",
        md5::compute(format!(
            "{}:{}:{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0),
            IDS.fetch_add(1, Ordering::Relaxed)
        ))
    )
}
pub(super) fn run(
    repo: &Path,
    mut case: Case,
    stop: &crate::tasks::Cancellation,
) -> Result<Report, String> {
    if !case.approved {
        return Err("Inspect the source and explicitly approve sandbox execution first".into());
    }
    if case.source.is_empty() || case.source.len() > 64 * 1024 {
        return Err("The generated integration test must be 1–65536 bytes".into());
    }
    case.base = materialize::resolve(repo, &case.base)?;
    case.head = materialize::resolve(repo, &case.head)?;
    case.image = containers::image(repo, &case.image)?;
    for rev in [&case.base, &case.head] {
        materialize::text(
            repo,
            &[
                "update-ref",
                &format!("refs/gitcraft/reproducer-inputs/{rev}"),
                rev,
            ],
        )?;
    }
    let base_tree = materialize::text(repo, &["rev-parse", &format!("{}^{{tree}}", case.base)])?
        .trim()
        .to_owned();
    let head_tree = materialize::text(repo, &["rev-parse", &format!("{}^{{tree}}", case.head)])?
        .trim()
        .to_owned();
    let (base, base_cleanup) = containers::run_one(repo, &case.base, &base_tree, &case, &token())?;
    let mut cleanup = vec![base_cleanup];
    let head = if stop.is_cancelled() {
        Observation {
            commit: case.head.clone(),
            tree: head_tree.clone(),
            command: "No head run after cancellation".into(),
            code: None,
            stopped: true,
            lines: vec!["Stopped after base; no head comparison was made".into()],
            machine: base.machine.clone(),
        }
    } else {
        match containers::run_one(repo, &case.head, &head_tree, &case, &token()) {
            Ok((head, evidence)) => {
                cleanup.push(evidence);
                head
            }
            Err(error) => Observation {
                commit: case.head.clone(),
                tree: head_tree,
                command: "Head source preparation did not complete".into(),
                code: None,
                stopped: true,
                lines: vec![error],
                machine: base.machine.clone(),
            },
        }
    };
    let report = Report {
        case,
        base,
        head,
        cleanup: cleanup
            .into_iter()
            .filter(|e| e.result.starts_with("Cleanup needs attention"))
            .map(|e| {
                format!(
                    "{} ({}) · {}",
                    e.name,
                    e.id.as_deref().unwrap_or("unknown ID"),
                    e.result
                )
            })
            .collect(),
    };
    // Stop cancels execution, not recording its partial observations and bounded cleanup.
    crate::tasks::with_cancellation(crate::tasks::Cancellation::default(), || {
        let text = serde_json::to_string(&report).map_err(|e| e.to_string())?;
        materialize::save(
            repo,
            "reproducer-evidence",
            &format!("{:x}", md5::compute(text)),
            &report,
        )?;
        materialize::save(repo, "reproducer-evidence", "latest", &report)
    })?;
    Ok(report)
}
