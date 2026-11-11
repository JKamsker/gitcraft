use super::*;

#[test]
fn fluent_partition_doctor_actual_selection_keeps_confirmation_and_argv_in_both_languages()
-> Result<(), String> {
    // Supplied state only: health::suggestions and turn_on_action do no discovery or I/O.
    let supplied = Health {
        objects: health::Objects {
            loose: 5000,
            ..Default::default()
        },
        fsmonitor: Some(State::Off),
        ..Default::default()
    };
    let titles = [
        "Commit-graph",
        "Scheduled maintenance",
        "File system monitor",
        "Untracked cache",
        "Pack the objects",
    ];
    let german = [
        "Commit-Graph",
        "Geplante Wartung",
        "Dateisystemmonitor",
        "Cache unversionierter Dateien",
        "Objekte packen",
    ];
    let expected_ops = [
        vec![
            vec!["commit-graph", "write", "--reachable", "--changed-paths"],
            vec!["config", "--local", "fetch.writeCommitGraph", "true"],
        ],
        vec![vec!["maintenance", "start"]],
        vec![vec!["config", "--local", "core.fsmonitor", "true"]],
        vec![
            vec!["update-index", "--test-untracked-cache"],
            vec!["config", "--local", "core.untrackedCache", "true"],
            vec!["update-index", "--untracked-cache"],
        ],
        vec![vec!["gc", "--quiet"]],
    ];
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let suggestions = health::suggestions(&supplied);
        assert_eq!(suggestions.len(), titles.len());
        for (index, suggestion) in suggestions.iter().enumerate() {
            assert_eq!(suggestion.title, titles[index]);
            assert_eq!(
                suggestion.display_title(),
                if language == "de" {
                    german[index]
                } else {
                    titles[index]
                }
            );
            assert_eq!(suggestion.state, State::Off);
            assert_eq!(suggestion.ops, expected_ops[index]);
            let commands: Vec<_> = suggestion
                .ops
                .iter()
                .map(|op| format!("git {}", op.join(" ")))
                .collect();
            // This is the same selection function called by the actual Turn on button.
            let action = turn_on_action(suggestion, &commands);
            let expected_label = format!("Turn on: {}", titles[index].to_lowercase());
            match action {
                Action::Doctor(DoctorAct::Confirm(confirmation)) if index == 1 => {
                    let (title, body, button) = confirmation.displayed();
                    assert_eq!(
                        title,
                        if language == "de" {
                            "Wartung planen?"
                        } else {
                            "Schedule maintenance?"
                        }
                    );
                    assert_eq!(
                        button,
                        if language == "de" {
                            "Planen"
                        } else {
                            "Schedule"
                        }
                    );
                    assert!(body.starts_with("git maintenance start "));
                    assert!(body.contains(if language == "de" {
                        "trägt dieses Repository"
                    } else {
                        "adds this repository"
                    }));
                    for literal in [
                        "maintenance.repo",
                        "maintenance.auto=false",
                        "maintenance.strategy=incremental",
                        "refs/prefetch",
                        "git maintenance unregister",
                        "git maintenance stop",
                    ] {
                        assert!(body.contains(literal), "missing literal {literal}");
                    }
                    assert_eq!(confirmation.label, expected_label);
                    assert_eq!(confirmation.ops, expected_ops[index]);
                }
                Action::Doctor(DoctorAct::Run { label, ops }) if index != 1 => {
                    assert_eq!(label, expected_label);
                    assert_eq!(ops, expected_ops[index]);
                }
                _ => {
                    return Err(format!(
                        "incorrect confirmation/run selection for {language}: {}",
                        suggestion.title
                    ));
                }
            }
        }
        let unsupported = health::suggestions(&Health {
            fsmonitor: Some(State::Unsupported),
            ..supplied.clone()
        });
        assert_eq!(unsupported[2].title, "File system monitor");
        assert_eq!(unsupported[2].state, State::Unsupported);
        assert_eq!(unsupported[4].title, "Pack the objects");
        let opaque = Suggestion {
            title: "Geplante Wartung / external",
            why: String::new(),
            state: State::Off,
            ops: vec![vec!["opaque-argv".into()]],
        };
        assert_eq!(opaque.display_title(), opaque.title);
        match turn_on_action(&opaque, &["git opaque-argv".into()]) {
            Action::Doctor(DoctorAct::Run { label, ops }) => {
                assert_eq!(label, "Turn on: geplante wartung / external");
                assert_eq!(ops, opaque.ops);
            }
            _ => return Err("unknown title changed confirmation identity".into()),
        }
    }
    crate::i18n::set_language("en-US");
    Ok(())
}
