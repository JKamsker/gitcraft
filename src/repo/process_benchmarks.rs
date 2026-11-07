//! Opt-in subprocess/reload workloads, sampled by `ci/bench-processes.py`.

use std::time::{Duration, Instant};

use super::{Msg, RepoTab};
use crate::git;

#[test]
#[ignore = "run with ci/bench-processes.py"]
fn workload() -> Result<(), String> {
    let count: usize = std::env::var("GITCRAFT_BENCH_COUNT")
        .map_err(|e| e.to_string())?
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    if count == 0 {
        return Err("benchmark count must be positive".into());
    }
    let mode = std::env::var("GITCRAFT_BENCH_MODE").unwrap_or_else(|_| "commands".into());
    let dir = std::env::temp_dir().join(format!("gitcraft-process-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    git::run(Some(&dir), &["init", "-q"])?;
    git::run(
        Some(&dir),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "base",
        ],
    )?;
    let ctx = egui::Context::default();
    let mut tabs = Vec::new();
    for i in 0..count {
        let local = if mode == "commands" {
            dir.clone()
        } else {
            let local = dir.join(format!("tab-{i}"));
            git::run(
                None,
                &[
                    "clone",
                    "-q",
                    "--shared",
                    &dir.to_string_lossy(),
                    &local.to_string_lossy(),
                ],
            )?;
            local
        };
        #[cfg(unix)]
        let path = if mode == "remote-tabs" {
            std::path::PathBuf::from(format!(
                "ssh://{}{}",
                crate::remote::LOOPBACK,
                local.display()
            ))
        } else {
            local.clone()
        };
        #[cfg(not(unix))]
        let path = local;
        tabs.push(RepoTab::new_deferred(path, &ctx));
    }
    let start = Instant::now();
    if mode == "commands" {
        for tab in &tabs {
            let path = dir.clone();
            super::messages::spawn_or(
                &tab.tx,
                &ctx,
                |e| Msg::Loaded(Err(e)),
                move || {
                    Msg::Loaded(Err(git::run_shell(&path, "sleep 0.1; printf ok")
                        .map(|_| "benchmark completed".to_owned())
                        .unwrap_or_else(|e| e)))
                },
            );
        }
    } else {
        for tab in &mut tabs {
            tab.reload(&ctx);
        }
    }
    let submitted_ms = start.elapsed().as_secs_f64() * 1000.0;
    let mut done = vec![false; count];
    let mut latencies = Vec::new();
    let mut heartbeat_ms = 0.0_f64;
    let mut last = Instant::now();
    while done.iter().any(|d| !d) {
        let now = Instant::now();
        heartbeat_ms = heartbeat_ms.max(now.duration_since(last).as_secs_f64() * 1000.0);
        last = now;
        for (i, tab) in tabs.iter_mut().enumerate() {
            if done[i] {
                continue;
            }
            if mode == "commands" {
                if let Ok(Msg::Loaded(Err(e))) = tab.rx.try_recv() {
                    if e != "benchmark completed" {
                        return Err(e);
                    }
                    done[i] = true;
                }
            } else {
                tab.poll(&ctx);
                if !tab.loading {
                    if let Some(e) = &tab.load_error {
                        return Err(e.clone());
                    }
                    done[i] = true;
                }
            }
            if done[i] {
                latencies.push(start.elapsed().as_secs_f64() * 1000.0);
            }
        }
        if start.elapsed() > Duration::from_secs(120) {
            return Err("benchmark timed out".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    latencies.sort_by(f64::total_cmp);
    println!(
        "PROCESS_BENCH {}",
        serde_json::json!({"count":count,"mode":mode,
        "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"submit_ms":submitted_ms,
        "latency_p50_ms":latencies[count/2],"latency_p95_ms":latencies[(count*95/100).min(count-1)],
        "heartbeat_max_ms":heartbeat_ms,"jobs":crate::tasks::counts(),"job_queue_peak":crate::tasks::peak_queued(),"processes":crate::processes::counts()})
    );
    drop(tabs);
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
