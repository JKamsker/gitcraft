//! A real listener and later starts in this process: handing over a path, coming to the front,
//! strangers, stale files, garbage, and starts at the same moment.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::Duration;

use super::discovery::{self, Found};
use super::message::{self, MAX_LINE};
use super::server::Primary;
use super::{Claim, Request, claim, request_for, temp_dir};

/// How long a later start in these tests tries before it gives up.
const SHORT: Duration = Duration::from_millis(600);

fn absolute(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

fn primary(dir: &Path) -> Result<Primary, String> {
    match claim(dir, &Ok(Request::Focus), SHORT) {
        Claim::Primary(p) => Ok(p),
        other => Err(format!("not the running instance: {}", describe(&other))),
    }
}

fn describe(claim: &Claim) -> String {
    match claim {
        Claim::Primary(_) => "running instance".to_owned(),
        Claim::HandedOver => "handed over".to_owned(),
        Claim::Failed(m) => format!("failed: {m}"),
        Claim::Alone(e) => format!("alone: {e}"),
    }
}

/// What the running instance at `port` answers to `bytes`, until it closes the connection.
fn raw(port: u16, bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    let _ = stream.write_all(bytes);
    let _ = stream.shutdown(std::net::Shutdown::Write);
    let mut reply = Vec::new();
    let _ = stream.read_to_end(&mut reply);
    Ok(reply)
}

#[test]
fn a_path_is_handed_to_the_running_window() -> Result<(), String> {
    let dir = temp_dir("handoff");
    let first = primary(&dir)?;
    let path = absolute("some-repo");
    let second = claim(&dir, &Ok(Request::Open(path.clone())), SHORT);
    assert!(matches!(second, Claim::HandedOver), "{}", describe(&second));
    assert_eq!(first.take_requests(), vec![Request::Open(path)]);
    assert!(first.take_requests().is_empty(), "taken once");
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

#[test]
fn a_bare_start_brings_the_window_to_the_front() -> Result<(), String> {
    let dir = temp_dir("focus");
    let first = primary(&dir)?;
    let second = claim(&dir, &request_for(None), SHORT);
    assert!(matches!(second, Claim::HandedOver), "{}", describe(&second));
    assert_eq!(first.take_requests(), vec![Request::Focus]);
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// `.` and a subfolder are read against the start's own folder, before they are handed over.
#[test]
fn a_relative_path_is_made_absolute_where_it_was_typed() -> Result<(), String> {
    let here = std::env::current_dir().map_err(|e| e.to_string())?;
    assert_eq!(
        request_for(Some(Path::new("."))),
        Ok(Request::Open(here.join(".")))
    );
    assert_eq!(
        request_for(Some(Path::new("src/app"))),
        Ok(Request::Open(here.join("src/app")))
    );
    assert_eq!(request_for(Some(Path::new(""))), Ok(Request::Focus));
    // On another machine: as it is.
    let ssh = Path::new("ssh://build/home/me/repo");
    assert_eq!(request_for(Some(ssh)), Ok(Request::Open(ssh.to_owned())));
    Ok(())
}

/// A file naming the right port with a wrong token: the running instance tells nothing, takes
/// nothing, and the start gives up rather than open a second window.
#[test]
fn a_wrong_token_is_not_answered() -> Result<(), String> {
    let dir = temp_dir("token");
    let first = primary(&dir)?;
    let port = first.port().ok_or("no port")?;
    let forged = Found {
        v: 1,
        port,
        token: "f".repeat(32),
        pid: 1,
    };
    let reply = raw(
        port,
        message::encode_request(&forged.token, &Request::Focus).as_bytes(),
    )?;
    assert!(reply.is_empty(), "{:?}", String::from_utf8_lossy(&reply));
    discovery::write(&dir, &forged).map_err(|e| e.to_string())?;
    let second = claim(&dir, &Ok(Request::Focus), SHORT);
    assert!(
        matches!(&second, Claim::Failed(m) if m.contains("doesn't answer")),
        "{}",
        describe(&second)
    );
    assert!(first.take_requests().is_empty());
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// A crashed instance's file, its port now nobody's: the next start is the running instance,
/// and its own file replaces that one.
#[test]
fn a_crashed_instances_file_is_taken_over() -> Result<(), String> {
    let dir = temp_dir("stale");
    let dead_port = std::net::TcpListener::bind(("127.0.0.1", 0))
        .and_then(|l| l.local_addr())
        .map_err(|e| e.to_string())?
        .port();
    let stale = Found {
        v: 1,
        port: dead_port,
        token: "a".repeat(32),
        pid: 999_999,
    };
    discovery::write(&dir, &stale).map_err(|e| e.to_string())?;
    let first = primary(&dir)?;
    let now = discovery::read(&dir).ok_or("no file")?;
    assert_eq!(Some(now.port), first.port());
    assert_eq!(now.token, first.token());
    assert_ne!(now.token, stale.token);
    // And later starts find it.
    let second = claim(&dir, &Ok(Request::Focus), SHORT);
    assert!(matches!(second, Claim::HandedOver), "{}", describe(&second));
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// An instance that crashed holding the lock lets go of it with its process: dropped here.
#[test]
fn a_lock_let_go_without_shutting_down_is_taken() -> Result<(), String> {
    let dir = temp_dir("crash");
    let first = primary(&dir)?;
    drop(first);
    let second = primary(&dir)?;
    second.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

#[test]
fn an_endless_or_malformed_request_is_dropped() -> Result<(), String> {
    let dir = temp_dir("garbage");
    let first = primary(&dir)?;
    let port = first.port().ok_or("no port")?;
    // No newline, ever: more than the limit, then the end of what it sends.
    let endless = vec![b'{'; usize::try_from(MAX_LINE).unwrap_or(0) + 4096];
    assert!(raw(port, &endless)?.is_empty());
    for garbage in [
        &b"POST / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n{}"[..],
        b"\xff\xfe\n",
        b"{\"kind\":\"focus\"}\n",
        b"",
    ] {
        let reply = raw(port, garbage)?;
        assert!(reply.is_empty(), "{:?}", String::from_utf8_lossy(&reply));
    }
    assert!(first.take_requests().is_empty());
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// A request with the token, but one this instance can't take: refused, saying why, and the
/// start ends with that.
#[test]
fn a_bad_path_from_a_gitcraft_is_refused_with_the_reason() -> Result<(), String> {
    let dir = temp_dir("refused");
    let first = primary(&dir)?;
    let port = first.port().ok_or("no port")?;
    let line = format!(
        "{{\"v\":1,\"token\":\"{}\",\"kind\":\"open\",\"path\":\"relative\"}}\n",
        first.token()
    );
    let reply = String::from_utf8(raw(port, line.as_bytes())?).map_err(|e| e.to_string())?;
    assert!(
        matches!(message::decode_reply(&reply), Some(message::Reply::Refused(w)) if w.contains("absolute")),
        "{reply}"
    );
    let unknown = format!(
        "{{\"v\":1,\"token\":\"{}\",\"kind\":\"exec\"}}\n",
        first.token()
    );
    let reply = String::from_utf8(raw(port, unknown.as_bytes())?).map_err(|e| e.to_string())?;
    assert!(
        matches!(
            message::decode_reply(&reply),
            Some(message::Reply::Refused(_))
        ),
        "{reply}"
    );
    assert!(first.take_requests().is_empty());
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Closed: what comes later is answered "closing", the file goes, and the next start is the
/// running instance (an update starting its new version, a start just as the window closes).
#[test]
fn after_the_window_closes_the_next_start_takes_over() -> Result<(), String> {
    let dir = temp_dir("closing");
    let first = primary(&dir)?;
    let port = first.port().ok_or("no port")?;
    let token = first.token().to_owned();
    first.shut_down();
    let reply = raw(
        port,
        message::encode_request(&token, &Request::Focus).as_bytes(),
    )?;
    assert_eq!(
        message::decode_reply(&String::from_utf8_lossy(&reply)),
        Some(message::Reply::Closing)
    );
    assert_eq!(discovery::read(&dir), None);
    assert!(first.take_requests().is_empty());
    let second = primary(&dir)?;
    second.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Starts at the same moment: exactly one is the running instance, and every other hands its
/// request to it.
#[test]
fn of_starts_at_the_same_moment_only_one_opens_a_window() -> Result<(), String> {
    const STARTS: usize = 8;
    let dir = temp_dir("race");
    let gate = Arc::new(Barrier::new(STARTS));
    let starts: Vec<_> = (0..STARTS)
        .map(|i| {
            let (dir, gate) = (dir.clone(), Arc::clone(&gate));
            std::thread::spawn(move || {
                gate.wait();
                let request = Request::Open(absolute(&format!("repo-{i}")));
                claim(&dir, &Ok(request), Duration::from_secs(10))
            })
        })
        .collect();
    let claims: Vec<Claim> = starts
        .into_iter()
        .map(|s| s.join().map_err(|_| "a start panicked".to_owned()))
        .collect::<Result<_, _>>()?;
    let primaries: Vec<&Primary> = claims
        .iter()
        .filter_map(|c| match c {
            Claim::Primary(p) => Some(p),
            _ => None,
        })
        .collect();
    let handed = claims
        .iter()
        .filter(|c| matches!(c, Claim::HandedOver))
        .count();
    let all: Vec<String> = claims.iter().map(describe).collect();
    assert_eq!(primaries.len(), 1, "{all:?}");
    assert_eq!(handed, STARTS - 1, "{all:?}");
    let taken = primaries
        .first()
        .map(|p| p.take_requests())
        .unwrap_or_default();
    assert_eq!(taken.len(), STARTS - 1);
    for p in primaries {
        p.shut_down();
    }
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Sends one byte every 100 ms on `stream`, without a line break, for `for_ms`.
fn trickle(mut stream: TcpStream, for_ms: u64) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for _ in 0..for_ms / 100 {
            if stream.write_all(b"{").is_err() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    })
}

/// A line sent a byte now and then is given up on once its time is out, all told, not only
/// when one byte is late.
#[test]
fn a_line_sent_byte_by_byte_runs_out_of_time() -> Result<(), String> {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let sender = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
    let (received, _) = listener.accept().map_err(|e| e.to_string())?;
    let slow = trickle(sender, 3000);
    let started = std::time::Instant::now();
    let line = super::server::read_line(&received, Duration::from_millis(500))
        .map_err(|e| e.to_string())?;
    assert_eq!(line, None);
    assert!(
        started.elapsed() < Duration::from_millis(1500),
        "{:?}",
        started.elapsed()
    );
    drop(received);
    let _ = slow.join();
    Ok(())
}

/// Every connection the listener serves at once held open by someone sending a byte now and
/// then: once their time is out, a start is served again.
#[test]
fn slow_senders_dont_lock_later_starts_out() -> Result<(), String> {
    let dir = temp_dir("slow");
    let first = primary(&dir)?;
    let port = first.port().ok_or("no port")?;
    let slow: Vec<_> = (0..super::server::MAX_CONNECTIONS)
        .map(|_| TcpStream::connect(("127.0.0.1", port)).map(|s| trickle(s, 20_000)))
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let second = claim(&dir, &Ok(Request::Focus), Duration::from_secs(15));
    assert!(matches!(second, Claim::HandedOver), "{}", describe(&second));
    assert_eq!(first.take_requests(), vec![Request::Focus]);
    first.shut_down();
    drop(slow);
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// A request the window hadn't taken yet when it closed is answered "closing", not "taken":
/// its start waits for the lock and opens the window itself, rather than end with nothing.
#[test]
fn a_request_not_taken_before_the_window_closes_is_not_lost() -> Result<(), String> {
    let dir = temp_dir("lost");
    let first = primary(&dir)?;
    let path = absolute("late-repo");
    let late = {
        let (dir, path) = (dir.clone(), path.clone());
        std::thread::spawn(move || claim(&dir, &Ok(Request::Open(path)), Duration::from_secs(10)))
    };
    for _ in 0..500 {
        if first.waiting() > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(first.waiting(), 1);
    first.shut_down();
    let late = late.join().map_err(|_| "the start panicked".to_owned())?;
    let Claim::Primary(second) = late else {
        return Err(format!("the late start: {}", describe(&late)));
    };
    second.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}

/// Taken by the window, a request is answered at once.
#[test]
fn a_request_taken_by_the_window_is_answered_at_once() -> Result<(), String> {
    let dir = temp_dir("taken");
    let first = Arc::new(primary(&dir)?);
    let window = {
        let first = Arc::clone(&first);
        std::thread::spawn(move || {
            for _ in 0..400 {
                if !first.take_requests().is_empty() {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            false
        })
    };
    let started = std::time::Instant::now();
    let second = claim(&dir, &Ok(Request::Focus), SHORT);
    assert!(matches!(second, Claim::HandedOver), "{}", describe(&second));
    assert!(
        started.elapsed() < Duration::from_millis(1000),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(window.join().ok(), Some(true));
    first.shut_down();
    std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())
}
