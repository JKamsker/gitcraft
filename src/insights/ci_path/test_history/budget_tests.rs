//! Shared logs, aggregate entry limits and retaining the highest-priority signal rows.
use super::tests::{explorer, log, run};
use super::*;

#[test]
fn duplicate_jobs_and_cloned_evidence_share_the_same_parsed_log() -> Result<(), String> {
    let explorer = explorer(run(60, 1, "abc", 2), Some(run(59, 1, "old", 1)), Vec::new());
    let mut plan = plan(&explorer, 0).ok_or("job")?;
    plan.before = Some(plan.now.clone());
    let mut fetched = 0;
    let evidence = read(plan, |_| {
        fetched += 1;
        Ok((log(&[("t", "FAILED")]), false))
    });
    assert_eq!(fetched, 1);
    let before = evidence
        .before
        .as_ref()
        .ok_or("baseline")?
        .log
        .as_ref()
        .map_err(String::as_str)?;
    let now = evidence.now.log.as_ref().map_err(String::as_str)?;
    assert!(Arc::ptr_eq(now, before));
    let clone = evidence.clone();
    assert!(Arc::ptr_eq(
        now,
        clone.now.log.as_ref().map_err(String::as_str)?
    ));
    Ok(())
}

#[test]
fn aggregate_budget_is_shared_across_jobs_and_exhausted_logs_are_incomplete() -> Result<(), String>
{
    let current = run(70, 2, "abc", 2);
    let explorer = explorer(
        current.clone(),
        Some(run(69, 1, "old", 1)),
        vec![run(70, 1, "abc", 2), current],
    );
    let plan = plan(&explorer, 0).ok_or("job")?;
    let text = log(&[("a", "FAILED"), ("b", "FAILED"), ("c", "FAILED")]);
    let mut fetched = Vec::new();
    let evidence = read_bounded(
        plan,
        |job| {
            fetched.push(job);
            Ok((text.clone(), false))
        },
        4,
    );
    assert_eq!(
        fetched.len(),
        2,
        "after the budget is spent a third log is not downloaded"
    );
    assert_eq!(evidence.now.report(), Some(Report::Complete));
    let before = evidence.before.as_ref().ok_or("baseline")?;
    assert_eq!(before.report(), Some(Report::Incomplete));
    assert_eq!(evidence.history[0].report(), Some(Report::Incomplete));
    let retained: usize = [&evidence.now, before, &evidence.history[0]]
        .into_iter()
        .map(|read| {
            read.log
                .as_ref()
                .map(|log| log.tests.len())
                .unwrap_or_default()
        })
        .sum();
    assert_eq!(retained, 4);
    Ok(())
}

#[test]
fn late_high_priority_signals_displace_rows_without_losing_counts() -> Result<(), String> {
    let explorer = explorer(run(80, 1, "abc", 2), Some(run(79, 1, "old", 1)), Vec::new());
    let plan = plan(&explorer, 0).ok_or("job")?;
    let names: Vec<String> = (0..MAX_ROWS + 50)
        .map(|i| format!("a{i:04}"))
        .chain(["z_new".into()])
        .collect();
    let evidence = read(plan, |job| {
        let tests: Vec<_> = names
            .iter()
            .map(|name| {
                (
                    name.as_str(),
                    if job == 7910 && name == "z_new" {
                        "ok"
                    } else {
                        "FAILED"
                    },
                )
            })
            .collect();
        Ok((log(&tests), false))
    });
    assert_eq!(evidence.rows.len(), MAX_ROWS);
    assert_eq!(evidence.more, names.len() - MAX_ROWS);
    assert_eq!(evidence.rows[0].test.1, "z_new");
    assert_eq!(evidence.rows[0].transition, Transition::NewlyFailed);
    assert_eq!(evidence.rows.last().ok_or("last row")?.test.1, "a0198");
    assert!(evidence.rows.iter().all(|row| row.history.len() == 1));
    Ok(())
}
