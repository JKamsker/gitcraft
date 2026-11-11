use super::*;
#[test]
fn fluent_partition_actual_receipt_sentence_localizes_counts_and_keeps_reflog_identity() {
    let receipt = Receipt {
        branch: "Kunde/ü".into(),
        old_tip: "a".repeat(40),
        new_tip: "b".repeat(40),
        same_files: true,
        unpaired: false,
        pairs: vec![
            Pair {
                kind: PairKind::Same,
                old: Some(Commit {
                    author: "Raw Author <raw@example.test>".into(),
                    ..Commit::default()
                }),
                new: Some(Commit {
                    author: "Raw Author <raw@example.test>".into(),
                    signed: true,
                    ..Commit::default()
                }),
                message_changed: false
            };
            1234
        ],
    };
    for (lang, expected) in [
        ("en-US", "1,234 commits now signed"),
        ("de", "1.234 Commits jetzt signiert"),
    ] {
        crate::i18n::set_language(lang);
        let sentence = receipt.sentence();
        assert!(sentence.contains(expected), "{sentence}");
        assert_eq!(
            super::super::observe::describe_reflog("commit (amend): Raw subject"),
            "Amend “Raw subject”"
        );
        assert_eq!(receipt.branch, "Kunde/ü");
        assert_eq!(receipt.counts().signed_after, 1234);
    }
}
