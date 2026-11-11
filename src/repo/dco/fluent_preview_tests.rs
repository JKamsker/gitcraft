use super::*;
#[test]
fn fluent_partition_dco_actual_preview_keeps_raw_labels_authors_and_numeric_plan() {
    let plan = Plan {
        branch: "feature/ß".into(),
        head: "abcdef1234567890".into(),
        base: "12345678abcdef00".into(),
        commits: (0..1234)
            .map(|index| Signed {
                hash: format!("{index:016x}"),
                summary: "Opaque subject 日本語".into(),
                author: "Other Author / ß".into(),
                email: "other@example.test".into(),
                merge: false,
                bot: false,
                signed: false,
            })
            .collect(),
        push: None,
    };
    let raw = plan.clone();
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        let body = fix_preview(&plan, "Me <me@example.test>", "me@example.test");
        assert!(body.contains("feature/ß") && body.contains("12345678"));
        assert!(body.contains("Signed-off-by: Me <me@example.test>"));
        assert!(body.contains("Opaque subject 日本語") && body.contains("Other Author / ß"));
        assert!(body.contains(if language == "de" {
            "1.234 Commits"
        } else {
            "1,234 commits"
        }));
        assert!(body.contains(if language == "de" {
            "1.226 weitere"
        } else {
            "1,226 more"
        }));
        assert!(body.contains(if language == "de" {
            "nur fort, wenn Sie dazu berechtigt sind"
        } else {
            "Only go ahead if you may"
        }));
        assert!(body.contains(if language == "de" {
            "nichts gepusht"
        } else {
            "nothing is pushed"
        }));
        assert_eq!(fix_label(&plan), "Sign off the commits of feature/ß");
        assert_eq!(plan, raw);
        assert_eq!(plan.unsigned().len(), 1234);
        assert_eq!(plan.by_others("me@example.test").len(), 1234);
    }
    crate::i18n::set_language("en-US");
}
#[test]
fn fluent_dco_reviewed_push_preview_keeps_lease_authority_and_local_undo() {
    let mut plan = Plan {
        branch: "topic/opaque-1234".into(),
        head: "abcdef1234567890".into(),
        base: "12345678abcdef00".into(),
        commits: vec![Signed {
            hash: "abcdef1234567890".into(),
            summary: "Opaque subject".into(),
            author: "Me".into(),
            email: "me@example.test".into(),
            merge: false,
            bot: false,
            signed: false,
        }],
        push: None,
    };
    for seen in [None, Some("0123456789abcdef".to_owned())] {
        plan.push = Some(signoff::PushTarget {
            remote: "origin-1234".into(),
            urls: vec!["https://example.invalid/opaque-1234".into()],
            remote_ref: "refs/heads/topic-1234".into(),
            tracking: Some("refs/remotes/origin-1234/topic-1234".into()),
            seen: seen.clone(),
        });
        let before = plan.clone();
        for language in ["en-US", "de"] {
            crate::i18n::set_language(language);
            let body = fix_preview(&plan, "Me <me@example.test>", "me@example.test");
            assert!(body.contains("origin-1234/topic-1234"));
            assert!(body.contains("Signed-off-by: Me <me@example.test>"));
            assert!(body.contains(if language == "de" {
                "Prüfen und genehmigen Sie diesen Push gesondert"
            } else {
                "Review and approve that push separately"
            }));
            assert!(body.contains(match (language, seen.is_some()) {
                ("de", true) => "seit dem letzten Abruf jemand gepusht",
                ("de", false) => "dieser Branch dort bereits existiert",
                (_, true) => "someone pushed there since it was last fetched",
                (_, false) => "that branch exists there already",
            }));
            assert!(body.contains(if language == "de" {
                "Strg+Z stellt die lokalen Commits wieder her"
            } else {
                "Ctrl+Z restores the local commits"
            }));
            assert!(body.contains(if language == "de" {
                "eigenen geprüften Push"
            } else {
                "requires its own reviewed push"
            }));
            assert!(!body.contains("https://example.invalid"));
            assert_eq!(plan, before);
            assert_eq!(
                fix_label(&plan),
                "Sign off the commits of topic/opaque-1234"
            );
        }
    }
    crate::i18n::set_language("en-US");
}
