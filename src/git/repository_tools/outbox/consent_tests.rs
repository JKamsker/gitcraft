use super::*;
#[test]
fn merge_pure_outbox_consent_retains_all_added_provider_choices() -> Result<(), String> {
    let profile = UserProfile::default();
    let context = crate::connections::Context {
        choices: profile.connections.clone(),
        ..Default::default()
    };
    let saved = Consent::capture(&profile, &context, None).map_err(|error| error.to_string())?;
    let encoded = serde_json::to_value(&saved).map_err(|error| error.to_string())?;
    for key in ["azure", "bitbucket", "jira", "linear", "github", "gitlab"] {
        assert!(encoded["choices"].get(key).is_some(), "missing {key}");
        let mut changed = profile.clone();
        match key {
            "azure" => changed.connections.azure.user = "other-account".into(),
            "bitbucket" => changed.connections.bitbucket.user = "other-account".into(),
            "jira" => changed.connections.jira.user = "other-account".into(),
            "linear" => changed.connections.linear.user = "other-account".into(),
            "github" => changed.connections.github.user = "other-account".into(),
            "gitlab" => changed.connections.gitlab.user = "other-account".into(),
            _ => unreachable!(),
        }
        let selected = crate::connections::Context {
            choices: changed.connections.clone(),
            ..Default::default()
        };
        let fresh =
            Consent::capture(&changed, &selected, None).map_err(|error| error.to_string())?;
        assert!(
            Consent::require(Some(&saved), &fresh).is_err(),
            "changed {key} remained authorized"
        );
    }
    assert_eq!(
        serde_json::from_value::<Consent>(encoded).map_err(|error| error.to_string())?,
        saved
    );
    Ok(())
}

#[test]
fn pure_outbox_consent_distinguishes_profiles_selected_accounts_scopes_and_legacy_without_secrets()
-> Result<(), String> {
    let mut a = UserProfile::default();
    a.connections.github.user = "alice".into();
    let ca = crate::connections::Context {
        choices: a.connections.clone(),
        ..Default::default()
    };
    let credentials = git::Credentials {
        username: "alice".into(),
        password: "test-secret-must-never-be-persisted".into(),
        scope: Some(
            git::CredentialScope::for_url("https://github.com/team/repo.git", "alice")
                .map_err(|error| error.to_string())?,
        ),
    };
    let saved = Consent::capture(&a, &ca, Some(&credentials)).map_err(|error| error.to_string())?;
    let encoded = serde_json::to_string(&saved).map_err(|error| error.to_string())?;
    assert!(!encoded.contains(&credentials.password));
    assert!(!encoded.contains("password"));
    assert_eq!(
        serde_json::from_str::<Consent>(&encoded).map_err(|error| error.to_string())?,
        saved
    );
    assert!(Consent::require(None, &saved).is_err());
    let mut b = a.clone();
    b.id += 1;
    let other_profile =
        Consent::capture(&b, &ca, Some(&credentials)).map_err(|error| error.to_string())?;
    assert!(Consent::require(Some(&saved), &other_profile).is_err());
    b = a.clone();
    b.connections.github.user = "bob".into();
    let cb = crate::connections::Context {
        choices: b.connections.clone(),
        ..Default::default()
    };
    assert!(
        Consent::require(
            Some(&saved),
            &Consent::capture(&b, &cb, Some(&credentials)).map_err(|error| error.to_string())?
        )
        .is_err()
    );
    assert!(Consent::capture(&b, &ca, Some(&credentials)).is_err());
    let mut changed = credentials.clone();
    changed.scope = Some(
        git::CredentialScope::for_url("https://github.com/other/repo.git", "alice")
            .map_err(|error| error.to_string())?,
    );
    assert!(
        Consent::require(
            Some(&saved),
            &Consent::capture(&a, &ca, Some(&changed)).map_err(|error| error.to_string())?
        )
        .is_err()
    );
    changed = credentials.clone();
    changed.username = "other".into();
    assert!(
        Consent::require(
            Some(&saved),
            &Consent::capture(&a, &ca, Some(&changed)).map_err(|error| error.to_string())?
        )
        .is_err()
    );
    changed = credentials.clone();
    changed.password = "rotated-secret-not-an-identity".into();
    assert!(
        Consent::require(
            Some(&saved),
            &Consent::capture(&a, &ca, Some(&changed)).map_err(|error| error.to_string())?
        )
        .is_ok()
    );
    changed.scope = None;
    assert!(Consent::capture(&a, &ca, Some(&changed)).is_err());
    Ok(())
}
