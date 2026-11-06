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
