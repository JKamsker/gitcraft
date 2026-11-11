use super::*;
#[test]
fn r2_pure_source_plumbing_disables_hooks_transports_filters_and_has_a_deadline()
-> Result<(), String> {
    let extra = inert_object_options(&["cat-file", "blob", "oid"], None, 8192)?;
    assert!(extra.pre.contains(&"core.hooksPath=/dev/null"));
    assert!(extra.pre.contains(&"protocol.allow=never"));
    assert!(extra.env.contains(&("GIT_NO_LAZY_FETCH", "1".into())));
    assert!(extra.timeout.is_some());
    assert_eq!(extra.output_limit, Some(8192));
    for command in [
        "checkout",
        "archive",
        "clone",
        "submodule",
        "clean",
        "fetch",
        "config",
    ] {
        assert!(inert_object_options(&[command], None, 1).is_err());
    }
    Ok(())
}
