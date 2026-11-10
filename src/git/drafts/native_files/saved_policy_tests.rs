use super::*;
#[test]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn pure_native_full_policy_roundtrip_keeps_owner_group_native_attributes_and_acl()
-> Result<(), String> {
    let access = serde_json::json!({"entries": [[[117,115,101,114,46,97],[1,2,3]]]});
    #[cfg(target_os = "macos")]
    let access = {
        let mut access = access;
        access["acl"] = serde_json::json!([{"kind":"user","name":"501","perms":["read"],"flags":[],"allow":false}]);
        access
    };
    let policy: Policy = serde_json::from_value(serde_json::json!({
        "platform":std::env::consts::OS,"mode":416,"uid":501,"gid":20,"access":access
    }))
    .map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec(&policy).map_err(|error| error.to_string())?;
    assert_eq!(
        serde_json::from_slice::<Policy>(&bytes).map_err(|error| error.to_string())?,
        policy
    );
    let mut tightened = policy.clone();
    tightened.mode = 0o600;
    assert_ne!(tightened, policy);
    tightened = policy.clone();
    tightened.gid += 1;
    assert_ne!(tightened, policy);
    let mut changed: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    changed["access"]["entries"][0][1] = serde_json::json!([9]);
    assert_ne!(
        serde_json::from_value::<Policy>(changed).map_err(|error| error.to_string())?,
        policy
    );
    assert!(serde_json::from_str::<Policy>("416").is_err());
    #[cfg(target_os = "macos")]
    {
        let mut changed: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        changed["access"]["acl"][0]["allow"] = serde_json::json!(true);
        assert_ne!(
            serde_json::from_value::<Policy>(changed).map_err(|error| error.to_string())?,
            policy
        );
    }
    assert!(policy.validate().is_ok());
    tightened.platform = "foreign platform".into();
    assert!(tightened.validate().is_err());
    let saved: Policies = [("f".into(), policy.clone())].into();
    assert_eq!(
        serde_json::from_slice::<Policies>(
            &serde_json::to_vec(&saved).map_err(|error| error.to_string())?
        )
        .map_err(|error| error.to_string())?["f"],
        policy
    );
    Ok(())
}
#[test]
fn pure_native_unknown_receipt_cannot_be_confirmed() -> Result<(), String> {
    assert!(CommitOutcome::Committed.confirmed().is_ok());
    assert!(
        CommitOutcome::Unknown("operation-1: receipt unavailable".into())
            .confirmed()
            .err()
            .ok_or("expected an error")?
            .contains("operation-1")
    );
    Ok(())
}
