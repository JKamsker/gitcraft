//! Connection validation, project keys, profile export and localized failures.
use super::super::Failure;
use super::*;

fn server(origin: &str) -> Account {
    Account {
        origin: origin.into(),
        secret_ref: "profile-3".into(),
        ..Account::default()
    }
}

fn cloud(origin: &str, organization: &str) -> Account {
    Account {
        edition: Edition::Cloud,
        origin: origin.into(),
        organization: organization.into(),
        secret_ref: "profile-3".into(),
    }
}

#[test]
fn connections_need_https_or_this_computer_and_cloud_needs_an_organization() {
    assert_eq!(Account::default().validate(), Ok(()));
    for good in [
        "https://sonar.example.com",
        "https://sonar.example.com/",
        "https://example.com/sonarqube",
        "https://sonar.example.com:8443/",
        "http://localhost:9000",
        "http://127.0.0.1:9000/",
        "http://[::1]:9000",
    ] {
        assert_eq!(server(good).validate(), Ok(()), "{good}");
    }
    for bad in [
        "http://sonar.example.com",
        "http://10.0.0.5:9000",
        "https://user:pw@sonar.example.com",
        "https://sonar.example.com/?x=1",
        "https://sonar.example.com/#top",
        "ftp://sonar.example.com",
        "sonar.example.com",
        "https://",
    ] {
        assert_eq!(
            server(bad).validate(),
            Err("analysis-connection-origin"),
            "{bad}"
        );
    }
    assert_eq!(
        server("https://sonar.example.com\n").validate(),
        Err("analysis-connection-control-characters")
    );
    let mut with_org = server("https://sonar.example.com");
    with_org.organization = "acme".into();
    assert_eq!(
        with_org.validate(),
        Err("analysis-connection-server-organization")
    );
    assert_eq!(
        cloud("https://sonarcloud.io", "my-org_1").validate(),
        Ok(())
    );
    assert_eq!(cloud("https://sonarqube.us/", "acme").validate(), Ok(()));
    assert_eq!(
        cloud("https://sonar.example.com", "acme").validate(),
        Err("analysis-connection-cloud-origin")
    );
    for org in ["", "my org", "../x", "a&b"] {
        assert_eq!(
            cloud("https://sonarcloud.io", org).validate(),
            Err("analysis-connection-organization"),
            "{org}"
        );
    }
}

#[test]
fn identity_ignores_trailing_slashes_and_the_secret_reference() {
    let a = server("https://sonar.example.com/");
    let mut b = server("https://sonar.example.com");
    b.secret_ref = "profile-9".into();
    assert_eq!(a.identity(), b.identity());
    assert_ne!(
        a.identity(),
        cloud("https://sonarcloud.io", "acme").identity()
    );
    assert_ne!(
        cloud("https://sonarcloud.io", "acme").identity(),
        cloud("https://sonarcloud.io", "other").identity()
    );
}

#[test]
fn project_keys_follow_the_servers_rules() {
    for good in ["my:project", "com.acme:app-1", "a_b.c", "x1"] {
        assert!(project_key(good), "{good}");
    }
    for bad in ["", "123", "a b", "a&b=c", "a/b", "ä", &"k".repeat(401)] {
        assert!(!project_key(bad), "{bad}");
    }
}

#[test]
fn exported_profiles_carry_a_reference_but_never_the_token() -> Result<(), String> {
    let account = cloud("https://sonarcloud.io", "acme");
    // Refused before any credential store is touched: no token, so nothing is saved.
    assert_eq!(account.store("   "), Err("analysis-token-required"));
    assert_eq!(account.store("a b"), Err("analysis-token-required"));
    let unreferenced = Account {
        secret_ref: String::new(),
        ..account.clone()
    };
    assert_eq!(
        unreferenced.store("squ_abc"),
        Err("analysis-token-required")
    );
    let choices = crate::connections::Choices {
        sonar: account,
        ..Default::default()
    };
    let exported = serde_json::to_string(&choices).map_err(|e| e.to_string())?;
    assert!(exported.contains(r#""secret_ref":"profile-3""#));
    assert!(!exported.contains("squ_"));
    assert!(!exported.to_ascii_lowercase().contains("token"));
    let back: crate::connections::Choices =
        serde_json::from_str(&exported).map_err(|e| e.to_string())?;
    assert_eq!(back, choices);
    // Profiles saved before this connection existed still load.
    let old: crate::connections::Choices =
        serde_json::from_str(r#"{"ssh_key":""}"#).map_err(|e| e.to_string())?;
    assert!(!old.sonar.configured());
    Ok(())
}

#[test]
fn every_failure_has_a_translated_sentence() {
    let all = [
        Failure::NotConfigured,
        Failure::InvalidConnection,
        Failure::MissingToken,
        Failure::Offline,
        Failure::Cancelled,
        Failure::Unauthorized,
        Failure::Forbidden,
        Failure::ProjectNotFound,
        Failure::WrongOrganization,
        Failure::BindingMismatch,
        Failure::RateLimited(None),
        Failure::RateLimited(Some(30)),
        Failure::UnsupportedVersion("7.6".into()),
        Failure::PullRequestsUnsupported,
        Failure::Http(502),
        Failure::Redirected,
        Failure::Timeout,
        Failure::Unreachable,
        Failure::Malformed,
        Failure::Busy,
    ];
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for failure in &all {
            let text = failure.text();
            assert!(!text.starts_with("analysis-"), "{language}: {failure:?}");
        }
    }
    crate::i18n::set_language("de");
    assert!(Failure::Http(502).text().contains("502"));
    assert!(Failure::RateLimited(Some(30)).text().contains("30"));
    crate::i18n::set_language("en-US");
}
