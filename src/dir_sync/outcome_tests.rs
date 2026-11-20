use super::*;

fn code(n: i32) -> io::Error {
    io::Error::from_raw_os_error(n)
}

#[test]
fn each_not_implemented_flush_code_is_unsupported_only_on_windows() {
    // ERROR_INVALID_FUNCTION (SMB, \\wsl$ 9P: observed), ERROR_NOT_SUPPORTED,
    // ERROR_INVALID_PARAMETER, ERROR_CALL_NOT_IMPLEMENTED.
    for n in [1, 50, 87, 120] {
        assert_eq!(unsupported_flush(&code(n)), cfg!(windows), "code {n}");
        let flushed = outcome(Err(code(n)));
        assert_eq!(
            matches!(flushed, Ok(Flushed::Unsupported(_))),
            cfg!(windows),
            "code {n}: {flushed:?}"
        );
    }
    // Access denied, sharing violation, not found, disk full: never "unsupported".
    for n in [5, 32, 2, 112] {
        assert!(!unsupported_flush(&code(n)), "code {n}");
    }
    assert!(!unsupported_flush(&io::Error::other("no code")));
}

#[test]
fn a_flush_refused_after_a_successful_open_is_a_failure_never_a_skip() {
    let refused = outcome(Err(io::Error::from(io::ErrorKind::PermissionDenied)));
    let failure = refused
        .err()
        .map(|f| (f.opened, f.unopenable(), f.denied()));
    assert_eq!(failure, Some((true, false, false)));
    let held = outcome(Err(code(32))).err().map(|f| f.unopenable());
    assert_eq!(
        held,
        Some(false),
        "a sharing violation while flushing is no open failure"
    );
    assert!(matches!(outcome(Ok(())), Ok(Flushed::Done)));
}

#[test]
fn only_windows_open_denial_or_sharing_violation_is_unopenable() {
    let denied = Failure::new(false, io::Error::from(io::ErrorKind::PermissionDenied));
    assert_eq!(
        (denied.unopenable(), denied.denied()),
        (cfg!(windows), cfg!(windows))
    );
    let held = Failure::new(false, code(32));
    assert_eq!((held.unopenable(), held.denied()), (cfg!(windows), false));
    let missing = Failure::new(false, io::Error::from(io::ErrorKind::NotFound));
    assert!(!missing.unopenable() && !missing.denied());
    assert!(
        Failure::new(false, code(5))
            .to_string()
            .starts_with("open: ")
    );
    assert!(
        Failure::new(true, code(5))
            .to_string()
            .starts_with("flush: ")
    );
}

#[test]
fn a_held_directory_is_retried_five_times_with_growing_pauses_then_fails() {
    let mut attempts = 0;
    let mut pauses = Vec::new();
    let result = retrying(
        || {
            attempts += 1;
            Err(code(32))
        },
        |pause| pauses.push(pause.as_millis()),
    );
    let failure = result.err().map(|f| (f.opened, f.error.raw_os_error()));
    assert_eq!(failure, Some((false, Some(32))));
    if cfg!(windows) {
        assert_eq!((attempts, pauses), (5, vec![20, 40, 60, 80]));
    } else {
        assert_eq!(
            (attempts, pauses.len()),
            (1, 0),
            "no sharing violations on Unix"
        );
    }
}

#[test]
fn access_denial_and_other_open_errors_are_never_retried() {
    for error in [io::ErrorKind::PermissionDenied, io::ErrorKind::NotFound] {
        let (mut attempts, mut pauses) = (0, 0);
        let result = retrying(
            || {
                attempts += 1;
                Err(io::Error::from(error))
            },
            |_| pauses += 1,
        );
        assert!(result.is_err());
        assert_eq!((attempts, pauses), (1, 0), "{error:?}");
    }
}

#[test]
fn a_directory_released_while_retrying_is_opened() -> io::Result<()> {
    // Any openable file stands in for the directory handle.
    let file = std::env::current_exe()?;
    let mut attempts = 0;
    let opened = retrying(
        || {
            attempts += 1;
            if attempts < 3 && cfg!(windows) {
                Err(code(32))
            } else {
                std::fs::File::open(&file)
            }
        },
        |_| {},
    );
    assert!(opened.is_ok());
    assert_eq!(attempts, if cfg!(windows) { 3 } else { 1 });
    Ok(())
}

/// Real evidence on a filesystem without directory flushes. Run with
/// `GITCRAFT_TEST_UNFLUSHABLE_DIR=\\localhost\D$\...\target` (SMB) or
/// `\\wsl$\<distro>\tmp` (9P), and `--ignored`.
#[cfg(windows)]
#[test]
#[ignore = "needs GITCRAFT_TEST_UNFLUSHABLE_DIR on an SMB or 9P path"]
fn a_real_smb_or_9p_directory_reports_unsupported_flush() -> io::Result<()> {
    let base = std::env::var_os("GITCRAFT_TEST_UNFLUSHABLE_DIR")
        .ok_or_else(|| io::Error::other("set GITCRAFT_TEST_UNFLUSHABLE_DIR"))?;
    let dir =
        std::path::Path::new(&base).join(format!("gitcraft-unflushable-{}", std::process::id()));
    std::fs::create_dir(&dir)?;
    let flushed = sync(&dir);
    std::fs::remove_dir(&dir)?;
    match flushed {
        Ok(Flushed::Unsupported(e)) => {
            assert_eq!(e.raw_os_error(), Some(1), "{e}");
            Ok(())
        }
        other => Err(io::Error::other(format!("expected unsupported: {other:?}"))),
    }
}
