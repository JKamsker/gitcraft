use super::*;
#[test]
fn cloud_account_scopes_and_public_metadata_reject_credential_sinks_and_incomplete_selections() {
    assert!(Service::Azure.server("https://dev.azure.com/work").is_ok());
    for server in [
        "http://dev.azure.com/work",
        "https://dev.azure.com/work/other",
        "https://dev.azure.com@other.invalid/work",
        "https://other.invalid/work",
        "https://dev.azure.com/../other",
    ] {
        assert!(Service::Azure.server(server).is_err(), "{server}");
    }
    assert!(Service::Bitbucket.server("https://bitbucket.org").is_ok());
    assert!(
        Service::Bitbucket
            .server("https://api.bitbucket.org")
            .is_err()
    );
    let account = CloudAccount {
        server: "https://dev.azure.com/work".into(),
        user: "work@example.invalid".into(),
        ..Default::default()
    };
    assert!(account.validate(Service::Azure).is_ok());
    assert!(
        CloudAccount {
            user: "work\nother".into(),
            ..account.clone()
        }
        .validate(Service::Azure)
        .is_err()
    );
    assert!(
        CloudAccount {
            credential_user: "wrong".into(),
            ..account.clone()
        }
        .validate(Service::Azure)
        .is_err()
    );
    assert!(
        CloudAccount {
            user: String::new(),
            config_dir: "/local/auth".into(),
            ..account
        }
        .validate(Service::Azure)
        .is_err()
    );
}
