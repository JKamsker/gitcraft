//! Mergiraf (<https://mergiraf.org>), a merge tool that knows the syntax of many languages.
//! gitcraft runs the `mergiraf` command, the one on the PATH or one it downloaded from
//! Mergiraf's own releases when asked to; it never bundles or links it, so Mergiraf's GPL stays
//! with Mergiraf.
//!
//! Besides solving a file in the conflict editor, it can set git up to merge with Mergiraf in
//! every repository (`merge.mergiraf.*` in the global config, a marked block in the global
//! attributes file), and take that back.

mod download;
mod setup;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Other ways to install it.
pub const DOCS: &str = "https://mergiraf.org/installation.html";
const EXE: &str = if cfg!(windows) {
    "mergiraf.exe"
} else {
    "mergiraf"
};

/// How long an answer to "is it installed" holds: installing it needs no restart.
const RECHECK: Duration = Duration::from_secs(30);

/// A Mergiraf that runs.
#[derive(Clone, Debug)]
pub struct Found {
    pub program: PathBuf,
    /// Its version line ("mergiraf 0.20.0").
    pub version: String,
    /// The one gitcraft downloaded, not one on the PATH.
    pub own: bool,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub found: Option<Found>,
    /// Git merges with Mergiraf, in every repository.
    pub in_merges: bool,
}

/// Something gitcraft is doing with Mergiraf in the background, or the last thing it did.
#[derive(Clone, Debug)]
pub enum Job {
    Idle,
    Running(&'static str),
    Done(String),
    Failed(String),
}

static STATE: Mutex<Option<(Instant, State)>> = Mutex::new(None);
static JOB: Mutex<Job> = Mutex::new(Job::Idle);

fn quiet(mut cmd: Command) -> Command {
    crate::shell_path::apply(&mut cmd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// The folder gitcraft downloads Mergiraf into.
fn own_dir() -> Option<PathBuf> {
    eframe::storage_dir("gitcraft").map(|d| d.join("mergiraf"))
}

fn probe(program: &Path) -> Option<String> {
    quiet(Command::new(program))
        .arg("--version")
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The one on the PATH first: the user looks after that one.
fn find() -> Option<Found> {
    let on_path = PathBuf::from("mergiraf");
    if let Some(version) = probe(&on_path) {
        return Some(Found {
            program: on_path,
            version,
            own: false,
        });
    }
    let own = own_dir()?.join(EXE);
    let version = probe(&own)?;
    Some(Found {
        program: own,
        version,
        own: true,
    })
}

/// Whether Mergiraf is installed and git set up to use it, as of about [`RECHECK`] ago. Asked
/// every frame (the conflict editor, the settings): only the first answer is worked out here,
/// later ones are refreshed in the background, the old answer standing meanwhile, and `ctx`
/// repainted when the new one is in.
pub fn state(ctx: &egui::Context) -> State {
    // Remembered for any refresh, also one started where there is no UI (`solve`): the UI
    // may have shown the old answer while it ran.
    if let Ok(mut repaint) = REPAINT.lock()
        && repaint.is_none()
    {
        *repaint = Some(ctx.clone());
    }
    current()
}

/// The UI to repaint when a background refresh is in.
static REPAINT: Mutex<Option<egui::Context>> = Mutex::new(None);

/// [`state`], for callers without a UI.
fn current() -> State {
    let Ok(mut cache) = STATE.lock() else {
        return State::default();
    };
    match cache.as_ref() {
        Some((when, state)) => {
            if when.elapsed() >= RECHECK && !REFRESHING.swap(true, Ordering::AcqRel) {
                let generation = FORGOTTEN.load(Ordering::Acquire);
                crate::connections::spawn(move || {
                    let fresh = read_state();
                    // Not over what a `forget` meanwhile said is out of date.
                    if let Ok(mut cache) = STATE.lock()
                        && FORGOTTEN.load(Ordering::Acquire) == generation
                    {
                        *cache = Some((Instant::now(), fresh));
                    }
                    REFRESHING.store(false, Ordering::Release);
                    let repaint = REPAINT.lock().ok().and_then(|ctx| ctx.clone());
                    if let Some(ctx) = repaint {
                        ctx.request_repaint();
                    }
                });
            }
            state.clone()
        }
        None => {
            let state = read_state();
            *cache = Some((Instant::now(), state.clone()));
            state
        }
    }
}

/// A background refresh of [`STATE`] is running.
static REFRESHING: AtomicBool = AtomicBool::new(false);
/// How many times [`forget`] dropped the answer.
static FORGOTTEN: AtomicU32 = AtomicU32::new(0);

fn read_state() -> State {
    State {
        found: find(),
        in_merges: setup::in_merges(),
    }
}

/// The Mergiraf that runs, as [`state`] has it.
pub fn found(ctx: &egui::Context) -> Option<Found> {
    state(ctx).found
}

fn forget() {
    if let Ok(mut cache) = STATE.lock() {
        FORGOTTEN.fetch_add(1, Ordering::AcqRel);
        *cache = None;
    }
}

pub fn job() -> Job {
    JOB.lock().map(|j| j.clone()).unwrap_or(Job::Idle)
}

fn start(ctx: &egui::Context, doing: &'static str, work: fn() -> Result<String, String>) {
    let Ok(mut job) = JOB.lock() else { return };
    if matches!(*job, Job::Running(_)) {
        return;
    }
    *job = Job::Running(doing);
    let ctx = ctx.clone();
    crate::connections::spawn(move || {
        let result = work();
        forget();
        if let Ok(mut job) = JOB.lock() {
            *job = match result {
                Ok(done) => Job::Done(done),
                Err(e) => Job::Failed(e),
            };
        }
        ctx.request_repaint();
    });
}

/// Downloads the latest Mergiraf release for this system into gitcraft's folder.
pub fn install(ctx: &egui::Context) {
    start(ctx, "Downloading Mergiraf…", download::download);
}

/// Sets git up to merge with Mergiraf in every repository, or takes that back.
pub fn use_in_merges(ctx: &egui::Context, on: bool) {
    if on {
        start(ctx, "Setting git up…", setup::enable);
    } else {
        start(ctx, "Setting git back…", setup::disable);
    }
}

/// What Mergiraf makes of the conflicts in `path`, a file of the repository at `repo`: the file
/// with every conflict it could solve solved, the others still marked. The file itself is left
/// as it is.
pub fn solve(repo: &Path, path: &str) -> Result<String, String> {
    let found = current().found.ok_or("Mergiraf isn't installed")?;
    if crate::remote::is_remote(repo) {
        return solve_copy(&found.program, repo, path);
    }
    let out = quiet(Command::new(&found.program))
        .current_dir(repo)
        .args(["solve", "--stdout", "--", path])
        .output()
        .map_err(|e| format!("Could not run mergiraf: {e}"))?;
    solved(&out, path)
}

/// [`solve`] for a repository on another machine (WSL, SSH): Mergiraf runs here, on a copy of
/// the file in a folder of its own, under the file's own name (its language is told by it).
/// Blocking: from a thread of its own.
fn solve_copy(program: &Path, repo: &Path, path: &str) -> Result<String, String> {
    let text = crate::remote::read_to_string(&crate::remote::join(repo, path))
        .map_err(|e| format!("Could not read {path}: {e}"))?;
    solve_contents(program, path, &text)
}

/// Checks forecast conflict contents on a private copy; never reads or edits the working tree.
pub(crate) fn solve_text(
    path: &str,
    text: &str,
    active: &dyn Fn() -> bool,
) -> Result<String, String> {
    // Do not use current(): its manual-tool discovery may be probing a stuck binary while
    // holding STATE. Every automatic probe and solve has its own cancellable deadline.
    let candidates =
        std::iter::once(PathBuf::from("mergiraf")).chain(own_dir().map(|d| d.join(EXE)));
    for program in candidates {
        let mut probe = quiet(Command::new(&program));
        probe.arg("--version");
        if crate::bounded_process::run(probe, active, Duration::from_secs(2))
            .is_ok_and(|o| o.status.success())
        {
            return solve_contents_with(&program, path, text, Some(active));
        }
        if !active() {
            return Err("Forecast cancelled".into());
        }
    }
    Err("Mergiraf isn't installed or did not answer its version probe".into())
}

fn solve_contents(program: &Path, path: &str, text: &str) -> Result<String, String> {
    solve_contents_with(program, path, text, None)
}

fn solve_contents_with(
    program: &Path,
    path: &str,
    text: &str,
    active: Option<&dyn Fn() -> bool>,
) -> Result<String, String> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or("file");
    if name.is_empty() || matches!(name, "." | "..") || name.contains(':') {
        return Err(format!("Cannot copy {path} for Mergiraf"));
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir =
        std::env::temp_dir().join(format!("gitcraft-mergiraf-{}-{nanos}", std::process::id()));
    // A fresh private directory: another process cannot substitute the copy or its contents.
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder
        .create(&dir)
        .map_err(|e| format!("Could not copy {path}: {e}"))?;
    let result = std::fs::write(dir.join(name), text)
        .map_err(|e| format!("Could not copy {path}: {e}"))
        .and_then(|()| {
            let mut command = quiet(Command::new(program));
            command
                .current_dir(&dir)
                .args(["solve", "--stdout", "--", name]);
            match active {
                Some(active) => {
                    crate::bounded_process::run(command, active, Duration::from_secs(3))
                }
                None => command
                    .output()
                    .map_err(|e| format!("Could not run mergiraf: {e}")),
            }
        })
        .and_then(|out| solved(&out, path));
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// What Mergiraf printed: the file, solved as far as it could be.
fn solved(out: &std::process::Output, path: &str) -> Result<String, String> {
    // 0: all solved; 2: some conflicts remain, still a file to show.
    let solved = out.status.success() || (out.status.code() == Some(2) && !out.stdout.is_empty());
    if solved {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(error_message(&String::from_utf8_lossy(&out.stderr), path))
    }
}

/// Mergiraf's last log line ("WARN Mergiraf: Could not find a supported language for 'x'"),
/// without its level and name.
fn error_message(stderr: &str, path: &str) -> String {
    let line = stderr.lines().rev().map(str::trim).find(|l| !l.is_empty());
    match line {
        Some(line) => {
            let line = ["ERROR ", "WARN ", "INFO "]
                .iter()
                .find_map(|level| line.strip_prefix(level))
                .unwrap_or(line);
            format!(
                "Mergiraf: {}",
                line.strip_prefix("Mergiraf: ").unwrap_or(line)
            )
        }
        None => format!("Mergiraf could not solve {path}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn forecast_contents_are_checked_on_a_private_copy() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("gitcraft-mergiraf-test-{nanos}"));
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let program = dir.join("solver");
        let result = (|| -> Result<(), String> {
            std::fs::write(&program, "#!/bin/sh\ntest \"$1\" = solve && test \"$2\" = --stdout && test \"$3\" = -- && test \"$4\" = both.rs && cat -- \"$4\"\n")
                .map_err(|e| e.to_string())?;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            let text = "<<<<<<< ours\nfn a() {}\n=======\nfn b() {}\n>>>>>>> theirs\n";
            assert_eq!(solve_contents(&program, "src/both.rs", text)?, text);
            assert!(solve_contents(&program, "../..", text).is_err());
            assert!(solve_contents(&program, "C:outside", text).is_err());
            assert!(!dir.join("both.rs").exists());
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    #[cfg(unix)]
    #[test]
    fn stalled_forecast_solver_is_cancelled_and_has_a_deadline() -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("gitcraft-stalled-solver-{nanos}"));
        std::fs::create_dir(&dir).map_err(|e| e.to_string())?;
        let program = dir.join("solver");
        let marker = dir.join("started");
        let result = (|| -> Result<(), String> {
            std::fs::write(
                &program,
                format!(
                    "#!/bin/sh\ntouch {}\nsleep 30\n",
                    crate::remote::sh_quote(&marker.to_string_lossy())
                ),
            )
            .map_err(|e| e.to_string())?;
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
            let began = Instant::now();
            let active = || began.elapsed() < Duration::from_millis(200);
            assert!(
                solve_contents_with(&program, "file.rs", "conflict", Some(&active))
                    .is_err_and(|e| e.contains("cancelled"))
            );
            assert!(marker.exists());
            assert!(began.elapsed() < Duration::from_secs(2));
            let began = Instant::now();
            assert!(
                solve_contents_with(&program, "file.rs", "conflict", Some(&|| true))
                    .is_err_and(|e| e.contains("timed out"))
            );
            assert!(began.elapsed() < Duration::from_secs(5));
            Ok(())
        })();
        let _ = std::fs::remove_dir_all(&dir);
        result
    }

    /// A conflicted file in WSL is solved by Mergiraf here, from a copy (run with
    /// `GITCRAFT_TEST_MERGIRAF=<mergiraf program>` and `GITCRAFT_TEST_WSL=<distro>`; skipped
    /// otherwise).
    #[test]
    fn a_file_elsewhere_is_solved_from_a_copy() -> Result<(), String> {
        let (Ok(program), Ok(distro)) = (
            std::env::var("GITCRAFT_TEST_MERGIRAF"),
            std::env::var("GITCRAFT_TEST_WSL"),
        ) else {
            return Ok(());
        };
        if !cfg!(windows) {
            return Ok(());
        }
        let host = crate::remote::Host::Wsl(distro);
        let dir = format!("/tmp/gitcraft-mergiraf-{}", std::process::id());
        let conflicted = concat!(
            "fn main() {}\n",
            "<<<<<<< ours\n",
            "fn a() {}\n",
            "||||||| base\n",
            "=======\n",
            "fn b() {}\n",
            ">>>>>>> theirs\n",
        );
        let made = crate::remote::exec(
            &host,
            &crate::remote::Request {
                script: format!("mkdir -p {dir} && cat > {dir}/both.rs"),
                stdin: Some(conflicted.as_bytes().to_vec()),
                ..crate::remote::Request::default()
            },
        )?;
        assert_eq!(made.code, 0);
        let repo = crate::remote::encode(&host, &dir);
        let solved = solve_copy(Path::new(&program), &repo, "both.rs");
        let _ = crate::remote::exec(
            &host,
            &crate::remote::Request {
                script: format!("rm -rf {dir}"),
                ..crate::remote::Request::default()
            },
        );
        let solved = solved?;
        assert!(!solved.contains("<<<<<<<"), "{solved}");
        assert!(
            solved.contains("fn a()") && solved.contains("fn b()"),
            "{solved}"
        );
        Ok(())
    }

    #[test]
    fn errors_lose_their_log_level() {
        assert_eq!(
            error_message(
                "WARN Mergiraf: Could not find a supported language for 'notes.txt'\n",
                "notes.txt"
            ),
            "Mergiraf: Could not find a supported language for 'notes.txt'"
        );
        assert_eq!(error_message("ERROR boom", "f"), "Mergiraf: boom");
        assert_eq!(error_message("  \n", "f"), "Mergiraf could not solve f");
    }
}
