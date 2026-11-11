use super::*;
#[test]
fn done_step_file_overrides_an_id_that_names_another_file() {
    let now = Instant::now();
    let mut progress = Progress::default();
    progress.apply(
        Report::Plan {
            steps: vec![
                PlannedStep {
                    id: "untouched.txt".into(),
                    title: "Resolve actual".into(),
                    file: Some("actual.txt".into()),
                },
                PlannedStep {
                    id: "fallback.txt".into(),
                    title: "Fallback".into(),
                    file: None,
                },
            ],
            estimate_minutes: None,
        },
        now,
    );
    for id in ["untouched.txt", "fallback.txt"] {
        progress.apply(
            Report::Step {
                id: id.into(),
                status: StepStatus::Done,
                note: None,
            },
            now,
        );
    }
    assert_eq!(done_files(&progress), vec!["actual.txt", "fallback.txt"]);
}
