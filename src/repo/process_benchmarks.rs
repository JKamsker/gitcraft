//! Opt-in subprocess/reload workloads, sampled by `ci/bench-processes.py`.

use std::time::{Duration, Instant};

use super::{Msg, RepoTab};
use crate::git;

fn cpu_time_ms() -> Option<f64> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        Some(time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1_000_000.0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

fn ssh_authentications() -> Option<usize> {
    let log = std::env::var_os("GITCRAFT_BENCH_SSH_LOG")?;
    let text = std::fs::read_to_string(log).ok()?;
    Some(
        text.lines()
            .filter(|line| line.contains("Accepted publickey"))
            .count(),
    )
}

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
    let source = std::env::var_os("GITCRAFT_BENCH_REPO").map(std::path::PathBuf::from);
    let rounds = std::env::var("GITCRAFT_BENCH_ROUNDS")
        .unwrap_or_else(|_| "1".into())
        .parse::<usize>()
        .map_err(|e| e.to_string())?;
    if rounds == 0 {
        return Err("benchmark rounds must be positive".into());
    }
    #[cfg(unix)]
    let ssh_host = std::env::var("GITCRAFT_BENCH_SSH_HOST")
        .unwrap_or_else(|_| crate::remote::LOOPBACK.to_owned());
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
                    &source.as_deref().unwrap_or(&dir).to_string_lossy(),
                    &local.to_string_lossy(),
                ],
            )?;
            local
        };
        #[cfg(unix)]
        let path = if mode == "remote-tabs" {
            std::path::PathBuf::from(format!("ssh://{}{}", ssh_host, local.display()))
        } else {
            local.clone()
        };
        #[cfg(not(unix))]
        let path = local;
        tabs.push(RepoTab::new_deferred(path, &ctx));
    }
    for round in 0..rounds {
        let accepted_before = ssh_authentications();
        let spawned_before = crate::processes::counts().map_or(0, |counts| counts.spawned);
        git::backend::reset_metrics();
        let cpu_before = cpu_time_ms();
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
        let cpu_ms = cpu_before
            .zip(cpu_time_ms())
            .map(|(before, after)| after - before);
        println!(
            "PROCESS_BENCH {}",
            serde_json::json!({"count":count,"mode":mode,"round":round,
            "elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"submit_ms":submitted_ms,
            "latency_p50_ms":latencies[count/2],"latency_p95_ms":latencies[(count*95/100).min(count-1)],
            "heartbeat_max_ms":heartbeat_ms,"jobs":crate::tasks::counts(),"job_queue_peak":crate::tasks::peak_queued(),"processes":crate::processes::counts(),
            "measured_subprocesses":crate::processes::counts().map_or(0, |counts| counts.spawned).saturating_sub(spawned_before),
            "parent_cpu_ms":cpu_ms,"real_repository":source.is_some(),
            "git_operations":git::backend::metrics(),
            "ssh_authenticated_connections":accepted_before.zip(ssh_authentications()).map(|(a,b)|b.saturating_sub(a))})
        );
        if round + 1 < rounds {
            // Finish secondary panel jobs before resetting samples for a warm reload.
            let settling = Instant::now();
            let mut idle_since = Instant::now();
            loop {
                for tab in &mut tabs {
                    tab.poll(&ctx);
                }
                if crate::tasks::counts().is_some_and(|counts| counts != (0, 0))
                    || crate::processes::counts()
                        .is_some_and(|counts| counts.active != 0 || counts.queued != 0)
                {
                    idle_since = Instant::now();
                } else if idle_since.elapsed() >= Duration::from_millis(50) {
                    break;
                }
                if settling.elapsed() > Duration::from_secs(30) {
                    return Err("benchmark did not settle before warm reload".into());
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
    drop(tabs);
    // The harness runs this opt-in test alone. Release pooled shells and managed masters,
    // following app shutdown, so benchmark processes leave no persistent SSH daemons.
    crate::tasks::shutdown();
    crate::processes::shutdown();
    crate::remote::shutdown();
    let _ = std::fs::remove_dir_all(dir);
    Ok(())
}
