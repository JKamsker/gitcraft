use super::*;
#[test]
fn r2_pure_captured_config_preserves_credentials_and_cannot_include_external_routing()
-> Result<(), String> {
    let entries = parse_config(
        "credential.https://example.test.helper\nstore --file=/private/key\0http.proxy\nproxy\0core.bare\nfalse\0include.path\n/changed/profile\0url.foreign.insteadOf\nhttps://example.test/\0url.foreign.pushInsteadOf\nhttps://example.test/\0remote.team.url\nold-destination\0",
    )?;
    let entries = freeze_routing(
        entries,
        vec![
            (
                "remote.team.url".into(),
                "https://example.test/captured-fetch".into(),
            ),
            (
                "remote.team.pushurl".into(),
                "https://example.test/captured-push".into(),
            ),
        ],
    );
    let rendered = render_config(entries, Some("/owned/work"), "/owned/hooks", "sha256")?;
    assert!(rendered.contains("[credential \"https://example.test\"]"));
    assert!(rendered.contains("store --file=/private/key"));
    for forbidden in [
        "insteadOf",
        "pushInsteadOf",
        "/changed/profile",
        "old-destination",
    ] {
        assert!(!rendered.contains(forbidden));
    }
    assert!(rendered.contains("https://example.test/captured-push"));
    assert!(rendered.contains("objectFormat = \"sha256\""));
    assert!(rendered.contains("hooksPath = \"/owned/hooks\""));
    assert!(render_config(vec![], None, "/hooks", "unknown").is_err());
    Ok(())
}
#[test]
fn r2_pure_command_source_env_separates_configuration_refs_and_shared_objects() {
    let env = source_env(
        Path::new("/owned/private"),
        "/original/objects",
        "/original/index",
        Some("/original/work"),
    );
    assert!(env.contains(&("GIT_DIR", "/owned/private".into())));
    assert!(env.contains(&("GIT_OBJECT_DIRECTORY", "/original/objects".into())));
    assert!(env.contains(&("GIT_CONFIG_NOSYSTEM", "1".into())));
    assert!(!safe_reference("refs/../../other"));
    assert!(!object_id("main"));
}
