use super::*;
#[test]
fn recording_cancelled_pending_result_proves_no_dispatch_but_unknown_never_does()
-> Result<(), Box<dyn std::error::Error>> {
    for result in [
        Err("cancelled".into()),
        Err("process timed out and was stopped".into()),
    ] {
        let (code, complete, out, err) = terminal_parts(result)?;
        assert_eq!(code, 125);
        assert!(!complete);
        assert!(out.is_empty() && err.is_empty());
    }
    for message in [
        "could not finish owned process",
        "Process service stopped before returning a result",
    ] {
        assert!(terminal_parts(Err(message.into())).is_err());
    }
    assert!(
        terminal_parts(Ok(crate::processes::recording_output(Some(
            crate::processes::StopKind::Uncertain
        ))))
        .is_err()
    );
    Ok(())
}
#[test]
fn recording_native_exit_codes_preserve_windows_status_range() {
    for code in [0, 7, 255] {
        assert!(native_exit_code(code, false) && native_exit_code(code, true));
    }
    for code in [
        256,
        65536,
        -1073741819,
        i64::from(i32::MIN),
        i64::from(i32::MAX),
    ] {
        assert!(native_exit_code(code, true));
        assert!(!native_exit_code(code, false));
    }
    assert!(!native_exit_code(i64::MAX, true));
}
#[test]
fn helper_entrypoint() {
    #[cfg(unix)]
    match super::super::owned_runtime::dispatch() {
        Ok(false) => {}
        Ok(true) => unreachable!(),
        Err(error) => {
            eprintln!("finite fixture child: {error}");
            std::process::exit(125);
        }
    }
    if std::env::var_os("GITCRAFT_NATIVE_HELPER").is_some() {
        std::process::exit(entrypoint());
    }
}

#[cfg(unix)]
#[test]
#[ignore = "runtime hold: exact source-reviewed invocation only"]
fn owned_runtime_endpoint_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    use super::super::owned_runtime as fixture;
    let mut scratch = fixture::Scratch::new()?;
    let service = crate::processes::owned_fixture_service()?;
    let canary = fixture::canary(&service, &scratch.0)?;
    fixture::challenge(&canary, &scratch.0, b"before-native")?;
    for (mode, cancelled) in [("exit", false), ("leader", true)] {
        let directory = scratch.0.join(mode);
        std::fs::create_dir(&directory)?;
        let record = directory.join("target.owner");
        let native = command(fixture::command(mode, &directory)?, &record, fixture::LIMIT)?;
        let identity = native.endpoint.clone();
        let mut options = fixture::options();
        let transport = native.configure(&mut options);
        let ticket = service.submit(transport, options)?;
        fixture::until(|| Ok(record.exists()))?;
        let record_bytes = std::fs::read(&record)?;
        let endpoint = Endpoint::from_record(&record_bytes)?;
        if cancelled {
            fixture::until(|| Ok(directory.join("target-ready").exists()))?;
            let mut wrong = endpoint.clone();
            wrong.token = if endpoint.token == "a".repeat(64) {
                "b".repeat(64)
            } else {
                "a".repeat(64)
            };
            assert!(wrong.stop().is_err());
            assert!(!endpoint.path.join("terminal").exists());
            assert_eq!(std::fs::read(&record)?, record_bytes);
            assert!(
                ticket.try_result().is_none(),
                "wrong token stopped the target"
            );
            fixture::challenge(&canary, &scratch.0, b"after-wrong-token")?;
            ticket.cancel(); // configure's authenticated stop_remote owns cancellation.
        }
        let output = fixture::wait(&ticket)?;
        assert_eq!(
            output.stop_kind,
            if cancelled {
                Some(crate::processes::StopKind::Cancelled)
            } else {
                None
            },
            "{:?}",
            output.stopped
        );
        assert_eq!(output.status.code(), Some(if cancelled { 125 } else { 7 }));
        assert_eq!(
            *identity.lock().map_err(|e| e.to_string())?,
            Some(endpoint.clone())
        );
        if !cancelled {
            assert_eq!(output.stdout, fixture::stdout(b"finite-stdout\0\xff\n"));
            assert_eq!(output.stderr, b"finite-stderr\0\xfe\n");
        }
        let receipt_bytes = endpoint.terminal()?;
        endpoint.stop()?;
        endpoint.stop()?; // Atomic terminal proof makes repeated stop idempotent.
        let receipt: Receipt = serde_json::from_slice(&receipt_bytes)?;
        assert!(endpoint.confirms(&receipt_bytes));
        assert_eq!(receipt.endpoint, endpoint);
        assert_eq!(receipt.state, "stopped");
        assert_eq!(receipt.code, if cancelled { 125 } else { 7 });
        assert_eq!(receipt.complete, !cancelled);
        assert!(!endpoint.path.join("terminal.new").exists());
        assert_eq!(std::fs::read(&record)?, record_bytes);
        assert_eq!(super::super::recover_owner(&record)?, record_bytes);
        let mut wrong_receipt = receipt_bytes.clone();
        wrong_receipt.push(b'x');
        assert!(endpoint.retire_proven(&wrong_receipt).is_err());
        assert_eq!(endpoint.terminal()?, receipt_bytes);
        endpoint.retire_proven(&receipt_bytes)?;
        endpoint.retire_proven(&receipt_bytes)?;
        assert!(!endpoint.path.join("terminal").exists());
        if cancelled {
            std::thread::sleep(fixture::EFFECT_DELAY + Duration::from_millis(100));
            assert!(!directory.join("unexpected-effect").exists());
        }
        fixture::challenge(&canary, &scratch.0, mode.as_bytes())?;
    }
    canary.cancel();
    assert_eq!(
        fixture::wait(&canary)?.stop_kind,
        Some(crate::processes::StopKind::Cancelled)
    );
    drop(service); // Joins only this fixture's Service and its retained children.
    scratch.complete();
    Ok(())
}
#[test]
fn recording_endpoint_refuses_wrong_identity_and_extra_fields() -> Result<(), String> {
    let ep = Endpoint {
        v: 2,
        nonce: "a".repeat(64),
        path: std::env::temp_dir().join(format!("{PREFIX}{}", "b".repeat(32))),
        port: 4000,
        token: "c".repeat(64),
    };
    let bytes = serde_json::to_vec(&ep).map_err(|e| e.to_string())?;
    assert_eq!(Endpoint::from_record(&bytes)?, ep);
    let receipt = serde_json::to_vec(&Receipt {
        endpoint: ep.clone(),
        state: "stopped".into(),
        code: 7,
        complete: true,
    })
    .map_err(|e| e.to_string())?;
    assert!(ep.confirms(&receipt));
    assert!(!ep.confirms(&receipt[..receipt.len() - 1]));
    let mut value = serde_json::to_value(&ep).map_err(|e| e.to_string())?;
    value["extra"] = true.into();
    assert!(
        Endpoint::from_record(&serde_json::to_vec(&value).map_err(|e| e.to_string())?).is_err()
    );
    value.as_object_mut().ok_or("not object")?.remove("extra");
    value["path"] = "/etc/passwd".into();
    assert!(
        Endpoint::from_record(&serde_json::to_vec(&value).map_err(|e| e.to_string())?).is_err()
    );
    assert!(!format!("{ep:?}").contains(&ep.token));
    Ok(())
}
#[test]
fn recording_only_native_helper_gets_controller_mode() -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.args(["-c", "alias.test=!echo x", "test"]);
    let native = command(
        cmd,
        Path::new("/recording-only/0.owner"),
        Duration::from_secs(1),
    )?;
    let mut options = crate::processes::Options::default();
    assert!(!options.native_controller);
    let transport = native.configure(&mut options);
    assert!(options.native_controller && options.hold_input && options.stop_remote.is_some());
    assert_eq!(
        transport.get_program(),
        std::env::current_exe().map_err(|e| e.to_string())?
    );
    Ok(())
}
#[test]
fn recording_native_words_roundtrip_without_loss() -> Result<(), String> {
    for word in [
        OsStr::new("space ü"),
        OsStr::new("$(not-a-command)"),
        OsStr::new(""),
    ] {
        assert_eq!(Word::capture(word).restore()?, word);
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let word = OsStr::from_bytes(b"invalid-\xff");
        assert_eq!(Word::capture(word).restore()?, word);
    }
    Ok(())
}
