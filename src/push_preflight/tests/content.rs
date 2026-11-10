#[test]
fn six_categories_cover_actual_outgoing_history_and_never_return_secret_values()
-> Result<(), String> {
    let f = Fixture::new()?;
    f.advance_receiver()?;
    f.git(&["fetch", "-q", "origin"])?;
    let secret = "ghp_THIS_IS_A_DUMMY_FIXTURE_TOKEN_123456789";
    f.commit(
        "token.txt",
        &format!("api_key = '{secret}'\n"),
        "WIP: unfinished",
    )?;
    std::fs::write(f.repo.join("large.bin"), vec![b'x'; 6 * 1024 * 1024])
        .map_err(|e| e.to_string())?;
    f.git(&["add", "large.bin"])?;
    f.git(&["commit", "-qm", "Large asset"])?;
    let mut profile = f.profile();
    profile.author_email = "personal@example.org".into();
    profile.own_signing = true;
    profile.signing.sign_commits = true;
    let before = f.git(&["status", "--porcelain=v1"])?;
    let refs = f.git(&["show-ref"])?;
    let head = f.receiver_head()?;
    let plan = f.plan(profile)?;
    for kind in [
        Kind::Identity,
        Kind::Signature,
        Kind::Message,
        Kind::Secret,
        Kind::Large,
        Kind::RemoteHistory,
    ] {
        assert!(
            plan.findings.iter().any(|i| i.kind == kind),
            "missing {kind:?}"
        );
    }
    assert!(
        plan.findings
            .iter()
            .any(|i| i.kind == Kind::RemoteHistory && i.detail.contains("1 by other authors"))
    );
    assert!(plan.findings.iter().all(|i| !i.detail.contains(secret)));
    assert!(push_args(&plan).is_err());
    let mut force = plan.clone();
    force.force = true;
    assert!(
        push_args(&force)?
            .iter()
            .any(|a| a.starts_with("--force-with-lease="))
    );
    assert_eq!(f.git(&["status", "--porcelain=v1"])?, before);
    assert_eq!(f.git(&["show-ref"])?, refs);
    assert_eq!(f.receiver_head()?, head);
    Ok(())
}

#[test]
fn secret_scan_streams_large_and_binary_objects_without_match_output() -> Result<(), String> {
    let f = Fixture::new()?;
    let token = "github_pat_DUMMY_FIXTURE_VALUE_1234567890";
    let mut bytes = vec![b' '; 3 * 1024 * 1024];
    bytes.extend_from_slice(format!("\naccess_token={token}\n").as_bytes());
    bytes.push(0);
    std::fs::write(f.repo.join("binary.dat"), bytes).map_err(|e| e.to_string())?;
    f.git(&["add", "binary.dat"])?;
    f.git(&["commit", "-qm", "Payload"])?;
    let plan = f.plan(f.profile())?;
    assert!(
        plan.findings.iter().any(|x| x.kind == Kind::Secret
            && x.path == "binary.dat"
            && x.detail.contains("line 2"))
    );
    assert!(!format!("{:?}", plan.findings).contains(token));
    Ok(())
}

#[cfg(unix)]
#[test]
fn missing_promisor_blob_does_not_trigger_a_lazy_fetch_or_credential_helper() -> Result<(), String>
{
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new()?;
    f.commit("missing.txt", "payload", "Missing")?;
    let blob = f.git(&["rev-parse", "HEAD:missing.txt"])?;
    let marker = f.root.join("upload-pack-ran");
    let program = f.root.join("upload-pack");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\ntouch {}\nexit 1\n",
            crate::remote::sh_quote(&marker.to_string_lossy())
        ),
    )
    .map_err(|e| e.to_string())?;
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| e.to_string())?;
    f.git(&["config", "extensions.partialClone", "origin"])?;
    f.git(&["config", "remote.origin.promisor", "true"])?;
    f.git(&[
        "config",
        "remote.origin.uploadpack",
        &program.to_string_lossy(),
    ])?;
    std::fs::remove_file(
        f.repo
            .join(".git/objects")
            .join(&blob[..2])
            .join(&blob[2..]),
    )
    .map_err(|e| e.to_string())?;
    assert!(f.plan(f.profile()).is_err());
    assert!(!marker.exists());
    Ok(())
}
