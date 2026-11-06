//! Cloning, followed and cancelled, as the start page does it.

use std::net::TcpListener;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::*;

/// A clone from a server that never answers, cancelled: it stops, and the folder it was
/// making is gone.
#[test]
fn a_cancelled_clone_leaves_no_folder() -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    // Accepts, then says nothing.
    std::thread::spawn(move || {
        let held: Vec<_> = listener.incoming().take(4).collect();
        std::thread::sleep(Duration::from_secs(60));
        drop(held);
    });
    let dir = std::env::temp_dir().join(format!("gitcraft-clone-cancel {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join("nouveau dépôt");
    let control = OpControl::new(|| {});
    let canceller = Arc::clone(&control);
    let watched = dest.clone();
    std::thread::spawn(move || {
        // Once git has made the folder (or after a while).
        let start = Instant::now();
        while !watched.exists() && start.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(200));
        canceller.cancel();
    });
    let url = format!("git://127.0.0.1:{port}/r.git");
    let started = Instant::now();
    let result = clone(&url, &dest, None, &control);
    let took = started.elapsed();
    let left = dest.exists();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(result, Err(CANCELLED.to_owned()));
    assert!(took < Duration::from_secs(20), "took {took:?}");
    assert!(!left, "the cancelled clone's folder is still there");
    Ok(())
}

/// A clone that goes through reports where it is and makes the repository.
#[test]
fn a_clone_reports_progress() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-clone-progress {}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let src = dir.join("src");
    std::fs::create_dir_all(&src).map_err(|e| e.to_string())?;
    run(Some(&src), &["init", "-q"])?;
    run(
        Some(&src),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "x",
        ],
    )?;
    let control = OpControl::new(|| {});
    // A `file://` URL: through git's transport, as from a server.
    let url = format!("file://{}", src.to_string_lossy().replace('\\', "/"));
    let url = if url.starts_with("file:///") {
        url
    } else {
        url.replacen("file://", "file:///", 1)
    };
    let dest = dir.join("copy");
    let result = clone(&url, &dest, None, &control);
    let head = head(&dest);
    let _ = std::fs::remove_dir_all(&dir);
    result?;
    assert!(!head.is_empty());
    assert!(control.progress().is_some());
    Ok(())
}
