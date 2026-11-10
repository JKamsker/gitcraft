use super::*;

/// The scratch folder script, run here by `sh`: it makes a private folder, removes it, and
/// refuses to remove anything outside gitcraft's own folder.
#[test]
fn a_scratch_folder_is_made_and_only_it_is_removed() -> Result<(), String> {
    let sh = |script: String| {
        std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .map_err(|e| e.to_string())
    };
    let made = sh(scratch_script(None))?;
    let path = String::from_utf8_lossy(&made.stdout).trim().to_owned();
    assert!(made.status.success() && Path::new(&path).is_dir(), "{path}");
    let outside = std::env::temp_dir().join(format!("gitcraft-keep-{}", std::process::id()));
    std::fs::create_dir_all(&outside).map_err(|e| e.to_string())?;
    let refused = sh(scratch_script(Some(&outside.display().to_string())))?;
    let escaped = sh(scratch_script(Some(&format!("{path}/../.."))))?;
    let removed = sh(scratch_script(Some(&path)))?;
    let kept = outside.is_dir();
    let _ = std::fs::remove_dir_all(&outside);
    assert!(!refused.status.success() && !escaped.status.success() && kept);
    assert!(removed.status.success() && !Path::new(&path).exists());
    Ok(())
}

/// The script run on another machine, run here by `sh` the same way: a linked folder, a
/// missing file, a file too large and a link are refused; a plain file is read.
#[test]
fn a_file_there_is_read_only_when_plain_inside_and_small() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("gitcraft-within-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("docs")).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("docs/s.png"), "picture").map_err(|e| e.to_string())?;
    std::os::unix::fs::symlink("/", dir.join("docs/l")).map_err(|e| e.to_string())?;
    let repo = dir.display().to_string();
    std::os::unix::fs::symlink("s.png", dir.join("docs/link.png")).map_err(|e| e.to_string())?;
    let read = |rel: &str, max: u64| {
        let script = read_within_script(&repo, rel, max).unwrap_or_default();
        std::process::Command::new("sh")
            .args(["-c", &script])
            .output()
            .ok()
            .filter(|out| out.status.success() && out.stdout.len() as u64 <= max)
            .map(|out| out.stdout)
    };
    let found = (
        read("docs/s.png", 100),
        read("docs/l/etc/hostname", 1 << 20),
        read("docs/none.png", 100),
        read("docs/s.png", 3),
        read("docs/link.png", 100),
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(found, (Some(b"picture".to_vec()), None, None, None, None));
    assert!(read_within_script(&repo, "", 1).is_none());
    assert!(read_within_script(&repo, "docs/../x", 1).is_none());
    Ok(())
}

/// A folder swapped for a link while the script reads: what is outside is never read,
/// whichever moment the swap falls on.
#[test]
fn a_folder_swapped_for_a_link_meanwhile_is_never_read_there() -> Result<(), String> {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let base = std::env::temp_dir().join(format!("gitcraft-within-swap-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (dir, outside) = (base.join("repo"), base.join("outside"));
    for d in [dir.join("docs"), outside.join("docs")] {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(dir.join("docs/s.png"), "picture").map_err(|e| e.to_string())?;
    std::fs::write(outside.join("docs/s.png"), "secret").map_err(|e| e.to_string())?;
    let stop = Arc::new(AtomicBool::new(false));
    let swaps = Arc::new(AtomicUsize::new(0));
    let swapper = {
        let (dir, outside) = (dir.clone(), outside.clone());
        let (stop, swaps) = (stop.clone(), swaps.clone());
        std::thread::spawn(move || {
            let (real, moved) = (dir.join("docs"), dir.join("docs-real"));
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::rename(&real, &moved);
                let _ = std::os::unix::fs::symlink(outside.join("docs"), &real);
                let _ = std::fs::remove_file(&real);
                let _ = std::fs::rename(&moved, &real);
                swaps.fetch_add(1, Ordering::Relaxed);
            }
        })
    };
    let script =
        read_within_script(&dir.display().to_string(), "docs/s.png", 100).unwrap_or_default();
    let (mut leaked, mut reads) = (0, 0);
    while reads < 300 || swaps.load(Ordering::Relaxed) < 300 {
        let out = std::process::Command::new("sh")
            .args(["-c", &script])
            .output();
        if out.is_ok_and(|o| o.status.success() && o.stdout == b"secret") {
            leaked += 1;
        }
        reads += 1;
    }
    stop.store(true, Ordering::Relaxed);
    let _ = swapper.join();
    let _ = std::fs::remove_dir_all(&base);
    assert_eq!(leaked, 0);
    Ok(())
}
