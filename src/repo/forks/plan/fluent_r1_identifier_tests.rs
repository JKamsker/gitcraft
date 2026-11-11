use super::*;
#[test]
fn fluent_partition_r1_collision_names_preserve_decimal_pr_identity_and_planned_argv()
-> Result<(), String> {
    let info = ForkInfo {
        number: 1234,
        cross: true,
        maintainers_can_edit: true,
        owner: "ann".into(),
        branch: "fix".into(),
        clone_https: "https://example.invalid/ann/repo.git".into(),
        ..Default::default()
    };
    let setup = Setup::default();
    let locals = vec!["fix".into(), "ann/fix".into()];
    let expected = checkout(&info, &[], &setup, &locals, "previous", None)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(expected.local, "pr-1234");
    for lang in ["de", "en-US", "de"] {
        crate::i18n::set_language(lang);
        let planned = checkout(&info, &[], &setup, &locals, "previous", None)
            .map_err(|error| format!("{error:?}"))?;
        assert_eq!(planned.local, expected.local);
        assert_eq!(planned.steps, expected.steps);
        assert_eq!(planned.undo, expected.undo);
        for collision in ["pr-1234", "pr-1234/child"] {
            let mut occupied = locals.clone();
            occupied.push(collision.into());
            let error = checkout(&info, &[], &setup, &occupied, "previous", None)
                .err()
                .ok_or("missing test value")?;
            assert!(error.contains("pr-1234"), "{error}");
            assert!(!error.contains("1.234") && !error.contains("1,234"));
        }
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
