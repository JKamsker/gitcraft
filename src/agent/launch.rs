//! Starting a terminal in a new worktree that runs a script first (the setup recipe, then the
//! agent) and the user's shell after it, on the repository's machine: `sh` here (Git's own on
//! Windows), in the WSL distribution, or over SSH.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;

use super::{Recipe, sh};
use crate::remote::{self, Host};
use crate::terminal::ShellSpec;

/// What a new worktree's terminal runs before the shell.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Launch {
    /// The terminal tab's name.
    pub label: String,
    /// The worktree's folder, in the form the app gives paths.
    pub worktree: PathBuf,
    pub recipe: Recipe,
    /// The agent's command line, started once the recipe went through.
    pub agent: Option<String>,
    /// A script of gitcraft's own run after that (a bake-off's build), before the shell.
    pub run: Option<String>,
}

impl Launch {
    /// The whole script, `shell` (a command line) ending it.
    pub fn script(&self, shell: &str) -> String {
        let there = remote::git_path(&self.worktree);
        let mut s = String::from("[ -n \"$gitcraft_f\" ] && rm -f -- \"$gitcraft_f\"\n");
        s.push_str(&format!(
            "cd -- {} || {{ printf 'gitcraft: %s is not there\\n' {}; exec {shell}; }}\n",
            remote::sh_path(&there),
            sh::quote(&there),
        ));
        s.push_str(&self.recipe.script());
        if let Some(agent) = &self.agent {
            let program = agent.split_whitespace().next().unwrap_or(agent);
            let place = if remote::is_remote(&self.worktree) {
                "the repository's machine"
            } else {
                "this machine"
            };
            s.push_str(&format!(
                "if [ \"$gitcraft_failed\" != 0 ]; then\n  gitcraft_say {}\n\
                 elif ! command -v {} >/dev/null 2>&1; then\n  gitcraft_say {}\n\
                 else\n  gitcraft_say {}\n  {agent}\nfi\n",
                sh::quote(&format!(
                    "{} was not started; start it yourself with: {agent}",
                    self.label
                )),
                sh::quote(program),
                sh::quote(&format!(
                    "{program} isn't installed on {place} (not found on its PATH): install it, \
                     then start it with: {agent}"
                )),
                sh::quote(&format!("starting {}", self.label)),
            ));
        }
        if let Some(run) = &self.run {
            s.push_str(run);
            s.push('\n');
        }
        s.push_str(&format!("exec {shell}\n"));
        s
    }

    /// The program the terminal runs for it, and the folder it starts in. `local_shell` is the
    /// shell of this machine to end with (a repository elsewhere ends with the user's own
    /// shell there); `windows_sh` finds `sh` on Windows.
    pub fn program(
        &self,
        local_shell: &ShellSpec,
        windows_sh: impl FnOnce() -> Result<String, String>,
    ) -> Result<(ShellSpec, PathBuf), String> {
        let label = self.label.clone();
        let Some(target) = remote::parse(&self.worktree) else {
            let mut words = vec![sh::quote(&local_shell.program)];
            words.extend(local_shell.args.iter().map(|a| sh::quote(a)));
            let script = self.script(&words.join(" "));
            let spec = if cfg!(windows) {
                encoded(label, windows_sh()?, Vec::new(), &script)
            } else {
                ShellSpec {
                    label,
                    program: "/bin/sh".to_owned(),
                    args: vec!["-c".to_owned(), script],
                }
            };
            return Ok((spec, self.worktree.clone()));
        };
        let script = self.script("\"${SHELL:-sh}\" -l");
        let spec = match &target.host {
            Host::Wsl(distro) => encoded(
                label,
                "wsl.exe".to_owned(),
                ["-d", distro.as_str(), "-e", "sh"]
                    .map(str::to_owned)
                    .to_vec(),
                &script,
            ),
            Host::Ssh(dest) => ShellSpec {
                label,
                program: "ssh".to_owned(),
                args: vec![
                    "-t".to_owned(),
                    "--".to_owned(),
                    remote::ssh_destination(dest),
                    ssh_words(&script),
                ],
            },
        };
        Ok((spec, std::env::temp_dir()))
    }
}

/// What runs a script handed over in base64 (`$1`), with the terminal as its input: it is
/// written to a file there and read with `.`, and the file is removed as the script starts
/// (see [`Launch::script`]). Words no shell and no Windows command line reads specially: no
/// quote, backslash or `!`.
const DECODE: &str =
    "gitcraft_f=$(mktemp) && printf %s $1 | base64 -d > $gitcraft_f && . $gitcraft_f";

/// `program`, with `before` its first arguments, running `sh -c DECODE` on `script`.
fn encoded(label: String, program: String, before: Vec<String>, script: &str) -> ShellSpec {
    let mut args = before;
    args.extend(["-c", DECODE, "sh"].map(str::to_owned));
    args.push(base64::engine::general_purpose::STANDARD.encode(script));
    ShellSpec {
        label,
        program,
        args,
    }
}

/// What ssh is told to run: the user's login shell there reads it, which may be fish or csh,
/// so it is `exec sh -c '<decode>' sh <script in base64>`, words every shell reads alike.
fn ssh_words(script: &str) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(script);
    format!("exec sh -c '{DECODE}' sh {b64}")
}

/// The `sh` of Git for Windows, from where its git is (`git --exec-path`,
/// `…/Git/mingw64/libexec/git-core`): the candidates, likeliest first.
pub(crate) fn sh_beside_git(exec_path: &str) -> Vec<PathBuf> {
    let exec = Path::new(exec_path.trim());
    let mut out = Vec::new();
    // `bin\sh.exe` first: it puts Git's own tools (`mktemp`, `base64`) on the PATH, which
    // `usr\bin\sh.exe` run by itself doesn't.
    for up in [3, 2] {
        let mut root = exec;
        for _ in 0..up {
            root = root.parent().unwrap_or(root);
        }
        out.push(root.join("bin").join("sh.exe"));
        out.push(root.join("usr").join("bin").join("sh.exe"));
    }
    out
}

/// Git for Windows' `sh`, found once.
pub(crate) fn windows_sh() -> Result<String, String> {
    static FOUND: Mutex<Option<String>> = Mutex::new(None);
    if let Some(found) = FOUND.lock().ok().and_then(|f| f.clone()) {
        return Ok(found);
    }
    let exec = crate::git::run(None, &["--exec-path"])?;
    let sh = sh_beside_git(&exec)
        .into_iter()
        .find(|p| p.is_file())
        .map(|p| p.display().to_string())
        .ok_or_else(|| {
            "Git's sh.exe wasn't found beside git: the setup recipe and agents need Git for \
             Windows' sh"
                .to_owned()
        })?;
    if let Ok(mut found) = FOUND.lock() {
        *found = Some(sh.clone());
    }
    Ok(sh)
}

/// Launches waiting for the tab of their worktree to take them: a worktree opened right after
/// it is made gets a tab of its own, which starts its terminal there. One that no tab takes in
/// a while (the worktree couldn't be opened) is dropped, never started later by surprise.
pub(crate) struct PendingLaunches;

/// How long a launch waits for its tab.
const PENDING_FOR: Duration = Duration::from_secs(120);

/// Each with its worktree's folder in the form tabs have it, worked out when it was queued.
static PENDING: Mutex<Vec<(PathBuf, Instant, Launch)>> = Mutex::new(Vec::new());
/// Whether [`PENDING`] has any, so that a tab looks without taking the lock every frame.
static ANY: AtomicBool = AtomicBool::new(false);

impl PendingLaunches {
    /// Queues `launch` for the tab of its worktree. Resolves the folder here (through links):
    /// a tab's own folder is resolved so too, and is then only compared with, every frame.
    pub fn queue(launch: Launch) {
        let key = tab_form(&launch.worktree);
        if let Ok(mut list) = PENDING.lock() {
            list.retain(|(_, at, _)| at.elapsed() < PENDING_FOR);
            list.push((key, Instant::now(), launch));
            ANY.store(true, Ordering::Relaxed);
        }
    }

    /// The launch waiting for the worktree at `path` (a tab's folder), if one is.
    pub fn take(path: &Path) -> Option<Launch> {
        if !ANY.load(Ordering::Relaxed) {
            return None;
        }
        let mut list = PENDING.lock().ok()?;
        list.retain(|(_, at, _)| at.elapsed() < PENDING_FOR);
        let found = list
            .iter()
            .position(|(key, _, _)| same_written(key, path))
            .map(|i| list.remove(i).2);
        ANY.store(!list.is_empty(), Ordering::Relaxed);
        found
    }
}

/// `path` as a tab of it has it: resolved through links here (as the app opens folders),
/// as written for one elsewhere.
pub(crate) fn tab_form(path: &Path) -> PathBuf {
    if remote::is_remote(path) {
        return path.to_owned();
    }
    match std::fs::canonicalize(path) {
        Ok(real) => {
            let s = real.to_string_lossy();
            // Windows' `\\?\C:\…` as the app writes it, `C:\…`.
            match s.strip_prefix(r"\\?\") {
                Some(rest) if !rest.starts_with("UNC") => PathBuf::from(rest),
                _ => real,
            }
        }
        Err(_) => path.to_owned(),
    }
}

/// Whether two paths are written alike, but for a separator at the end.
fn same_written(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches(['/', '\\'])
        == b.to_string_lossy().trim_end_matches(['/', '\\'])
}

/// Finds Git's `sh` in the background ahead of the first launch, which then needn't ask git on
/// the window's thread. Only on Windows, the only place it is needed.
pub(crate) fn warm_up() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    if cfg!(windows) {
        ONCE.call_once(|| {
            std::thread::spawn(|| {
                let _ = windows_sh();
            });
        });
    }
}

#[cfg(all(test, unix))]
pub(super) fn decode_for_tests() -> &'static str {
    DECODE
}
