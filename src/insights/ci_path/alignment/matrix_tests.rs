use super::*;
use crate::ci::timing::RunInfo;

fn run(name: &str) -> TimedRun {
    TimedRun {
        info: RunInfo {
            workflow_id: Some(7),
            ..RunInfo::default()
        },
        jobs: vec![TimedJob {
            name: name.into(),
            ..TimedJob::default()
        }],
        ..TimedRun::default()
    }
}

#[test]
fn unnamed_matrix_axes_are_bound_by_name_even_when_axis_order_changes() -> Result<(), String> {
    let before = workflow::parse_github(
        "jobs:\n  test:\n    strategy:\n      matrix:\n        os: [blue, red]\n        color: [blue, red]\n",
    )?;
    let after = workflow::parse_github(
        "jobs:\n  test:\n    strategy:\n      matrix:\n        color: [blue, red]\n        os: [blue, red]\n",
    )?;
    let misleading = jobs(
        &run("test (blue, red)"),
        &run("test (blue, red)"),
        Some(&after),
        Some(&before),
    );
    assert!(
        misleading
            .iter()
            .all(|pair| pair.now.is_none() || pair.before.is_none())
    );
    let correct = jobs(
        &run("test (red, blue)"),
        &run("test (blue, red)"),
        Some(&after),
        Some(&before),
    );
    assert_eq!(correct[0].before, Some(0));
    let opaque = workflow::parse_github(
        "jobs:\n  test:\n    strategy:\n      matrix: ${{ fromJSON(needs.setup.outputs.matrix) }}\n",
    )?;
    assert!(
        jobs(
            &run("test (blue, red)"),
            &run("test (blue, red)"),
            Some(&opaque),
            Some(&opaque)
        )
        .iter()
        .all(|pair| pair.now.is_none() || pair.before.is_none())
    );
    Ok(())
}
