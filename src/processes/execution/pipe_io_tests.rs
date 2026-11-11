//! Recording-only I/O cases: no child, PID lookup, signal, or external endpoint.
use super::*;

#[test]
fn readiness_preserves_payload_at_every_chunk_boundary() {
    let bytes = b"noise\n  READY \r\n\0binary\xffpayload\n";
    for cut in 0..=bytes.len() {
        let mut capture = Capture::default();
        let mut options = Options {
            stdout_ready: Some("READY"),
            ..Options::default()
        };
        let blocking = Arc::new(tokio::sync::Semaphore::new(1));
        let mut stopped = None;
        capture.stdout(&bytes[..cut], &mut options, &blocking, &mut stopped);
        capture.stdout(&bytes[cut..], &mut options, &blocking, &mut stopped);
        assert!(capture.ready, "cut {cut}");
        assert_eq!(capture.stdout, b"\0binary\xffpayload\n", "cut {cut}");
        assert!(stopped.is_none());
    }
}

#[test]
fn unconfirmed_preamble_is_bounded_and_never_becomes_output() {
    let mut capture = Capture::default();
    let mut options = Options {
        stdout_ready: Some("READY"),
        ..Options::default()
    };
    let blocking = Arc::new(tokio::sync::Semaphore::new(1));
    let mut stopped = None;
    capture.stdout(&vec![b'x'; 100_000], &mut options, &blocking, &mut stopped);
    assert_eq!(capture.preamble.len(), 65536);
    assert!(!capture.ready);
    assert!(capture.stdout.is_empty());
    capture.stdout(b"\nREADY\nactual", &mut options, &blocking, &mut stopped);
    assert!(capture.ready);
    assert_eq!(capture.stdout, b"actual");
}

#[tokio::test(flavor = "current_thread")]
async fn connection_callback_runs_once_without_consuming_later_payload() -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mut capture = Capture::default();
    let mut options = Options {
        stdout_ready: Some("READY"),
        ready: Some(Box::new(move || {
            let _ = tx.send(());
        })),
        ..Options::default()
    };
    let blocking = Arc::new(tokio::sync::Semaphore::new(1));
    let mut stopped = None;
    capture.stdout(b"READY\nfirst", &mut options, &blocking, &mut stopped);
    rx.await.map_err(|e| e.to_string())?;
    assert!(options.ready.is_none());
    capture.stdout(b"READY\nsecond", &mut options, &blocking, &mut stopped);
    assert_eq!(capture.stdout, b"firstREADY\nsecond");
    Ok(())
}

#[test]
fn filters_callbacks_and_independent_capture_limits_keep_their_order() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&seen);
    let mut options = Options {
        output_limit: 3,
        stderr_limit: Some(2),
        stderr_filter: Some(Box::new(|bytes| {
            bytes.iter().copied().filter(|b| *b != b'x').collect()
        })),
        stderr: Some(Box::new(move |bytes| {
            observed
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend_from_slice(bytes)
        })),
        ..Options::default()
    };
    let mut capture = Capture {
        ready: true,
        ..Capture::default()
    };
    let blocking = Arc::new(tokio::sync::Semaphore::new(1));
    let mut stopped = None;
    capture.stdout(b"abcde", &mut options, &blocking, &mut stopped);
    assert_eq!(capture.stdout, b"abc");
    assert!(stopped.is_some());
    stopped = None;
    capture.stderr(b"x12x3", &mut options, &mut stopped);
    assert_eq!(capture.stderr, b"12");
    assert_eq!(*seen.lock().unwrap_or_else(|e| e.into_inner()), b"123");
    assert!(stopped.is_some());
}

#[test]
#[allow(clippy::panic)] // Deliberate callback failures must retain captured diagnostics.
fn callback_panics_do_not_lose_bytes_or_hide_failure() {
    let mut options = Options {
        stdout: Some(Box::new(|_| panic!("stdout callback"))),
        stderr: Some(Box::new(|_| panic!("stderr callback"))),
        ..Options::default()
    };
    let mut capture = Capture {
        ready: true,
        ..Capture::default()
    };
    let blocking = Arc::new(tokio::sync::Semaphore::new(1));
    let mut stopped = None;
    capture.stdout(b"output", &mut options, &blocking, &mut stopped);
    assert_eq!(capture.stdout, b"output");
    assert_eq!(stopped.as_deref(), Some("process output callback panicked"));
    capture.stderr(b"errors", &mut options, &mut stopped);
    assert_eq!(capture.stderr, b"errors");
    assert_eq!(
        stopped.as_deref(),
        Some("process progress callback panicked")
    );
}

#[test]
fn recording_pipe_outcomes_preserve_input_and_failure_classification() {
    let mut pipes = Pipes {
        input: None,
        out: None,
        err: None,
        capture: Capture::default(),
        written: 7,
        obuf: [0; 8192],
        ebuf: [0; 8192],
    };
    let mut stopped = None;
    pipes.input_written(Ok(false), true, &mut stopped);
    assert_eq!(pipes.written, 7);
    pipes.input_written(Ok(true), true, &mut stopped);
    assert_eq!(pipes.written, usize::MAX);
    pipes.input_written(Err(io::ErrorKind::BrokenPipe.into()), false, &mut stopped);
    assert!(stopped.is_none());
    pipes.input_written(Err(io::ErrorKind::WriteZero.into()), false, &mut stopped);
    assert!(
        stopped
            .as_deref()
            .is_some_and(|s| s.starts_with("could not pass process its input:"))
    );
    let mut options = Options::default();
    let blocking = Arc::new(tokio::sync::Semaphore::new(1));
    pipes.stdout_read(
        Err(io::ErrorKind::Other.into()),
        &mut options,
        &blocking,
        &mut stopped,
    );
    assert!(
        stopped
            .as_deref()
            .is_some_and(|s| s.starts_with("could not read process output:"))
    );
    pipes.stderr_read(Err(io::ErrorKind::Other.into()), &mut options, &mut stopped);
    assert!(
        stopped
            .as_deref()
            .is_some_and(|s| s.starts_with("could not read process errors:"))
    );
}
