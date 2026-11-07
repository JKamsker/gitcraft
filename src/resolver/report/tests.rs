use std::time::{Duration, Instant};

use super::*;

fn planned(id: &str, title: &str) -> PlannedStep {
    PlannedStep {
        id: id.to_owned(),
        title: title.to_owned(),
        file: Some(format!("{id}.rs")),
    }
}

fn step(id: &str, status: StepStatus) -> Report {
    Report::Step {
        id: id.to_owned(),
        status,
        note: None,
    }
}

#[test]
fn a_report_reads_back_as_written() {
    let report = Report::Step {
        id: "a".into(),
        status: StepStatus::Running,
        note: Some("reading both sides".into()),
    };
    let text = serde_json::to_string(&report).unwrap_or_default();
    assert_eq!(
        text,
        r#"{"kind":"step","id":"a","status":"running","note":"reading both sides"}"#
    );
    assert_eq!(serde_json::from_str::<Report>(&text).ok(), Some(report));
}

#[test]
fn steps_go_from_planned_to_done_and_count_towards_the_bar() {
    let t0 = Instant::now();
    let mut p = Progress::default();
    p.apply(
        Report::Plan {
            steps: vec![planned("a", "Resolve a"), planned("b", "Resolve b")],
            estimate_minutes: Some(4),
        },
        t0,
    );
    assert_eq!(p.counts(), (0, 2));
    assert_eq!(p.fraction(), 0.0);
    p.apply(step("a", StepStatus::Running), t0);
    assert_eq!(p.fraction(), 0.25);
    p.apply(step("a", StepStatus::Done), t0 + Duration::from_secs(60));
    assert_eq!(p.counts(), (1, 2));
    assert_eq!(p.fraction(), 0.5);
    p.apply(
        Report::Finish {
            ok: true,
            summary: "both resolved".into(),
        },
        t0,
    );
    assert_eq!(p.fraction(), 1.0);
    assert_eq!(p.remaining(t0), None);
}

#[test]
fn time_left_comes_from_the_estimate_then_from_the_steps_done() {
    let t0 = Instant::now();
    let mut p = Progress::default();
    p.apply(
        Report::Plan {
            steps: vec![planned("a", "A"), planned("b", "B"), planned("c", "C")],
            estimate_minutes: Some(10),
        },
        t0,
    );
    // Before a step is over, the agent's estimate counts down.
    assert_eq!(
        p.remaining(t0 + Duration::from_secs(120)),
        Some(Duration::from_secs(480))
    );
    p.apply(step("a", StepStatus::Running), t0);
    p.apply(step("a", StepStatus::Done), t0 + Duration::from_secs(30));
    // Then two steps of 30 s each are left.
    assert_eq!(
        p.remaining(t0 + Duration::from_secs(30)),
        Some(Duration::from_secs(60))
    );
    // The running one is part way through.
    p.apply(step("b", StepStatus::Running), t0 + Duration::from_secs(30));
    assert_eq!(
        p.remaining(t0 + Duration::from_secs(40)),
        Some(Duration::from_secs(50))
    );
}

#[test]
fn steps_reported_over_at_once_dont_bring_the_estimate_down() {
    let t0 = Instant::now();
    let mut p = Progress::default();
    p.apply(
        Report::Plan {
            steps: vec![planned("a", "A"), planned("b", "B"), planned("c", "C")],
            estimate_minutes: Some(10),
        },
        t0,
    );
    // Done without ever running: no time to learn from, so the estimate still counts.
    p.apply(step("a", StepStatus::Done), t0 + Duration::from_secs(60));
    p.apply(step("b", StepStatus::Skipped), t0 + Duration::from_secs(60));
    assert_eq!(
        p.remaining(t0 + Duration::from_secs(60)),
        Some(Duration::from_secs(540))
    );
}

#[test]
fn a_new_plan_keeps_how_known_steps_stand_and_unplanned_steps_are_added() {
    let t0 = Instant::now();
    let mut p = Progress::default();
    p.apply(
        Report::Plan {
            steps: vec![planned("a", "A")],
            estimate_minutes: None,
        },
        t0,
    );
    p.apply(step("a", StepStatus::Done), t0);
    p.apply(
        Report::Plan {
            steps: vec![planned("a", ""), planned("b", "B"), planned("b", "again")],
            estimate_minutes: None,
        },
        t0,
    );
    assert_eq!(p.steps.len(), 2, "a repeated id is taken once");
    assert_eq!(p.steps[0].status, StepStatus::Done);
    assert_eq!(p.steps[0].title, "A", "an empty title keeps the old one");
    p.apply(step("verify", StepStatus::Running), t0);
    assert_eq!(p.steps.len(), 3);
    assert_eq!(p.steps[2].title, "verify");
}

#[test]
fn what_the_agent_writes_is_cleaned_and_bounded() {
    assert_eq!(clean("a\u{202e}b\n\tc   d\u{200b}", 100), "ab c d");
    assert_eq!(clean("abcdef", 3), "abc…");
    let t0 = Instant::now();
    let mut p = Progress::default();
    let many = (0..MAX_STEPS + 10)
        .map(|i| planned(&i.to_string(), "x"))
        .collect();
    p.apply(
        Report::Plan {
            steps: many,
            estimate_minutes: Some(100_000),
        },
        t0,
    );
    assert_eq!(p.steps.len(), MAX_STEPS);
    assert_eq!(
        p.remaining(t0),
        Some(Duration::from_secs(u64::from(MAX_ESTIMATE_MINUTES) * 60))
    );
    p.apply(step("one more", StepStatus::Running), t0);
    assert_eq!(p.steps.len(), MAX_STEPS, "no step past the most kept");
}

#[test]
fn times_read_as_a_clock_and_in_words() {
    assert_eq!(clock(Duration::from_secs(65)), "1:05");
    assert_eq!(clock(Duration::from_secs(3723)), "1:02:03");
    assert_eq!(
        left_words(Duration::from_secs(20)),
        "less than a minute left"
    );
    assert_eq!(left_words(Duration::from_secs(170)), "about 3 min left");
}
