//! SOURCE ONLY under the execution hold. Owned local shells; no network pool/fallback.
use super::*;
use crate::remote::{Request, session::framed, shell::Shell};
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

fn fixture() -> Result<(PathBuf, Shell), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "gitcraft-owned-transport-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())?;
    let mut command = Command::new("/bin/sh");
    command
        .env_clear()
        .current_dir(&root)
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", &root)
        .env("XDG_CONFIG_HOME", &root)
        .env("TMPDIR", &root);
    let shell = Shell::start(command, "owned local transport fixture")?;
    Ok((root, shell))
}

#[test]
fn owned_fixture_real_frame_preserves_bytes_and_captured_context() -> Result<(), String> {
    let (root, mut shell) = fixture()?;
    let context = crate::connections::Context {
        choices: crate::connections::Choices {
            ssh_key: "/recording-only/captured-key".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let owner = context.run(OwnedDispatch::default);
    let bytes = b"owned\0binary\ninput\xff".to_vec();
    let request = Request {
        script: "cat".into(),
        stdin: Some(bytes.clone()),
        ..Default::default()
    };
    owner.run(|| -> Result<(), String> {
        assert_eq!(crate::connections::current(), context);
        assert!(owner.pending());
        assert!(owner.begin());
        let output = shell
            .run(
                &framed(&request, "owned-success"),
                "owned-success",
                Instant::now() + Duration::from_secs(2),
            )
            .map_err(|_| "Owned local frame did not complete")?;
        assert_eq!(output.code, 0);
        assert_eq!(output.stdout, bytes);
        assert!(output.stderr.is_empty());
        owner.completed();
        Ok(())
    })?;
    assert_eq!(owner.phase(), Phase::Completed);
    assert!(!owner.pending());
    drop(shell); // Existing Shell owns and reaps this exact local child handle.
    println!("Owned frame completed; retained fixture {}", root.display());
    Ok(())
}

#[test]
fn owned_fixture_timeout_keeps_unknown_and_late_owned_receipt() -> Result<(), String> {
    let (root, mut shell) = fixture()?;
    let receipt = root.join("late-receipt");
    let request = Request {
        script: format!(
            "sleep 0.2; printf done >{}",
            crate::remote::sh_quote(&receipt.to_string_lossy())
        ),
        ..Default::default()
    };
    let owner = crate::connections::Context::default().run(OwnedDispatch::default);
    owner.run(|| {
        assert!(owner.begin());
        let failure = shell.run(
            &framed(&request, "owned-late"),
            "owned-late",
            Instant::now() + Duration::from_millis(30),
        );
        match failure {
            Err(failure) => assert_eq!(
                failure_action(&failure, true, true),
                FailureAction::OwnedUnknown
            ),
            Ok(_) => panic!("Owned timeout fixture unexpectedly completed before its deadline"),
        }
        owner.unknown();
    });
    assert_eq!(owner.phase(), Phase::Unknown);
    assert!(!receipt.exists());
    // No stop or process selector: keep the owned connection alive until this finite,
    // benign local job writes its final receipt. Its source performs no later action.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !receipt.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert_eq!(fs::read(&receipt).map_err(|e| e.to_string())?, b"done");
    assert_eq!(
        owner.phase(),
        Phase::Unknown,
        "A late file does not itself certify a feature outcome"
    );
    assert!(root.is_dir(), "Unknown evidence must not be cleaned up");
    drop(shell);
    println!(
        "Unknown retained through late receipt; fixture {}",
        root.display()
    );
    Ok(())
}
