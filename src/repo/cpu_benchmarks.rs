//! Opt-in CPU workloads. `python3 ci/bench-cpu.py --label before` freezes real repository
//! inputs and runs this test in isolation; ordinary `cargo test` never runs benchmarks.

use std::hash::{Hash as _, Hasher as _};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::{DiffKey, RepoTab, diff, highlight};
use crate::{git, graph, settings::Settings};

#[derive(Deserialize)]
struct Inputs {
    repo: PathBuf,
    revision: String,
    revisions: Vec<String>,
    sources: Vec<String>,
    diffs: Vec<Patch>,
}

#[derive(Deserialize)]
struct Patch {
    revision: String,
    path: String,
}

#[derive(Serialize)]
struct Measurement {
    name: String,
    iterations_per_sample: usize,
    samples_ms: Vec<f64>,
    median_ms: f64,
    p95_ms: f64,
    cpu_samples_ms: Vec<f64>,
    median_cpu_ms: Option<f64>,
}

struct Bench {
    filter: String,
    samples: usize,
    sample_time: Duration,
    results: Vec<Measurement>,
}

impl Bench {
    fn measure(&mut self, name: &str, mut run: impl FnMut()) {
        if !self.filter.is_empty() && !name.contains(&self.filter) {
            return;
        }
        for _ in 0..3 {
            run();
        }
        let start = Instant::now();
        run();
        let iterations = (self.sample_time.as_secs_f64() / start.elapsed().as_secs_f64())
            .ceil()
            .clamp(1.0, 10_000.0) as usize;
        let mut samples_ms = Vec::new();
        let mut cpu_samples_ms = Vec::new();
        for _ in 0..self.samples {
            let cpu_start = cpu_time_ms();
            let start = Instant::now();
            for _ in 0..iterations {
                run();
            }
            samples_ms.push(start.elapsed().as_secs_f64() * 1000.0 / iterations as f64);
            if let (Some(start), Some(end)) = (cpu_start, cpu_time_ms()) {
                cpu_samples_ms.push((end - start) / iterations as f64);
            }
        }
        let mut sorted = samples_ms.clone();
        sorted.sort_by(f64::total_cmp);
        let median_ms = sorted[sorted.len() / 2];
        let p95_ms = sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)];
        let mut cpu_sorted = cpu_samples_ms.clone();
        cpu_sorted.sort_by(f64::total_cmp);
        let median_cpu_ms = cpu_sorted.get(cpu_sorted.len() / 2).copied();
        println!(
            "{name}: median {median_ms:.3} ms, p95 {p95_ms:.3} ms ({iterations} iterations/sample)"
        );
        if let Some(cpu) = median_cpu_ms {
            println!("  process CPU: {cpu:.3} ms/iteration (excludes Git children)");
        }
        self.results.push(Measurement {
            name: name.to_owned(),
            iterations_per_sample: iterations,
            samples_ms,
            median_ms,
            p95_ms,
            cpu_samples_ms,
            median_cpu_ms,
        });
    }
}

fn cpu_time_ms() -> Option<f64> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let time = rustix::time::clock_gettime(rustix::time::ClockId::ProcessCPUTime);
        Some(time.tv_sec as f64 * 1000.0 + time.tv_nsec as f64 / 1_000_000.0)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    None
}

#[test]
#[ignore = "real CPU workloads; run through ci/bench-cpu.py"]
fn real_world_cpu() -> Result<(), String> {
    let input = std::env::var("GITCRAFT_BENCH_INPUTS").map_err(|e| e.to_string())?;
    let inputs: Inputs = serde_json::from_slice(&std::fs::read(input).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut bench = Bench {
        filter: std::env::var("GITCRAFT_BENCH_FILTER").unwrap_or_default(),
        samples: std::env::var("GITCRAFT_BENCH_SAMPLES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9)
            .max(1),
        sample_time: Duration::from_millis(
            std::env::var("GITCRAFT_BENCH_SAMPLE_MS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(50),
        ),
        results: Vec::new(),
    };
    let mut fingerprints = std::collections::BTreeMap::new();
    let commits = git::log(
        &inputs.repo,
        Some(&inputs.revisions),
        git::LogOrder::default(),
        0,
        1500,
    )?;
    println!(
        "Frozen revision {}, {} history commits",
        inputs.revision,
        commits.len()
    );
    bench.measure("history/read-first-500", || {
        let result = black_box(git::log(
            &inputs.repo,
            Some(&inputs.revisions),
            git::LogOrder::default(),
            0,
            500,
        ));
        assert!(result.is_ok(), "History read failed: {:?}", result.err());
    });
    bench.measure("history/layout-1500", || {
        black_box(graph::layout(&commits));
    });

    for path in &inputs.sources {
        let bytes = git::file_at(&inputs.repo, Some(&inputs.revision), path)?;
        let source = String::from_utf8(bytes).map_err(|e| e.to_string())?;
        let lines: Vec<_> = source.lines().collect();
        // A skipped/oversized input must not appear to be a very fast highlighting result.
        let coloured = highlight::highlight(path, lines.iter().copied(), true)
            .ok_or_else(|| format!("Highlight benchmark input was skipped: {path}"))?;
        // Per-byte colours ignore harmless changes to token boundaries between engines.
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for line in &coloured {
            for (colour, text) in line {
                for byte in text.bytes() {
                    colour.hash(&mut hash);
                    byte.hash(&mut hash);
                }
            }
            0xffu8.hash(&mut hash);
        }
        fingerprints.insert(path, hash.finish());
        bench.measure(&format!("highlight/{path}"), || {
            black_box(highlight::highlight(path, lines.iter().copied(), true));
        });
    }
    for patch in &inputs.diffs {
        let key = DiffKey::Commit {
            hash: patch.revision.clone(),
            path: patch.path.clone(),
        };
        diff::load(&inputs.repo, &key, git::DiffOptions::default(), true)?;
        bench.measure(&format!("diff/open/{}", patch.path), || {
            let result = black_box(diff::load(
                &inputs.repo,
                &key,
                git::DiffOptions::default(),
                true,
            ));
            assert!(result.is_ok(), "Diff load failed: {:?}", result.err());
        });
    }

    let ctx = egui::Context::default();
    crate::theme::install_fonts(&ctx);
    egui_extras::install_image_loaders(&ctx);
    crate::image_fit::install(&ctx);
    crate::image_cache::install(&ctx);
    Settings {
        avatars: false,
        offline: true,
        ..Default::default()
    }
    .put_for_test(&ctx);
    // A deferred tab: fixed loaded data, no watcher, network requests, git operations or drafts.
    let mut tab = RepoTab::new_deferred(inputs.repo.clone(), &ctx);
    tab.commits = commits.into_iter().map(Arc::new).collect();
    tab.graph = graph::layout(&tab.commits);
    tab.index = tab
        .commits
        .iter()
        .enumerate()
        .map(|(i, c)| (c.hash.clone(), i))
        .collect();
    tab.refs = git::refs(&inputs.repo)?;
    tab.loading = false;
    tab.has_more = false;
    let mut frame = 0u64;
    let mut draw = |scroll: bool| {
        frame += 1;
        let mut events = vec![egui::Event::PointerMoved(egui::pos2(700.0, 400.0))];
        if scroll {
            if frame.is_multiple_of(180) {
                tab.scroll_to = Some(0);
            }
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -36.0),
                modifiers: egui::Modifiers::NONE,
                phase: egui::TouchPhase::Move,
            });
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 800.0),
                )),
                time: Some(frame as f64 / 60.0),
                events,
                ..Default::default()
            },
            |ui| {
                crate::image_cache::maintain(&ctx);
                egui::CentralPanel::default().show(ui, |ui| tab.graph_view(ui, &mut Vec::new()));
            },
        );
        black_box(ctx.tessellate(std::mem::take(&mut output.shapes), output.pixels_per_point));
        output.drop_without_applying_deltas();
    };
    for _ in 0..20 {
        draw(false);
    }
    bench.measure("ui/graph-warm-frame", || draw(false));
    bench.measure("ui/graph-scroll-frame", || draw(true));
    if bench.results.is_empty() {
        return Err("No benchmarks matched".to_owned());
    }
    let output = std::env::var("GITCRAFT_BENCH_OUTPUT").map_err(|e| e.to_string())?;
    std::fs::write(
        output,
        serde_json::to_vec_pretty(&serde_json::json!({
            "measurements": bench.results, "highlight_fingerprints": fingerprints,
        }))
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}
