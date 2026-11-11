use super::*;
#[test]
fn fluent_partition_r1_actual_deferred_action_retains_branch_and_opaque_raw_payload()
-> Result<(), String> {
    let branch = "candidate raw/1234;\n{$x}";
    crate::i18n::set_language("de");
    let Action::Compare {
        base,
        base_label,
        target,
        target_label,
        captions,
        since_fork,
    } = candidate_comparison("raw-original", "raw-candidate", branch)
    else {
        return Err("actual source must compare".into());
    };
    assert_eq!(base, "raw-original");
    assert_eq!(target.as_deref(), Some("raw-candidate"));
    assert!(!since_fork);
    assert_eq!(base_label, format!("original {branch}"));
    assert_eq!(target_label, format!("candidate {branch}"));
    let comparison = crate::repo::Compare {
        base,
        base_label: base_label.clone(),
        target,
        target_label: target_label.clone(),
        captions,
        since_fork,
        data: None,
        request: 29,
    };
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        assert!(comparison.display_base_label().contains(branch));
        assert!(comparison.display_target_label().contains(branch));
        assert_eq!(
            comparison.display_base_label() == base_label,
            lang == "en-US"
        );
        assert_eq!(comparison.base_label, base_label);
        assert_eq!(comparison.target_label, target_label);
        assert_eq!(comparison.request, 29);
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
