//! Health of real repositories: counts, the commit-graph and the untracked cache turned on.

use super::health::{self, State};
use super::tests::{borrowed, git, repo, write};

#[test]
fn health_is_read_and_speed_ups_turned_on() -> Result<(), String> {
    let dir = repo("health", &[("a.txt", b"a\n"), ("é dir/b.txt", b"b\n")])?;
    let result = (|| {
        for i in 0..3 {
            write(&dir.join("a.txt"), format!("{i}\n").as_bytes())?;
            git(&dir, &["commit", "-qam", &format!("c{i}")])?;
        }
        let h = health::health(&dir, true)?;
        assert_eq!(h.commits, Some(4));
        assert!(h.objects.loose > 0, "{h:?}");
        assert!(h.status_took.is_some());
        assert!(!h.commit_graph && !h.untracked_cache && !h.maintenance);
        // On Linux (or another machine) git has no monitor of its own.
        assert_eq!(h.fsmonitor, Some(State::Unsupported));

        // Each suggestion's commands, run as the doctor runs them, turn it on.
        for s in health::suggestions(&h) {
            if matches!(s.title, "Commit-graph" | "Untracked cache") {
                for op in &s.ops {
                    git(&dir, &borrowed(op))?;
                }
            }
        }
        // And packing, which isn't suggested for so few objects, works the same.
        git(&dir, &["gc", "--quiet"])?;
        let after = health::health(&dir, true)?;
        assert!(after.commit_graph && after.untracked_cache, "{after:?}");
        assert!(after.objects.packs >= 1 && after.objects.loose < h.objects.loose);
        let states: Vec<_> = health::suggestions(&after)
            .into_iter()
            .map(|s| (s.title, s.state))
            .collect();
        assert!(states.contains(&("Commit-graph", State::On)));
        assert!(states.contains(&("Untracked cache", State::On)));
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn shallow_clones_empty_chains_and_the_monitor_set_on_linux() -> Result<(), String> {
    let dir = repo("health-shallow", &[("a.txt", b"a\n")])?;
    let clone = std::path::PathBuf::from(format!("{} clone", dir.display()));
    let _ = std::fs::remove_dir_all(&clone);
    let result = (|| {
        git(&dir, &["commit", "-q", "--allow-empty", "-m", "two"])?;
        let url = format!("file://{}", dir.display().to_string().replace('\\', "/"));
        git(
            &dir,
            &[
                "clone",
                "-q",
                "--depth",
                "1",
                &url,
                &clone.to_string_lossy(),
            ],
        )?;
        let shallow = health::health(&clone, true)?;
        assert!(shallow.shallow);
        assert_eq!(health::suggestions(&shallow)[0].state, State::Unsupported);

        // A split chain written back into one file leaves its folder behind, empty.
        git(&dir, &["commit-graph", "write", "--reachable", "--split"])?;
        assert!(health::health(&dir, true)?.commit_graph);
        let info = dir.join(".git/objects/info");
        let _ = std::fs::remove_file(info.join("commit-graphs/commit-graph-chain"));
        for entry in std::fs::read_dir(info.join("commit-graphs")).map_err(|e| e.to_string())? {
            let _ = std::fs::remove_file(entry.map_err(|e| e.to_string())?.path());
        }
        let _ = std::fs::remove_file(info.join("commit-graph"));
        assert!(!health::health(&dir, true)?.commit_graph);

        // core.fsmonitor=true does nothing where git has no monitor of its own.
        git(&dir, &["config", "core.fsmonitor", "true"])?;
        assert_eq!(
            health::health(&dir, true)?.fsmonitor,
            Some(State::Unsupported)
        );
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&clone);
    let _ = std::fs::remove_dir_all(&dir);
    result
}
