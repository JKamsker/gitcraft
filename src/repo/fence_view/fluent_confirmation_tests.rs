use super::*;
#[test]
fn fluent_partition_fence_actual_confirmation_keeps_paths_and_checks_raw() {
    let violation = Violation {
        path: "secret/ß file".into(),
        pattern: "secret/**".into(),
        how: fence::How::Untracked,
    };
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for has_found in [false, true] {
            for has_errors in [false, true] {
                for worktree in [None, Some("/supplied/ß tree")] {
                    let found = if has_found {
                        vec![violation.clone()]
                    } else {
                        vec![]
                    };
                    let errors = if has_errors {
                        vec!["Opaque provider detail 日本語".into()]
                    } else {
                        vec![]
                    };
                    let body = confirm_body("feature/ß", worktree, &found, &errors);
                    assert!(body.starts_with("feature/ß"));
                    assert_eq!(body.contains("/supplied/ß tree"), worktree.is_some());
                    assert_eq!(body.contains("secret/ß file"), has_found);
                    assert_eq!(body.contains("secret/**"), has_found);
                    assert_eq!(body.contains("Opaque provider detail 日本語"), has_errors);
                    assert!(body.ends_with(if language == "de" {
                        "Trotzdem fortfahren?"
                    } else {
                        "Go ahead all the same?"
                    }));
                    if has_found {
                        assert!(body.contains(if language == "de" {
                            "Agenten nicht verändern dürfen"
                        } else {
                            "agents must stay out of"
                        }));
                        assert!(body.contains(if language == "de" {
                            "nicht versioniert; Grenze:"
                        } else {
                            "untracked; fence:"
                        }));
                    }
                    if has_errors {
                        assert!(body.contains(if language == "de" {
                            "konnte"
                        } else {
                            "could not"
                        }));
                    }
                    assert_eq!(violation.how.label(), "untracked");
                    assert_eq!(found.len(), usize::from(has_found));
                    assert_eq!(errors.len(), usize::from(has_errors));
                }
            }
        }
    }
    crate::i18n::set_language("en-US");
}
