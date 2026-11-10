//! Reuse Git's graph output only while every input to the read is unchanged.
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Clone, PartialEq, Eq)]
struct State(Vec<Vec<u8>>);

struct Entry {
    path: PathBuf,
    args: Vec<String>,
    state: State,
    output: String,
}
static CACHE: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

fn snapshot(path: &Path) -> Option<State> {
    // Open afresh: config includes may change without changing the included file's name.
    let mut permissions = gix::open::Permissions::default();
    permissions.config.git_binary = true;
    let repo = gix::discover_opts(
        path,
        Default::default(),
        gix::open::Options::default()
            .permissions(permissions)
            .strict_config(true)
            .bail_if_untrusted(true),
    )
    .ok()?;
    super::commits::plain_history(&repo).ok()?;
    if std::fs::read_dir(repo.common_dir().join("worktrees"))
        .is_ok_and(|mut entries| entries.next().is_some())
    {
        return None;
    }
    let mut refs = Vec::new();
    for reference in repo.references().ok()?.all().ok()? {
        let reference = reference.ok()?;
        refs.push((
            reference.name().as_bstr().to_vec(),
            format!("{:?}", reference.target()).into_bytes(),
        ));
    }
    refs.sort();
    let mut state = vec![
        repo.config_snapshot().plumbing().to_bstring().to_vec(),
        std::fs::read(repo.git_dir().join("HEAD")).ok()?,
        jiff::tz::TimeZone::system()
            .iana_name()?
            .as_bytes()
            .to_vec(),
    ];
    for (name, target) in refs {
        state.extend([name, target]);
    }
    Some(State(state))
}

/// Only log_scoped's fixed format and ordinary ref roots are passed here.
pub(super) fn read(
    path: &Path,
    args: &[String],
    eligible: bool,
    read: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    let start = Instant::now();
    let before = if eligible && super::capability(path).is_ok() {
        snapshot(path)
    } else {
        None
    };
    if let Some(state) = &before
        && let Ok(cache) = CACHE.lock()
        && let Some(entry) = cache
            .iter()
            .find(|e| e.path == path && e.args == args && &e.state == state)
    {
        super::record("history-log-cache", true, false, None, start);
        return Ok(entry.output.clone());
    }
    let output = read()?;
    if let Some(state) = before
        && snapshot(path).as_ref() == Some(&state)
        && output.len() <= 8 * 1024 * 1024
        && let Ok(mut cache) = CACHE.lock()
    {
        cache.retain(|e| e.path != path || e.args != args);
        if cache.len() >= 4 {
            cache.remove(0);
        }
        cache.push(Entry {
            path: path.to_owned(),
            args: args.to_vec(),
            state,
            output: output.clone(),
        });
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::fixture::Repo;
    use std::cell::Cell;

    #[test]
    fn cached_log_is_gits_and_invalidates_on_refs_head_config_and_arguments() -> Result<(), String>
    {
        let repo = Repo::new("history-log-cache")?;
        repo.commit("first\n\nBody", &[("a", Some("one\n"))])?;
        let args = vec![
            "log".to_owned(),
            "--format=%H%x00%D%x00%s%x00%b".to_owned(),
            "--all".to_owned(),
        ];
        let calls = Cell::new(0);
        let cli = |args: &[String]| -> Result<String, String> {
            calls.set(calls.get() + 1);
            let out = std::process::Command::new("git")
                .current_dir(repo.path())
                .args(args)
                .envs(crate::git::command::GIT_ENV)
                .output()
                .map_err(|e| e.to_string())?;
            assert!(out.status.success());
            String::from_utf8(out.stdout).map_err(|e| e.to_string())
        };
        let first = read(repo.path(), &args, true, || cli(&args))?;
        assert_eq!(first, read(repo.path(), &args, true, || cli(&args))?);
        assert_eq!(calls.get(), 1);
        for command in [
            vec!["tag", "v1"],
            vec!["switch", "-q", "-c", "other"],
            vec!["config", "log.excludeDecoration", "refs/tags/*"],
            vec!["tag", "-d", "v1"],
        ] {
            repo.git(&command)?;
            let before = calls.get();
            assert_eq!(read(repo.path(), &args, true, || cli(&args))?, cli(&args)?);
            assert_eq!(calls.get(), before + 2);
        }
        let mut paged = args.clone();
        paged.push("--max-count=0".to_owned());
        assert_eq!(read(repo.path(), &paged, true, || cli(&paged))?, "");
        // A ref move during the read must not publish an answer under the old snapshot.
        let mut changing = args.clone();
        changing.push("--skip=0".to_owned());
        read(repo.path(), &changing, true, || {
            repo.git(&["branch", "new"])?;
            cli(&changing)
        })?;
        let before = calls.get();
        read(repo.path(), &changing, true, || cli(&changing))?;
        assert_eq!(calls.get(), before + 1);
        let before = calls.get();
        read(repo.path(), &args, false, || cli(&args))?;
        read(repo.path(), &args, false, || cli(&args))?;
        assert_eq!(calls.get(), before + 2);
        Ok(())
    }

    #[test]
    fn orders_pages_and_decorations_keep_gits_bytes() -> Result<(), String> {
        let repo = Repo::new("history-cache-orders")?;
        repo.commit("first\n\nBody paragraph", &[("a", Some("one\n"))])?;
        repo.git(&["switch", "-q", "-c", "side"])?;
        repo.commit("side", &[("b", Some("two\n"))])?;
        repo.git(&["switch", "-q", "main"])?;
        repo.commit("main", &[("c", Some("three\n"))])?;
        repo.git(&["merge", "-q", "--no-ff", "--no-edit", "side"])?;
        repo.git(&["tag", "-a", "v1", "-m", "release"])?;
        for order in ["--date-order", "--topo-order", "--author-date-order"] {
            for skip in ["--skip=0", "--skip=1"] {
                let args: Vec<String> = [
                    "log",
                    "--no-show-signature",
                    "--encoding=UTF-8",
                    order,
                    skip,
                    "--max-count=3",
                    "--date=format-local:%Y-%m-%d %H:%M",
                    "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%ad%x00%s%x00%D%x00%T%x00%b%x00",
                    "--exclude=refs/tags/*",
                    "--all",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect();
                let cli = || {
                    crate::git::run(
                        Some(repo.path()),
                        &args.iter().map(String::as_str).collect::<Vec<_>>(),
                    )
                };
                let expected = cli()?;
                assert_eq!(read(repo.path(), &args, true, cli)?, expected);
                assert_eq!(
                    read(repo.path(), &args, true, || Err("cache missed".to_owned()))?,
                    expected
                );
            }
        }
        Ok(())
    }

    #[test]
    #[ignore = "set GITCRAFT_PARITY_REPO to measure an existing repository"]
    fn graph_cache_measurement() -> Result<(), String> {
        let path = std::env::var_os("GITCRAFT_PARITY_REPO").ok_or("repository to measure")?;
        let path = Path::new(&path);
        let args: Vec<String> = [
            "log",
            "--no-show-signature",
            "--encoding=UTF-8",
            "--date-order",
            "--date=format-local:%Y-%m-%d %H:%M",
            "--skip=0",
            "--max-count=1500",
            "--format=%H%x00%P%x00%an%x00%ae%x00%at%x00%ad%x00%s%x00%D%x00%T%x00%b%x00",
            "--exclude=refs/stash",
            "--exclude=refs/gitcraft/*",
            "--exclude=refs/notes/*",
            "--exclude=refs/prefetch/*",
            "--exclude=refs/t3/*",
            "--all",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let calls = Cell::new(0);
        let cli = || -> Result<String, String> {
            calls.set(calls.get() + 1);
            let out = std::process::Command::new("git")
                .current_dir(path)
                .args(&args)
                .envs(crate::git::command::GIT_ENV)
                .output()
                .map_err(|e| e.to_string())?;
            assert!(out.status.success());
            String::from_utf8(out.stdout).map_err(|e| e.to_string())
        };
        let start = Instant::now();
        let cold = read(path, &args, true, cli)?;
        let cold_ms = start.elapsed();
        let start = Instant::now();
        let warm = read(path, &args, true, cli)?;
        let warm_ms = start.elapsed();
        assert_eq!(cold, warm);
        assert_eq!(calls.get(), 1, "the second read must start no Git process");
        assert_eq!(warm, cli()?);
        println!("cold {cold_ms:?}; warm {warm_ms:?}; byte parity; one subprocess for two reads");
        Ok(())
    }
}
