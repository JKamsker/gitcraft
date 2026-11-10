use super::*;
use std::io::{Read, Write};

#[test]
fn final_buffered_stdin_write_is_flushed_and_its_error_is_observed() -> io::Result<()> {
    use std::pin::Pin;
    use std::task::{Context, Poll};
    struct Buffered {
        fail: bool,
        flushed: bool,
    }
    impl tokio::io::AsyncWrite for Buffered {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            bytes: &[u8],
        ) -> Poll<io::Result<usize>> {
            Poll::Ready(Ok(bytes.len()))
        }
        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            if !self.flushed {
                self.flushed = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            Poll::Ready(if self.fail {
                Err(io::Error::other("deferred write failed"))
            } else {
                Ok(())
            })
        }
        fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
            unreachable!()
        }
    }
    let runtime = tokio::runtime::Builder::new_current_thread().build()?;
    runtime.block_on(async {
        for fail in [false, true] {
            let mut pipe = Buffered {
                fail,
                flushed: false,
            };
            let mut written = 0;
            assert!(!write_input(&mut pipe, b"input", &mut written).await?);
            assert_eq!(written, 5);
            assert!(!pipe.flushed, "acceptance is not completion");
            let result = write_input(&mut pipe, b"input", &mut written).await;
            assert!(pipe.flushed);
            if fail {
                assert!(result.is_err_and(|e| e.to_string() == "deferred write failed"));
            } else {
                assert!(result?);
            }
        }
        Ok(())
    })
}

/// A real child executable on every OS, independent of shell quoting/tools.
#[test]
fn child() -> Result<(), String> {
    let Ok(mode) = std::env::var("GITCRAFT_PROCESS_TEST_CHILD") else {
        return Ok(());
    };
    if let Ok(path) = std::env::var("GITCRAFT_PROCESS_TEST_PID") {
        std::fs::write(path, std::process::id().to_string()).map_err(|e| e.to_string())?;
    }
    match mode.as_str() {
        "flood" => {
            for _ in 0..128 {
                std::io::stdout()
                    .write_all(&[b'o'; 8192])
                    .map_err(|e| e.to_string())?;
                std::io::stderr()
                    .write_all(&[b'e'; 8192])
                    .map_err(|e| e.to_string())?;
            }
            let mut input = Vec::new();
            std::io::stdin()
                .read_to_end(&mut input)
                .map_err(|e| e.to_string())?;
            println!("INPUT={}", input.len());
        }
        "fail" => {
            eprintln!("child failed deliberately");
            std::process::exit(7);
        }
        "stdout-large" => {
            for _ in 0..128 {
                std::io::stdout()
                    .write_all(&[b'x'; 8192])
                    .map_err(|e| e.to_string())?;
            }
        }
        "hang" => {
            println!("partial stdout");
            eprintln!("partial stderr");
            std::thread::sleep(Duration::from_secs(60));
        }
        "descendant" => {
            let marker =
                std::env::var_os("GITCRAFT_PROCESS_TEST_EFFECT").ok_or("missing marker")?;
            let marker = std::path::PathBuf::from(marker);
            let child = command("side-effect")?
                .stdin(Stdio::null())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|e| e.to_string())?;
            println!("DESCENDANT={}", child.id());
            until(|| marker.with_extension("ready").exists())?;
            std::process::exit(0);
        }
        "side-effect" => {
            let marker = std::path::PathBuf::from(
                std::env::var_os("GITCRAFT_PROCESS_TEST_EFFECT").ok_or("missing marker")?,
            );
            std::fs::write(marker.with_extension("ready"), b"ready").map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_secs(1));
            std::fs::write(marker, b"survived").map_err(|e| e.to_string())?;
        }
        "ready" => {
            print!("login banner\nREADY_");
            std::io::stdout().flush().map_err(|e| e.to_string())?;
            std::thread::sleep(Duration::from_millis(10));
            print!("MARKER\r\ncommand output");
        }
        "short" => std::thread::sleep(Duration::from_millis(150)),
        _ => return Err(format!("unknown child mode: {mode}")),
    }
    Ok(())
}

fn command(mode: &str) -> Result<Command, String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .args(["processes::tests::child", "--exact", "--nocapture"])
        .env("GITCRAFT_PROCESS_TEST_CHILD", mode);
    Ok(command)
}

fn until(mut condition: impl FnMut() -> bool) -> Result<(), String> {
    let start = Instant::now();
    while !condition() {
        if start.elapsed() > Duration::from_secs(10) {
            return Err("condition timed out".into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

#[test]
fn drains_both_full_pipes_while_writing_large_input() -> Result<(), String> {
    let service = Service::new(2, 4).map_err(|e| e.to_string())?;
    let out = service
        .submit(
            command("flood")?,
            Options {
                input: Some(vec![b'i'; 2 << 20]),
                timeout: Some(Duration::from_secs(15)),
                ..Default::default()
            },
        )?
        .wait()?
        .complete()?;
    assert!(out.status.success());
    assert!(out.stdout.iter().filter(|&&b| b == b'o').count() >= 1 << 20);
    assert_eq!(out.stderr, vec![b'e'; 1 << 20]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("INPUT=2097152"));
    Ok(())
}

#[test]
fn failures_and_spawn_errors_reach_the_caller() -> Result<(), String> {
    let service = Service::new(1, 4).map_err(|e| e.to_string())?;
    let out = service
        .submit(command("fail")?, Options::default())?
        .wait()?
        .complete()?;
    assert_eq!(out.status.code(), Some(7));
    assert!(String::from_utf8_lossy(&out.stderr).contains("child failed deliberately"));
    let not_ready = service
        .submit(
            command("fail")?,
            Options {
                stdout_ready: Some("never ready"),
                ..Default::default()
            },
        )?
        .wait()?;
    assert!(
        not_ready
            .stopped
            .as_deref()
            .is_some_and(|s| s.contains("child failed deliberately"))
    );
    assert!(
        service
            .submit(
                Command::new("gitcraft-no-such-executable-160"),
                Options::default()
            )?
            .wait()
            .is_err()
    );
    Ok(())
}

#[test]
fn many_processes_share_bounded_slots_and_keep_their_results() -> Result<(), String> {
    let service = Service::new(3, 32).map_err(|e| e.to_string())?;
    let mut tickets = Vec::new();
    for _ in 0..16 {
        tickets.push(service.submit(command("short")?, Options::default())?);
    }
    for ticket in tickets {
        assert!(ticket.wait()?.complete()?.status.success());
    }
    assert_eq!(service.counts().spawned, 16);
    assert_eq!(service.counts().peak, 3);
    assert_eq!(service.counts().active, 0);
    Ok(())
}

#[test]
fn queue_backpressure_is_immediate_and_cancelled_queued_work_never_spawns() -> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let first = service.submit(command("hang")?, Options::default())?;
    until(|| service.counts().spawned == 1)?;
    let second = service.submit(command("fail")?, Options::default())?;
    let third = service.submit(command("fail")?, Options::default())?;
    let start = Instant::now();
    assert!(
        service
            .submit(command("fail")?, Options::default())
            .is_err()
    );
    assert!(start.elapsed() < Duration::from_secs(1));
    second.cancel();
    third.cancel();
    first.cancel();
    assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
    assert!(second.wait().is_err());
    assert!(third.wait().is_err());
    assert_eq!(service.counts().spawned, 1);
    Ok(())
}

#[test]
fn timeout_keeps_partial_output_and_reaps_the_child() -> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let out = service
        .submit(
            command("hang")?,
            Options {
                timeout: Some(Duration::from_millis(300)),
                ..Default::default()
            },
        )?
        .wait()?;
    assert!(
        out.stopped
            .as_deref()
            .is_some_and(|s| s.contains("timed out"))
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("partial stdout"));
    assert!(String::from_utf8_lossy(&out.stderr).contains("partial stderr"));
    assert!(!out.status.success());
    assert!(started.elapsed() < Duration::from_secs(10));
    #[cfg(windows)]
    assert_eq!(out.status.code(), Some(1));
    Ok(())
}

#[test]
fn output_overflow_is_bounded_and_reported() -> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let out = service
        .submit(
            command("flood")?,
            Options {
                output_limit: 32768,
                ..Default::default()
            },
        )?
        .wait()?;
    assert!(
        out.stopped
            .as_deref()
            .is_some_and(|s| s.contains("capture limit"))
    );
    assert!(out.stdout.len() <= 32768 && out.stderr.len() <= 32768);
    Ok(())
}

#[test]
fn smaller_stderr_limit_stops_a_process_with_large_stdout_budget() -> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let out = service
        .submit(
            command("flood")?,
            Options {
                output_limit: 2 << 20,
                stderr_limit: Some(1024),
                ..Default::default()
            },
        )?
        .wait()?;
    assert!(
        out.stopped
            .as_deref()
            .is_some_and(|s| s.contains("capture limit"))
    );
    assert!(out.stderr.len() <= 1024);
    assert!(out.stdout.len() < 2 << 20);
    Ok(())
}

#[test]
fn exports_can_stream_to_disk_beyond_the_capture_limit() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("gitcraft-stream-export-{}", std::process::id()));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let service = Service::new(1, 4).map_err(|e| e.to_string())?;
    let out = service
        .submit(
            command("stdout-large")?,
            Options {
                stdout_file: Some(file),
                output_limit: 1024,
                ..Default::default()
            },
        )?
        .wait()?
        .complete()?;
    let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    let _ = std::fs::remove_file(path);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(size >= 1024 * 1024);
    Ok(())
}

#[test]
fn dropping_a_ticket_cancels_it_and_shutdown_cleans_up_running_and_queued_children()
-> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let first = service.submit(command("hang")?, Options::default())?;
    until(|| service.counts().spawned == 1)?;
    drop(first);
    until(|| service.counts().active == 0)?;
    let second = service.submit(command("hang")?, Options::default())?;
    until(|| service.counts().spawned == 2)?;
    let queued = service.submit(command("fail")?, Options::default())?;
    service.shutdown();
    assert_eq!(second.wait()?.stopped.as_deref(), Some("cancelled"));
    assert!(queued.wait().is_err());
    assert_eq!(service.counts().spawned, 2);
    assert_eq!(service.counts().active, 0);
    assert!(
        service
            .submit(command("fail")?, Options::default())
            .is_err()
    );
    Ok(())
}

#[test]
fn inherited_open_pipes_have_a_deadline_and_descendants_are_stopped() -> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let effect =
        std::env::temp_dir().join(format!("gitcraft-descendant-effect-{}", std::process::id()));
    let _ = std::fs::remove_file(&effect);
    let _ = std::fs::remove_file(effect.with_extension("ready"));
    let mut command = command("descendant")?;
    command.env("GITCRAFT_PROCESS_TEST_EFFECT", &effect);
    let start = Instant::now();
    let out = service
        .submit(
            command,
            Options {
                drain_for: Duration::from_millis(100),
                ..Default::default()
            },
        )?
        .wait()?;
    assert!(
        out.stopped
            .as_deref()
            .is_some_and(|s| s.contains("kept its output open"))
    );
    assert!(start.elapsed() < Duration::from_secs(10));
    assert!(String::from_utf8_lossy(&out.stdout).contains("DESCENDANT="));
    assert!(effect.with_extension("ready").exists());
    std::thread::sleep(Duration::from_millis(1200));
    let survived = effect.exists();
    let _ = std::fs::remove_file(&effect);
    let _ = std::fs::remove_file(effect.with_extension("ready"));
    assert!(!survived, "a descendant survived process-tree cleanup");
    Ok(())
}

#[test]
fn timeouts_expire_while_queued_without_spawning() -> Result<(), String> {
    let service = Service::new(1, 4).map_err(|e| e.to_string())?;
    let first = service.submit(command("hang")?, Options::default())?;
    until(|| service.counts().spawned == 1)?;
    let timed = service.submit(
        command("short")?,
        Options {
            timeout: Some(Duration::from_millis(100)),
            ..Default::default()
        },
    )?;
    until(|| service.counts().queued == 0)?;
    assert!(timed.wait().is_err_and(|e| e.contains("timed out")));
    assert_eq!(service.counts().spawned, 1);
    first.cancel();
    assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
    Ok(())
}

#[test]
fn idle_pipes_do_not_starve_other_child_io() -> Result<(), String> {
    let service = Service::new(3, 8).map_err(|e| e.to_string())?;
    let first = service.submit(command("hang")?, Options::default())?;
    let second = service.submit(command("hang")?, Options::default())?;
    until(|| service.counts().spawned == 2)?;
    let out = service
        .submit(
            command("short")?,
            Options {
                timeout: Some(Duration::from_secs(3)),
                ..Default::default()
            },
        )?
        .wait()?
        .complete()?;
    assert!(out.status.success());
    first.cancel();
    second.cancel();
    assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
    assert_eq!(second.wait()?.stopped.as_deref(), Some("cancelled"));
    Ok(())
}
#[test]
fn remote_ready_marker_can_span_reads_and_keeps_the_first_output_bytes() -> Result<(), String> {
    let service = Service::new(1, 2).map_err(|e| e.to_string())?;
    let out = service
        .submit(
            command("ready")?,
            Options {
                stdout_ready: Some("READY_MARKER"),
                ..Default::default()
            },
        )?
        .wait()?
        .complete()?;
    assert!(out.stdout.starts_with(b"command output"));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("login banner"));
    assert!(out.status.success());
    Ok(())
}
#[test]
fn long_operations_leave_slots_for_interactive_reads() -> Result<(), String> {
    let service = Service::new(3, 8).map_err(|e| e.to_string())?;
    let first = service.submit(
        command("hang")?,
        Options {
            background: true,
            ..Default::default()
        },
    )?;
    until(|| service.counts().spawned == 1)?;
    let second = service.submit(
        command("hang")?,
        Options {
            background: true,
            ..Default::default()
        },
    )?;
    let read = service.submit(command("short")?, Options::default())?;
    assert!(read.wait()?.status.success());
    assert_eq!(
        service.counts().spawned,
        2,
        "a second long operation consumed a reserved read slot"
    );
    service.shutdown();
    assert_eq!(first.wait()?.stopped.as_deref(), Some("cancelled"));
    assert!(second.wait().is_err());
    Ok(())
}
