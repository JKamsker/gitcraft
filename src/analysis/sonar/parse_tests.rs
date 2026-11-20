//! Parsers against the endpoints' published response examples (SonarQube Server 24.12/26.x
//! `api/webservices/response_example` and SonarQube Cloud), trimmed to the fields that vary.
use super::super::gate::ServerGate;
use super::parse::*;

#[test]
fn versions_of_server_generations_and_cloud_parse() {
    assert_eq!(
        version("10.4.1.88267"),
        Some(Version {
            major: 10,
            minor: 4
        })
    );
    assert_eq!(
        version("2025.1.0.102418\n"),
        Some(Version {
            major: 2025,
            minor: 1
        })
    );
    assert_eq!(
        version("26.6.0.123539"),
        Some(Version {
            major: 26,
            minor: 6
        })
    );
    assert_eq!(version("7.6"), Some(Version { major: 7, minor: 6 }));
    assert_eq!(
        version("8.0.0.118008"),
        Some(Version { major: 8, minor: 0 })
    );
    assert_eq!(version("9"), Some(Version { major: 9, minor: 0 }));
    assert_eq!(version("<html>"), None);
    assert_eq!(version(""), None);
    assert!(Version { major: 7, minor: 6 } < Version { major: 7, minor: 7 });
}

#[test]
fn project_status_across_api_generations() -> Result<(), String> {
    // Server 10+: no periods; caycStatus added in 9.9.
    let current = r#"{"projectStatus":{"status":"ERROR","ignoredConditions":false,"caycStatus":"non-compliant","conditions":[{"status":"ERROR","metricKey":"new_coverage","comparator":"LT","errorThreshold":"85","actualValue":"82.5"},{"status":"OK","metricKey":"new_sqale_debt_ratio","comparator":"GT","errorThreshold":"5","actualValue":"0.65"},{"status":"ERROR","metricKey":"new_blocker_violations","comparator":"GT","errorThreshold":"0","actualValue":"14"}]}}"#;
    let (gate, failing) = project_status(current).map_err(|e| format!("{e:?}"))?;
    assert_eq!(gate, ServerGate::Error);
    assert_eq!(failing, ["new_coverage", "new_blocker_violations"]);
    // Cloud / Server before 10.0: periods and periodIndex present.
    let cloud = r#"{"projectStatus":{"status":"OK","ignoredConditions":false,"conditions":[{"status":"OK","metricKey":"new_coverage","comparator":"LT","periodIndex":1,"errorThreshold":"85","actualValue":"90"}],"periods":[{"index":1,"mode":"last_version","date":"2000-04-27T00:45:23+0200","parameter":"2015-12-07"}]}}"#;
    assert_eq!(project_status(cloud), Ok((ServerGate::Ok, Vec::new())));
    // Before 7.6 a WARN status existed.
    let warn = r#"{"projectStatus":{"status":"WARN","conditions":[{"status":"WARN","metricKey":"coverage"}]}}"#;
    assert_eq!(
        project_status(warn),
        Ok((ServerGate::Warn, vec!["coverage".to_owned()]))
    );
    let none = r#"{"projectStatus":{"status":"NONE","ignoredConditions":false}}"#;
    assert_eq!(project_status(none), Ok((ServerGate::None, Vec::new())));
    let odd = r#"{"projectStatus":{"status":"PURPLE"}}"#;
    assert_eq!(project_status(odd), Ok((ServerGate::Unknown, Vec::new())));
    let missing = r#"{"projectStatus":{}}"#;
    assert_eq!(
        project_status(missing),
        Ok((ServerGate::Unknown, Vec::new()))
    );
    assert!(project_status(r#"{"errors":[{"msg":"x"}]}"#).is_err());
    assert!(project_status("not json").is_err());
    Ok(())
}

#[test]
fn newest_analysis_with_and_without_revision() -> Result<(), String> {
    let server = r#"{"paging":{"pageIndex":1,"pageSize":1,"total":3},"analyses":[{"key":"A2","date":"2016-12-12T17:12:45+0100","projectVersion":"1.2.1","buildString":"1.2.1.423","revision":"be6c75b85da526349c44e3978374c95e0b80a96d","manualNewCodePeriodBaseline":false,"detectedCI":"Jenkins","events":[{"key":"E21","category":"QUALITY_PROFILE","name":"Quality Profile changed to Sonar Way","qualityProfile":{"key":"k","name":"n","languageKey":"java"}}]}]}"#;
    let analysis = latest_analysis(server)
        .map_err(|e| format!("{e:?}"))?
        .ok_or("missing analysis")?;
    assert_eq!(analysis.key.as_deref(), Some("A2"));
    assert_eq!(analysis.date.as_deref(), Some("2016-12-12T17:12:45+0100"));
    assert_eq!(
        analysis.revision.as_deref(),
        Some("be6c75b85da526349c44e3978374c95e0b80a96d")
    );
    // An analysis made without SCM data has no revision.
    let bare = r#"{"paging":{"pageIndex":1,"pageSize":1,"total":1},"analyses":[{"key":"P1","date":"2015-11-11T10:00:00+0100","events":[]}]}"#;
    let analysis = latest_analysis(bare)
        .map_err(|e| format!("{e:?}"))?
        .ok_or("missing analysis")?;
    assert_eq!(analysis.revision, None);
    let empty = r#"{"paging":{"pageIndex":1,"pageSize":1,"total":0},"analyses":[]}"#;
    assert_eq!(latest_analysis(empty), Ok(None));
    assert!(latest_analysis(r#"{"analyses":[{"date":"x"}]}"#).is_err());
    Ok(())
}

#[test]
fn branches_and_pull_requests_of_server_and_cloud() -> Result<(), String> {
    let server = r#"{"branches":[{"name":"feature/foo","isMain":false,"type":"BRANCH","status":{"qualityGateStatus":"OK"},"analysisDate":"2017-04-03T13:37:00+0100","excludedFromPurge":false,"branchId":"ac31"},{"name":"main","isMain":true,"type":"BRANCH","status":{"qualityGateStatus":"ERROR"},"analysisDate":"2017-04-01T01:15:42+0100","excludedFromPurge":true}]}"#;
    let rows = branches(server).map_err(|e| format!("{e:?}"))?;
    assert_eq!(rows.len(), 2);
    assert!(!rows[0].is_main && rows[1].is_main);
    assert_eq!(rows[1].name, "main");
    // Cloud's older branch types (LONG/SHORT) and commit fields are ignored.
    let cloud = r#"{"branches":[{"name":"master","isMain":true,"type":"LONG","status":{"qualityGateStatus":"ERROR","bugs":0},"analysisDate":"2017-04-01T01:15:42+0100","commit":{"sha":"P1A5"}}]}"#;
    assert_eq!(
        branches(cloud).map_err(|e| format!("{e:?}"))?[0].name,
        "master"
    );
    let prs = r#"{"pullRequests":[{"key":"123","title":"Add feature X","branch":"feature/bar","base":"feature/foo","status":{"qualityGateStatus":"OK","bugs":0,"vulnerabilities":0,"codeSmells":0},"analysisDate":"2017-04-01T02:15:42+0200","url":"https://github.com/o/r/pull/32","target":"feature/foo","commit":{"sha":"be6c75b85da526349c44e3978374c95e0b80a96d"},"contributors":[]},{"key":"234","title":"Never analyzed","branch":"feature/baz","base":"main","status":{}}]}"#;
    let rows = pull_requests(prs).map_err(|e| format!("{e:?}"))?;
    assert_eq!(rows[0].key, "123");
    assert_eq!(
        rows[0].sha.as_deref(),
        Some("be6c75b85da526349c44e3978374c95e0b80a96d")
    );
    assert_eq!(rows[1].sha, None);
    assert_eq!(rows[1].date, None);
    assert!(branches(r#"{"branches":[{"isMain":true}]}"#).is_err());
    assert!(pull_requests(r#"{}"#).is_err());
    Ok(())
}

#[test]
fn compute_engine_tasks_and_other_small_answers() -> Result<(), String> {
    let ce = r#"{"queue":[{"organization":"my-org-1","id":"AU_w84","type":"REPORT","componentKey":"project","status":"PENDING","submittedAt":"2015-09-21T19:28:54+0200","branch":"feature/x","branchType":"BRANCH"},{"id":"AU_w85","type":"REPORT","status":"IN_PROGRESS","pullRequest":"123"}],"current":{"id":"AU_w8L","type":"REPORT","analysisId":"123456","status":"FAILED","submittedAt":"2015-09-21T19:25:49+0200","errorMessage":"the error message"}}"#;
    let rows = tasks(ce).map_err(|e| format!("{e:?}"))?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].branch.as_deref(), Some("feature/x"));
    assert_eq!(rows[1].pull_request.as_deref(), Some("123"));
    assert!(rows[2].current && rows[2].status == "FAILED" && rows[2].branch.is_none());
    assert_eq!(tasks(r#"{"queue":[]}"#), Ok(Vec::new()));
    assert_eq!(valid(r#"{"valid": true}"#), Ok(true));
    assert_eq!(valid(r#"{"valid": false}"#), Ok(false));
    assert!(valid("{}").is_err());
    let show = r#"{"component":{"organization":"my-org-1","key":"com.sonarsource:java-markdown","name":"Java Markdown","qualifier":"TRK","visibility":"private"},"ancestors":[]}"#;
    assert_eq!(organization(show), Ok(Some("my-org-1".to_owned())));
    assert_eq!(
        organization(r#"{"component":{"key":"k"},"ancestors":[]}"#),
        Ok(None)
    );
    Ok(())
}
