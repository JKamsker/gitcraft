use super::*;

const HEAD: &str = "be6c75b85da526349c44e3978374c95e0b80a96d";
const OTHER: &str = "bfe36592eb7f9f2708b5d358b5b5f33ed535c8cf";

fn observed(gate: ServerGate, revision: Option<&str>) -> Observation {
    Observation {
        target: Target::Branch("main".into()),
        gate,
        analysis: Some(Analysis {
            key: Some("A2".into()),
            date: Some("2016-12-12T17:12:45+0100".into()),
            revision: revision.map(str::to_owned),
        }),
        pending: false,
        last_task_failed: false,
        failing: Vec::new(),
        dashboard: String::new(),
    }
}

#[test]
fn a_verified_ok_analysis_of_the_open_commit_passes_and_an_error_fails() {
    assert_eq!(
        evaluate(&observed(ServerGate::Ok, Some(HEAD)), Some(HEAD)),
        Verdict::Passed
    );
    let upper = HEAD.to_ascii_uppercase();
    assert_eq!(
        evaluate(&observed(ServerGate::Ok, Some(&upper)), Some(HEAD)),
        Verdict::Passed
    );
    assert_eq!(
        evaluate(&observed(ServerGate::Error, Some(HEAD)), Some(HEAD)),
        Verdict::Failed
    );
    assert_eq!(
        evaluate(&observed(ServerGate::Warn, Some(HEAD)), Some(HEAD)),
        Verdict::Failed
    );
}

#[test]
fn nothing_unverified_is_ever_shown_as_passed() {
    let cases = [
        (observed(ServerGate::Ok, None), Some(HEAD)),
        (observed(ServerGate::Ok, Some(OTHER)), Some(HEAD)),
        (observed(ServerGate::Ok, Some(&HEAD[..12])), Some(HEAD)),
        (observed(ServerGate::Ok, Some(HEAD)), None),
        (observed(ServerGate::Ok, Some(HEAD)), Some(&HEAD[..8])),
        (observed(ServerGate::Ok, Some("not-a-sha")), Some(HEAD)),
        (observed(ServerGate::Unknown, Some(HEAD)), Some(HEAD)),
        (observed(ServerGate::None, Some(HEAD)), Some(HEAD)),
        (
            Observation {
                pending: true,
                ..observed(ServerGate::Ok, Some(HEAD))
            },
            Some(HEAD),
        ),
        (
            Observation {
                last_task_failed: true,
                ..observed(ServerGate::Ok, Some(HEAD))
            },
            Some(HEAD),
        ),
        (
            Observation {
                analysis: None,
                ..observed(ServerGate::Ok, Some(HEAD))
            },
            Some(HEAD),
        ),
    ];
    for (observation, open) in cases {
        assert_ne!(
            evaluate(&observation, open),
            Verdict::Passed,
            "{observation:?} / {open:?}"
        );
    }
}

#[test]
fn stale_reasons_and_unavailable_states_are_specific() {
    assert_eq!(
        evaluate(&observed(ServerGate::Ok, Some(OTHER)), Some(HEAD)),
        Verdict::Stale(
            Stale::Mismatch {
                analyzed: OTHER.into(),
                open: HEAD.into()
            },
            ServerGate::Ok
        )
    );
    assert_eq!(
        evaluate(&observed(ServerGate::Error, None), Some(HEAD)),
        Verdict::Stale(Stale::RevisionUnknown, ServerGate::Error)
    );
    assert_eq!(
        evaluate(&observed(ServerGate::Ok, Some(HEAD)), None),
        Verdict::Stale(Stale::HeadUnknown, ServerGate::Ok)
    );
    let failed_task = Observation {
        last_task_failed: true,
        ..observed(ServerGate::Ok, Some(HEAD))
    };
    assert_eq!(
        evaluate(&failed_task, Some(HEAD)),
        Verdict::Stale(Stale::TaskFailed, ServerGate::Ok)
    );
    let never = Observation {
        analysis: None,
        ..observed(ServerGate::Unknown, None)
    };
    assert_eq!(
        evaluate(&never, Some(HEAD)),
        Verdict::Unavailable(Unavailable::NotAnalyzed)
    );
    let queued = Observation {
        pending: true,
        ..never
    };
    assert_eq!(evaluate(&queued, Some(HEAD)), Verdict::Pending);
    assert_eq!(
        evaluate(&observed(ServerGate::None, Some(HEAD)), Some(HEAD)),
        Verdict::Unavailable(Unavailable::NoGate)
    );
}

#[test]
fn sha256_object_ids_compare_like_sha1() {
    let long = "a".repeat(64);
    assert_eq!(
        evaluate(&observed(ServerGate::Ok, Some(&long)), Some(&long)),
        Verdict::Passed
    );
}
