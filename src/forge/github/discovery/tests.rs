use super::*;
use serde_json::json;
fn raw(status: u16, body: serde_json::Value) -> Response {
    Response {
        status,
        headers: vec![],
        body: body.to_string().into_bytes(),
    }
}
#[test]
fn discovery_categories_and_encoding() -> Result<(), DiscoveryError> {
    assert!(
        endpoints::endpoint(&Query::Mine)?
            .contains("affiliation=owner,collaborator,organization_member")
    );
    assert!(endpoints::endpoint(&Query::Starred)?.starts_with("/user/starred?"));
    assert!(
        endpoints::endpoint(&Query::OwnerRepositories {
            login: "team".into(),
            organization: true
        })?
        .starts_with("/orgs/team/repos?type=public")
    );
    assert!(
        endpoints::endpoint(&Query::SearchRepositories("a&b #?".into()))?
            .contains("q=a%26b+%23%3F")
    );
    assert!(
        endpoints::endpoint(&Query::OwnerRepositories {
            login: "../user".into(),
            organization: false
        })
        .is_err()
    );
    Ok(())
}
#[test]
fn discovery_next_is_bounded_to_query_and_authority() {
    let current = "/user/repos?per_page=50&page=1";
    assert!(
        endpoints::validate_next(
            current,
            "https://api.github.com/user/repos?per_page=50&page=2"
        )
        .is_some()
    );
    for bad in [
        "https://evil.invalid/user/repos?per_page=50&page=2",
        "/user/starred?per_page=50&page=2",
        "/user/repos?per_page=50&page=1",
        "/user/repos?per_page=100&page=2",
    ] {
        assert!(endpoints::validate_next(current, bad).is_none());
    }
}
#[test]
fn discovery_visibility_and_errors() -> Result<(), DiscoveryError> {
    let private =
        json!([{"id":1,"name":"repo","owner":{"id":2,"login":"me","type":"User"},"private":true}]);
    assert!(
        response::page(
            raw(200, json!({"items": private.clone()})),
            &Query::SearchRepositories("a".into()),
            "/search/repositories"
        )
        .is_err()
    );
    let page = response::page(raw(200, private), &Query::Mine, "/user/repos")?;
    assert_eq!(page.repositories[0].visibility, Visibility::Private);
    for (status, expected) in [
        (401, DiscoveryError::AuthRefused),
        (403, DiscoveryError::Forbidden),
        (404, DiscoveryError::NotFound),
        (429, DiscoveryError::RateLimited { retry_at: None }),
    ] {
        assert_eq!(
            response::page(raw(status, json!({})), &Query::Mine, "/user/repos").err(),
            Some(expected)
        );
    }
    let mut limited = raw(403, json!({}));
    limited
        .headers
        .push(("X-RateLimit-Remaining".into(), "0".into()));
    assert!(matches!(
        response::page(limited, &Query::Mine, "/user/repos"),
        Err(DiscoveryError::RateLimited { .. })
    ));
    assert!(response::page(raw(200, json!({})), &Query::Mine, "/user/repos").is_err());
    Ok(())
}

#[test]
fn discovery_search_metadata_and_size_limits() -> Result<(), DiscoveryError> {
    let query = Query::SearchOwners("git".into());
    let page = response::page(
        raw(
            200,
            json!({"items":[{"id":1,"login":"git","type":"Organization"}],"total_count":1200,"incomplete_results":true}),
        ),
        &query,
        "/search/users",
    )?;
    assert_eq!(page.owners.len(), 1);
    assert!(page.owners[0].organization);
    assert_eq!(page.total, Some(1200));
    assert!(page.incomplete);
    let oversized = Response {
        status: 200,
        headers: vec![],
        body: vec![b' '; BODY_LIMIT + 1],
    };
    assert_eq!(
        response::page(oversized, &Query::Mine, "/user/repos").err(),
        Some(DiscoveryError::Malformed)
    );
    Ok(())
}
