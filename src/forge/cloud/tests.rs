use super::*;
#[test]
fn selected_authority_and_secure_reference_are_metadata_only() -> Result<(), String> {
    let a = Account {
        server: "https://dev.azure.com/team".into(),
        user: "viewer-id".into(),
        secret_ref: "profile-2".into(),
        pat: true,
        ..Default::default()
    };
    assert!(a.validate(Service::Azure).is_ok());
    assert!(
        Account {
            server: "https://dev.azure.com/team/foreign".into(),
            ..a.clone()
        }
        .validate(Service::Azure)
        .is_err()
    );
    assert!(
        Service::Bitbucket
            .server("https://bitbucket.org.evil")
            .is_err()
    );
    assert_eq!(
        Service::Azure.server("https://dev.azure.com/TEAM/")?,
        "https://dev.azure.com/team"
    );
    let mut other = a.clone();
    other.user = "another-viewer".into();
    assert_ne!(a.key(Service::Azure), other.key(Service::Azure));
    assert!(
        serde_json::to_value(&a)
            .map_err(|e| e.to_string())?
            .get("token")
            .is_none()
    );
    Ok(())
}
#[test]
fn azure_command_and_expiry_are_recorded_without_process_execution() -> Result<(), String> {
    let a = Account {
        config_dir: "/recorded/azure-profile".into(),
        ..Default::default()
    };
    let mut c = std::process::Command::new("/recording/az");
    azure_command(&mut c, &a);
    assert_eq!(c.get_program(), "/recording/az");
    assert_eq!(
        c.get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
        [
            "account",
            "get-access-token",
            "--resource",
            "499b84ac-1321-427f-aa17-267ca6975798",
            "--output",
            "json"
        ]
    );
    assert!(c.get_envs().any(|(k, v)| k == "AZURE_CONFIG_DIR"
        && v == Some(std::ffi::OsStr::new("/recorded/azure-profile"))));
    assert_eq!(
        parse_azure_token(
            &json!({"accessToken":"recorded-only","expires_on":200}),
            100
        )?
        .1,
        Some(200)
    );
    assert!(
        parse_azure_token(
            &json!({"accessToken":"recorded-only","expires_on":100}),
            100
        )
        .is_err()
    );
    assert!(parse_azure_token(&json!({"accessToken":"recorded-only"}), 100).is_err());
    Ok(())
}
#[test]
fn native_viewer_context_and_endpoints_are_bound() -> Result<(), String> {
    let h = Hosting {
        forge: super::super::Forge::GitHub,
        host: "dev.azure.com".into(),
        web: "https://dev.azure.com".into(),
        path: "team/project/_git/repo".into(),
    };
    let api = Api::recording(
        Service::Azure,
        h,
        "viewer-id",
        std::sync::Arc::new(|method, path, _| {
            assert_eq!(method, "GET");
            assert!(path.starts_with("_apis/connectionData"));
            Ok(json!({"authenticatedUser":{"id":"viewer-id"}}))
        }),
    );
    assert!(api.verify_viewer().is_ok());
    assert!(api.get("https://foreign.invalid").is_err());
    assert!(api.get("../foreign").is_err());
    let mut changed = connections::current();
    changed.error = Some("changed".into());
    changed.run(|| assert!(api.guard_context().is_err()));
    Ok(())
}
