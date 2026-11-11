//! Verification.

use super::*;

#[test]
pub(super) fn old_git_verification_is_fail_closed_for_promisor_or_unreadable_configuration()
-> Result<(), String> {
    let unsupported = || Err("unknown option: --no-lazy-fetch\nusage: git ...".to_owned());
    assert_eq!(
        no_lazy_compatibility(Ok(vec![]), || Err(
            "must not read config for supported Git".into()
        ))?,
        vec!["--no-lazy-fetch"]
    );
    assert!(no_lazy_compatibility(unsupported(), || Ok(String::new()))?.is_empty());
    for config in [
        "remote.origin.promisor true",
        "remote.origin.promisor false",
        "extensions.partialclone origin",
    ] {
        assert!(
            no_lazy_compatibility(unsupported(), || Ok(config.into()))
                .is_err_and(|e| e.contains("will not be fetched automatically"))
        );
    }
    assert!(no_lazy_compatibility(unsupported(), || Err("cannot read config".into())).is_err());
    assert!(no_lazy_compatibility(Err("SSH failed".into()), || Ok(String::new())).is_err());
    Ok(())
}

#[test]
pub(super) fn verification_streams_only_the_validated_header_and_keeps_the_old_git_fallback()
-> Result<(), String> {
    let oid = "a".repeat(40);
    let header = format!("# v2 git bundle\n{oid} refs/heads/main\n\n");
    let mut bytes = header.as_bytes().to_vec();
    bytes.extend_from_slice(b"PACK");
    bytes.extend(vec![0; 1024 * 1024]);
    let long_path = format!("C:/{}exchange.bundle", "deep ü/".repeat(80));
    let (args, input) = verification_command(true, &["--no-lazy-fetch"], &long_path, &bytes)?;
    assert_eq!(
        args,
        [
            "--no-lazy-fetch",
            "-c",
            "core.longpaths=true",
            "bundle",
            "verify",
            "-"
        ]
    );
    assert_eq!(input, header.as_bytes());
    let (args, input) = verification_command(true, &[], &long_path, &bytes)?;
    assert_eq!(args.last().copied(), Some(long_path.as_str()));
    assert!(input.is_empty());
    let (args, input) = verification_command(false, &["--no-lazy-fetch"], &long_path, &bytes)?;
    assert_eq!(args.last().copied(), Some(long_path.as_str()));
    assert!(input.is_empty());
    assert!(verification_command(true, &["--no-lazy-fetch"], &long_path, b"not a bundle").is_err());
    Ok(())
}
