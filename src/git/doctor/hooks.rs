//! Hooks: the folder git runs them from (`core.hooksPath` or `.git/hooks`), which hooks are in
//! it and what runs each, what keeps one from running (not executable, a CRLF `#!` line, an
//! interpreter that isn't installed, a name git doesn't know), and the hook managers that set
//! them up (husky, lefthook, pre-commit). The folder is read in one go, here or on the
//! repository's machine.

use std::path::{Path, PathBuf};

use base64::Engine as _;

use crate::git::run;

/// The hooks git runs, and what runs each.
pub const KNOWN: &[(&str, &str)] = &[
    ("applypatch-msg", "git am"),
    ("pre-applypatch", "git am"),
    ("post-applypatch", "git am"),
    ("pre-commit", "git commit"),
    ("pre-merge-commit", "git merge, pull"),
    (
        "prepare-commit-msg",
        "git commit, merge, cherry-pick, revert, rebase",
    ),
    ("commit-msg", "git commit, merge"),
    ("post-commit", "git commit, cherry-pick, revert, rebase"),
    ("pre-rebase", "git rebase"),
    ("post-checkout", "git checkout, switch, clone, worktree add"),
    ("post-merge", "git merge, pull"),
    ("pre-push", "git push"),
    ("post-rewrite", "git commit --amend, rebase"),
    (
        "reference-transaction",
        "backend-every-change-of-a-ref-a1092b",
    ),
    ("pre-auto-gc", "git gc --auto"),
    (
        "post-index-change",
        "backend-every-write-of-the-index-51ed6f",
    ),
    ("sendemail-validate", "git send-email"),
    ("fsmonitor-watchman", "git status (core.fsmonitor)"),
    (
        "push-to-checkout",
        "backend-a-push-into-this-repository-576e18",
    ),
    (
        "pre-receive",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "update",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "proc-receive",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "post-receive",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    (
        "post-update",
        "backend-a-push-into-this-repository-server-side-c63be7",
    ),
    ("p4-changelist", "git p4 submit"),
    ("p4-prepare-changelist", "git p4 submit"),
    ("p4-post-changelist", "git p4 submit"),
    ("p4-pre-submit", "git p4 submit"),
];

/// What runs the hook `name`, `None` for a name git doesn't run.
pub fn runs_on(name: &str) -> Option<&'static str> {
    KNOWN
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, on)| crate::i18n::text(on))
}

/// Something that keeps a hook from running as meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Git skips it (with a hint) unless it is executable.
    NotExecutable,
    /// Its `#!` line ends in CR: the system looks for `sh\r`, which doesn't exist.
    CrlfShebang,
    /// It has CRLF line endings below its first line: shells read `\r` as part of commands.
    CrlfLines,
    /// No `#!` line: the system can't tell what runs it.
    NoShebang,
    /// The interpreter its `#!` line names isn't there.
    MissingInterpreter(String),
    /// Git runs no hook of that name (a typo, a backup such as `pre-commit.old`).
    UnknownName,
    /// A folder with a hook's name: git tries to run it, and the operation fails.
    IsFolder,
}

impl Problem {
    pub fn describe(&self) -> String {
        match self {
            Problem::NotExecutable => crate::i18n::text(
                "backend-not-executable-git-skips-it-prints-a-hint-and-goes-ahead-without-66d22e",
            )
            .to_owned(),
            Problem::IsFolder => crate::i18n::text(
                "backend-a-folder-with-a-hook-s-name-git-tries-to-run-it-and-the-operation-11b107",
            )
            .to_owned(),
            Problem::CrlfShebang => crate::i18n::text(
                "backend-its-line-ends-in-crlf-the-system-looks-for-an-interpreter-whose-n-74193c",
            )
            .to_owned(),
            Problem::CrlfLines => crate::i18n::text(
                "backend-crlf-line-endings-near-the-top-the-shell-reads-the-carriage-retur-dff8d9",
            )
            .to_owned(),
            Problem::NoShebang => crate::i18n::text(
                "backend-no-line-git-for-windows-can-t-tell-what-runs-it-start-it-with-bin-779e3f",
            )
            .to_owned(),
            Problem::MissingInterpreter(i) => crate::i18n::message(
                "backend-its-interpreter-i-isn-t-installed-where-git-runs-4d4fc2",
                &[("index", (i).to_string().into())],
            ),
            Problem::UnknownName => crate::i18n::text(
                "backend-git-runs-no-hook-of-this-name-it-never-runs-a-typo-or-a-copy-kept-3a6983",
            )
            .to_owned(),
        }
    }
}

/// A file in the hooks folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hook {
    pub name: String,
    /// Where it is when not in the hooks folder itself (`.husky/pre-commit`, which husky's
    /// stub of that name runs).
    pub file: Option<String>,
    /// What runs it, `None` for a name git doesn't know.
    pub runs_on: Option<&'static str>,
    /// The interpreter its `#!` line names (`sh`, `node`, `python3`…).
    pub interpreter: Option<String>,
    pub problems: Vec<Problem>,
}

/// A tool that manages the hooks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Manager {
    Husky,
    Lefthook,
    PreCommit,
}

impl Manager {
    pub fn label(self) -> &'static str {
        match self {
            Manager::Husky => "husky",
            Manager::Lefthook => "lefthook",
            Manager::PreCommit => "pre-commit",
        }
    }

    /// What to change rather than the hooks it writes.
    pub fn advice(self) -> &'static str {
        match self {
            Manager::Husky => crate::i18n::text(
                "backend-the-hooks-in-husky-are-husky-s-stubs-rewritten-on-install-edit-th-583641",
            ),
            Manager::Lefthook => crate::i18n::text(
                "backend-edit-lefthook-yml-lefthook-install-writes-the-hooks-again-9f7ab5",
            ),
            Manager::PreCommit => crate::i18n::text(
                "backend-edit-pre-commit-config-yaml-its-hooks-read-it-each-time-they-run-e85dff",
            ),
        }
    }
}

/// The repository's hooks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Hooks {
    /// The folder git runs them from, as git names it.
    pub folder: String,
    /// `core.hooksPath` when it is set (else the folder is `.git/hooks`).
    pub hooks_path: Option<String>,
    /// The folder doesn't exist (or isn't a folder).
    pub missing: bool,
    /// `core.hooksPath` is set to nothing: git runs no hook at all.
    pub disabled: bool,
    /// The files git runs as hooks (a hook's name), and the scripts husky's stubs run.
    pub active: Vec<Hook>,
    /// Other files of the folder (`pre-commit.old`, typos): never run.
    pub others: Vec<Hook>,
    /// How many `.sample` files there are (never run).
    pub samples: usize,
    pub managers: Vec<Manager>,
}

/// What is read of each file of the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub name: String,
    /// A folder (only listed for a hook's name).
    pub folder: bool,
    pub executable: bool,
    /// Its first bytes.
    pub head: Vec<u8>,
}

/// How much of each hook is read.
const HEAD: usize = 512;

/// The repository's hooks, with their problems.
pub fn hooks(repo: &Path) -> Result<Hooks, String> {
    let printed = run(Some(repo), &["rev-parse", "--git-path", "hooks"]).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-find-the-hooks-folder-e-acd76c",
            &[("detail", (e).to_string().into())],
        )
    })?;
    let folder = printed.trim_end_matches(['\n', '\r']).to_owned();
    // Unset (`None`) and set to nothing (hooks turned off) are told apart.
    let hooks_path = run(Some(repo), &["config", "--get", "core.hooksPath"])
        .ok()
        .map(|v| v.trim_end_matches(['\n', '\r']).to_owned());
    if hooks_path.as_deref() == Some("") {
        return Ok(Hooks {
            folder,
            hooks_path,
            disabled: true,
            ..Hooks::default()
        });
    }
    let location = crate::remote::resolve(repo, &folder);
    let scanned = scan(&location)?;
    let missing = scanned.is_none();
    let scanned = scanned.unwrap_or_default();
    let managers = managers(repo, &folder, hooks_path.as_deref(), &scanned);
    // husky's stubs (`.husky/_/pre-commit`) run `.husky/pre-commit`: those are checked too.
    let husky_scripts = if folder.trim_end_matches('/').ends_with(".husky/_") {
        let parent =
            crate::remote::resolve(repo, folder.trim_end_matches('/').trim_end_matches("/_"));
        scan(&parent)?.unwrap_or_default()
    } else {
        Vec::new()
    };
    let interpreters: Vec<String> = scanned
        .iter()
        .chain(&husky_scripts)
        .filter_map(|s| interpreter(&s.head))
        .collect();
    let found = installed(&location, &interpreters);
    let samples = scanned
        .iter()
        .filter(|s| s.name.ends_with(".sample"))
        .count();
    let exec_counts = executable_bit_counts(repo);
    let (active, others) = scanned
        .iter()
        .filter(|s| !s.name.ends_with(".sample") && !helper_file(&s.name))
        .map(|s| check(s, &found, exec_counts))
        .partition(|h| h.runs_on.is_some());
    let mut active: Vec<Hook> = active;
    let husky_dir = folder
        .trim_end_matches('/')
        .trim_end_matches("/_")
        .to_owned();
    for script in husky_scripts
        .iter()
        .filter(|s| runs_on(&s.name).is_some() && !s.folder)
    {
        // husky runs them with `sh -e`: neither the bit nor a #! line matters.
        let mut hook = check(script, &found, false);
        hook.problems.retain(|p| !matches!(p, Problem::NoShebang));
        hook.file = Some(format!("{husky_dir}/{}", script.name));
        active.push(hook);
    }
    Ok(Hooks {
        folder,
        hooks_path,
        missing,
        disabled: false,
        active,
        others,
        samples,
        managers,
    })
}

/// Files hook managers keep beside the hooks, which aren't hooks (husky's `h` and
/// `husky.sh`), and hidden files (husky's `.gitignore`).
fn helper_file(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "h" | "husky.sh")
}

/// Whether git looks at the executable bit where it runs: everywhere but Windows (Git for
/// Windows runs a hook through its `sh` whatever the bit).
fn executable_bit_counts(repo: &Path) -> bool {
    cfg!(unix) || crate::remote::is_remote(repo)
}

/// The problems of one scanned file. `exec_counts`: git looks at the executable bit where it
/// runs (everywhere but Git for Windows, which also needs a `#!` line to run a script).
pub fn check(s: &Scanned, installed: &[String], exec_counts: bool) -> Hook {
    let runs_on = runs_on(&s.name);
    let interpreter = interpreter(&s.head);
    let mut problems = Vec::new();
    if runs_on.is_none() {
        // Never run: nothing else about it matters.
        problems.push(Problem::UnknownName);
    } else if s.folder {
        problems.push(Problem::IsFolder);
    } else {
        if exec_counts && !s.executable {
            problems.push(Problem::NotExecutable);
        }
        let first_line_end = s.head.iter().position(|&b| b == b'\n');
        if s.head.starts_with(b"#!") {
            let shell = interpreter.as_deref().is_none_or(is_shell);
            if first_line_end.is_some_and(|end| end > 0 && s.head[end - 1] == b'\r') {
                problems.push(Problem::CrlfShebang);
            } else if shell && s.head.windows(2).any(|w| w == b"\r\n") {
                problems.push(Problem::CrlfLines);
            }
        } else if !exec_counts && !s.head.is_empty() && !is_binary(&s.head) {
            // Elsewhere git runs a script without one with /bin/sh.
            problems.push(Problem::NoShebang);
        }
        if let Some(i) = &interpreter
            && !installed.contains(i)
        {
            problems.push(Problem::MissingInterpreter(i.clone()));
        }
    }
    Hook {
        name: s.name.clone(),
        file: None,
        runs_on,
        interpreter,
        problems,
    }
}

/// A shell, which reads a carriage return as part of a command (`node`, `python` don't care).
fn is_shell(program: &str) -> bool {
    let name = program.rsplit('/').next().unwrap_or(program);
    matches!(name, "sh" | "bash" | "dash" | "zsh" | "ksh" | "ash")
}

/// A compiled program: ELF, Mach-O (both orders, universal) or a Windows executable.
fn is_binary(head: &[u8]) -> bool {
    head.starts_with(b"\x7fELF")
        || head.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
        || head.starts_with(&[0xfe, 0xed, 0xfa, 0xcf])
        || head.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
        || head.starts_with(b"MZ")
}

/// The program a `#!` line names: the one `env` is told to find (`#!/usr/bin/env node` →
/// `node`), or the path itself (`#!/bin/sh` → `/bin/sh`). CR at the end of the line is left
/// out (it is a problem of its own).
pub fn interpreter(head: &[u8]) -> Option<String> {
    let rest = head.strip_prefix(b"#!")?;
    let line = rest.split(|&b| b == b'\n').next()?;
    let line = String::from_utf8_lossy(line);
    let mut words = line.trim_end_matches('\r').split_whitespace();
    let program = words.next()?;
    if program.ends_with("/env") {
        // `env x`, `env -S x y`, `env -S VAR=1 x`, `env -u NAME x`.
        let mut skip_next = false;
        words
            .find(|w| {
                if std::mem::take(&mut skip_next) {
                    return false;
                }
                if matches!(*w, "-u" | "--unset" | "-C" | "--chdir") {
                    skip_next = true;
                    return false;
                }
                !w.starts_with('-') && !w.contains('=')
            })
            .map(str::to_owned)
    } else {
        Some(program.to_owned())
    }
}

/// The files of the folder at `location` (on whichever machine), `None` when it doesn't exist.
fn scan(location: &Path) -> Result<Option<Vec<Scanned>>, String> {
    match crate::remote::parse(location) {
        Some(t) => scan_there(&t),
        None => scan_here(location),
    }
}

fn scan_here(dir: &Path) -> Result<Option<Vec<Scanned>>, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        // Gone, or not a folder (`core.hooksPath=/dev/null`): no hook runs.
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        Err(e) => {
            return Err(crate::i18n::message(
                "backend-could-not-read-e-6dfcf3",
                &[
                    ("path", (dir.display()).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            ));
        }
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // Links are followed: git runs what they point to.
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        let name = entry.file_name().to_string_lossy().into_owned();
        let folder = meta.is_dir();
        // A folder matters only when git would try to run it.
        if !(meta.is_file() || folder && runs_on(&name).is_some()) {
            continue;
        }
        let head = if folder {
            Vec::new()
        } else {
            read_head(&path).unwrap_or_default()
        };
        out.push(Scanned {
            name,
            folder,
            executable: is_executable(&meta),
            head,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Some(out))
}

fn read_head(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut head = Vec::with_capacity(HEAD);
    std::fs::File::open(path)?
        .take(HEAD as u64)
        .read_to_end(&mut head)?;
    Ok(head)
}

#[cfg(unix)]
fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // The owner's bit: gitcraft runs git as the files' owner.
    meta.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

/// [`scan`] on another machine: one script lists every file, whether it is executable and
/// its first bytes in base64.
fn scan_there(t: &crate::remote::Target) -> Result<Option<Vec<Scanned>>, String> {
    let script = format!(
        concat!(
            "cd {} 2>/dev/null || exit 3\n",
            "command -v base64 >/dev/null 2>&1 || {{ echo 'base64 is missing' >&2; exit 4; }}\n",
            "for f in * .[!.]*; do\n",
            "  [ -e \"$f\" ] || continue\n",
            "  d=0; [ -d \"$f\" ] && d=1\n",
            "  x=0; [ -x \"$f\" ] && x=1\n",
            "  printf '%s\\n%s%s\\n' \"$(printf '%s' \"$f\" | base64 | tr -d '\\n')\" \"$d\" \"$x\"\n",
            "  if [ $d = 0 ]; then head -c {} < \"$f\" | base64 | tr -d '\\n'; fi; printf '\\n'\n",
            "done\n",
        ),
        crate::remote::sh_path(&t.path),
        HEAD
    );
    let out = crate::remote::exec(
        &t.host,
        &crate::remote::Request {
            script,
            ..Default::default()
        },
    )
    .map_err(|e| {
        crate::i18n::message(
            "backend-could-not-read-the-hooks-folder-e-4e4558",
            &[("detail", (e).to_string().into())],
        )
    })?;
    match out.code {
        0 => Ok(Some(parse_scan(&String::from_utf8_lossy(&out.stdout)))),
        3 => Ok(None),
        _ => Err(crate::i18n::message(
            "backend-could-not-read-the-hooks-folder-1444e9",
            &[(
                "from_utf8_lossy",
                (String::from_utf8_lossy(&out.stderr).trim())
                    .to_string()
                    .into(),
            )],
        )),
    }
}

/// Reads what [`scan_there`]'s script prints: per entry three lines, its name in base64, two
/// digits (a folder, executable), its first bytes in base64. Folders are kept only with a
/// hook's name.
pub fn parse_scan(out: &str) -> Vec<Scanned> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let lines: Vec<&str> = out.lines().collect();
    let mut list: Vec<Scanned> = lines
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|c| {
            let name = String::from_utf8(b64.decode(c[0]).ok()?).ok()?;
            let folder = c[1].starts_with('1');
            if folder && runs_on(&name).is_none() {
                return None;
            }
            Some(Scanned {
                name,
                folder,
                executable: c[1].ends_with('1'),
                head: b64.decode(c[2]).unwrap_or_default(),
            })
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    list
}

/// Which of `interpreters` exist where git runs for the folder at `location`.
fn installed(location: &Path, interpreters: &[String]) -> Vec<String> {
    let mut wanted: Vec<&String> = interpreters.iter().collect();
    wanted.sort();
    wanted.dedup();
    match crate::remote::parse(location) {
        Some(t) => installed_there(&t, &wanted),
        None => wanted
            .into_iter()
            .filter(|i| installed_here(i))
            .cloned()
            .collect(),
    }
}

fn installed_here(program: &str) -> bool {
    if program.starts_with('/') {
        // Git for Windows maps /bin/sh, /usr/bin/… onto its own: not checked there.
        return cfg!(windows) || Path::new(program).is_file();
    }
    // Git for Windows runs hooks with its own sh, bash and perl, which needn't be on PATH.
    if cfg!(windows) && matches!(program, "sh" | "bash" | "perl") {
        return true;
    }
    // The PATH git is run with (on macOS, the login shell's).
    let Some(paths) = crate::shell_path::path()
        .cloned()
        .or_else(|| std::env::var_os("PATH"))
    else {
        return false;
    };
    let names: Vec<String> = if cfg!(windows) {
        vec![format!("{program}.exe"), program.to_owned()]
    } else {
        vec![program.to_owned()]
    };
    std::env::split_paths(&paths)
        .any(|dir| names.iter().any(|n| PathBuf::from(&dir).join(n).is_file()))
}

fn installed_there(t: &crate::remote::Target, wanted: &[&String]) -> Vec<String> {
    if wanted.is_empty() {
        return Vec::new();
    }
    let checks: Vec<String> = wanted
        .iter()
        .map(|i| {
            let q = crate::remote::sh_quote(i);
            format!("command -v {q} >/dev/null 2>&1 && printf '%s\\n' {q}")
        })
        .collect();
    let script = format!("{}\nexit 0", checks.join("\n"));
    crate::remote::exec(
        &t.host,
        &crate::remote::Request {
            script,
            ..Default::default()
        },
    )
    .map(|out| {
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    })
    // Not known: none is reported missing.
    .unwrap_or_else(|_| wanted.iter().map(|s| (*s).clone()).collect())
}

/// Fixes a hook that isn't executable (`chmod +x`), on whichever machine it is.
pub fn make_executable(file: &Path) -> Result<(), String> {
    let failed = |e: String| {
        crate::i18n::message(
            "backend-could-not-make-executable-e-5be1af",
            &[
                ("path", (file.display()).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    };
    if crate::remote::try_is_link(file).map_err(|e| failed(e.to_string()))? {
        return Err(failed(
            crate::i18n::text("backend-it-is-a-symbolic-link-change-the-file-it-points-to-6ef20a")
                .to_owned(),
        ));
    }
    match crate::remote::parse(file) {
        Some(t) => {
            let out = crate::remote::exec(
                &t.host,
                &crate::remote::Request {
                    script: format!("exec chmod +x -- {}", crate::remote::sh_path(&t.path)),
                    ..Default::default()
                },
            )
            .map_err(failed)?;
            if out.code == 0 {
                Ok(())
            } else {
                Err(failed(
                    String::from_utf8_lossy(&out.stderr).trim().to_owned(),
                ))
            }
        }
        None => chmod_here(file).map_err(|e| failed(e.to_string())),
    }
}

#[cfg(unix)]
fn chmod_here(file: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(file)?.permissions().mode();
    // Executable for those who can read it.
    let exec = (mode & 0o444) >> 2;
    std::fs::set_permissions(file, std::fs::Permissions::from_mode(mode | exec))
}

#[cfg(not(unix))]
fn chmod_here(_file: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Rewrites a hook with LF line endings, keeping it executable (the file is written in place,
/// so its mode stays). Never through a symbolic link.
pub fn to_lf(file: &Path) -> Result<(), String> {
    let failed = |e: String| {
        crate::i18n::message(
            "backend-could-not-convert-e-08b705",
            &[
                ("path", (file.display()).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    };
    if crate::remote::try_is_link(file).map_err(|e| failed(e.to_string()))? {
        return Err(failed(
            crate::i18n::text("backend-it-is-a-symbolic-link-f4fd09").to_owned(),
        ));
    }
    let bytes = crate::remote::read(file).map_err(|e| failed(e.to_string()))?;
    let fixed = crlf_to_lf(&bytes);
    if fixed == bytes {
        return Ok(());
    }
    crate::remote::write(file, &fixed).map_err(|e| failed(e.to_string()))
}

/// `bytes` with every CRLF made LF (a lone CR is kept).
pub fn crlf_to_lf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// The hook managers that set the hooks up, from `core.hooksPath`, what the hooks say and
/// the managers' own files in the repository.
fn managers(
    repo: &Path,
    folder: &str,
    hooks_path: Option<&str>,
    scanned: &[Scanned],
) -> Vec<Manager> {
    let files = run(
        Some(repo),
        &[
            "ls-files",
            "-z",
            "-c",
            "-o",
            "--exclude-standard",
            "--",
            ".husky",
            "lefthook.yml",
            ".lefthook.yml",
            "lefthook.yaml",
            ".pre-commit-config.yaml",
        ],
    )
    .unwrap_or_default();
    detect_managers(folder, hooks_path, scanned, &files)
}

/// [`managers`] from what was read: `files` as `ls-files -z` listed them.
pub fn detect_managers(
    folder: &str,
    hooks_path: Option<&str>,
    scanned: &[Scanned],
    files: &str,
) -> Vec<Manager> {
    let listed: Vec<&str> = files.split('\0').filter(|f| !f.is_empty()).collect();
    let says = |word: &str| {
        scanned
            .iter()
            .any(|s| String::from_utf8_lossy(&s.head).contains(word))
    };
    let mut out = Vec::new();
    let husky_path = hooks_path.is_some_and(|p| p.contains(".husky")) || folder.contains(".husky");
    if husky_path || listed.iter().any(|f| f.starts_with(".husky/")) || says("husky") {
        out.push(Manager::Husky);
    }
    if listed.iter().any(|f| f.contains("lefthook.y")) || says("lefthook") {
        out.push(Manager::Lefthook);
    }
    if listed.contains(&".pre-commit-config.yaml") || says("File generated by pre-commit") {
        out.push(Manager::PreCommit);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scanned(name: &str, executable: bool, head: &[u8]) -> Scanned {
        Scanned {
            name: name.into(),
            folder: false,
            executable,
            head: head.to_vec(),
        }
    }

    #[test]
    fn interpreters_are_read_from_the_shebang() {
        assert_eq!(interpreter(b"#!/bin/sh\necho"), Some("/bin/sh".into()));
        assert_eq!(interpreter(b"#!/usr/bin/env node\n"), Some("node".into()));
        assert_eq!(
            interpreter(b"#!/usr/bin/env -S python3 -u\n"),
            Some("python3".into())
        );
        assert_eq!(interpreter(b"#! /bin/bash\r\n"), Some("/bin/bash".into()));
        assert_eq!(
            interpreter(b"#!/usr/bin/env -S VAR=1 node\n"),
            Some("node".into())
        );
        assert_eq!(
            interpreter(b"#!/usr/bin/env -u X node\n"),
            Some("node".into())
        );
        assert_eq!(interpreter(b"echo hi\n"), None);
        assert_eq!(interpreter(b"#!\n"), None);
        assert_eq!(interpreter(b""), None);
    }

    #[test]
    fn problems_are_found() {
        let ok = check(
            &scanned("pre-commit", true, b"#!/bin/sh\nexit 0\n"),
            &["/bin/sh".into()],
            true,
        );
        assert!(ok.problems.is_empty());
        assert_eq!(ok.runs_on, Some("git commit"));

        let crlf = check(
            &scanned("pre-push", true, b"#!/bin/sh\r\nexit 0\r\n"),
            &["/bin/sh".into()],
            true,
        );
        assert_eq!(crlf.problems, [Problem::CrlfShebang]);
        let lines = check(
            &scanned("pre-push", true, b"#!/bin/sh\nexit 0\r\n"),
            &["/bin/sh".into()],
            true,
        );
        assert_eq!(lines.problems, [Problem::CrlfLines]);

        let off = check(
            &scanned("commit-msg", false, b"#!/bin/sh\n"),
            &["/bin/sh".into()],
            true,
        );
        assert_eq!(off.problems, [Problem::NotExecutable]);
        // Where git doesn't look at the bit (Git for Windows), it isn't a problem.
        assert!(
            check(
                &scanned("commit-msg", false, b"#!/bin/sh\n"),
                &["/bin/sh".into()],
                false
            )
            .problems
            .is_empty()
        );

        let node = check(
            &scanned("pre-commit", true, b"#!/usr/bin/env node\n"),
            &[],
            true,
        );
        assert_eq!(node.problems, [Problem::MissingInterpreter("node".into())]);
        let old = check(
            &scanned("pre-commit.old", true, b"#!/bin/sh\n"),
            &["/bin/sh".into()],
            true,
        );
        assert_eq!(old.problems, [Problem::UnknownName]);
        // Without a #! line git runs it with /bin/sh; Git for Windows can't.
        let sh = check(&scanned("post-merge", true, b"echo hi\n"), &[], true);
        assert!(sh.problems.is_empty());
        let bare = check(&scanned("post-merge", true, b"echo hi\n"), &[], false);
        assert_eq!(bare.problems, [Problem::NoShebang]);
        // A compiled program needs none.
        let elf = check(&scanned("post-merge", true, b"\x7fELF\x02"), &[], false);
        assert!(elf.problems.is_empty());
        // CRLF below the #! line only matters to a shell.
        let node = check(
            &scanned("pre-push", true, b"#!/usr/bin/env node\nx\r\n"),
            &["node".into()],
            true,
        );
        assert!(node.problems.is_empty());
        // A name git never runs: only that is said, whatever else is wrong with it.
        let other = check(&scanned(".gitignore", false, b"*\r\n"), &[], true);
        assert_eq!(other.problems, [Problem::UnknownName]);
        let folder = Scanned {
            folder: true,
            ..scanned("pre-commit", true, b"")
        };
        assert_eq!(check(&folder, &[], true).problems, [Problem::IsFolder]);
    }

    #[test]
    fn the_remote_scan_is_read() {
        let b64 = base64::engine::general_purpose::STANDARD;
        let out = format!(
            "{}\n01\n{}\n{}\n00\n\n{}\n11\n\n{}\n11\n\nbroken\n",
            b64.encode("pré commit"),
            b64.encode(b"#!/bin/sh\n"),
            b64.encode("empty"),
            b64.encode("zz-folder"),
            b64.encode("pre-push"),
        );
        let list = parse_scan(&out);
        // The folder named as a hook is kept, the other one isn't.
        assert_eq!(list.len(), 3);
        assert!(list.iter().any(|s| s.name == "pre-push" && s.folder));
        let named = |n: &str| list.iter().find(|s| s.name == n);
        assert!(named("empty").is_some_and(|s| !s.executable && s.head.is_empty()));
        assert!(named("pré commit").is_some_and(|s| s.executable && s.head == b"#!/bin/sh\n"));
        assert!(parse_scan("").is_empty());
    }

    #[test]
    fn crlf_becomes_lf() {
        assert_eq!(
            crlf_to_lf(b"#!/bin/sh\r\nexit 0\r\n"),
            b"#!/bin/sh\nexit 0\n"
        );
        assert_eq!(crlf_to_lf(b"a\rb\n"), b"a\rb\n");
        assert_eq!(crlf_to_lf(b""), b"");
        assert_eq!(crlf_to_lf(b"\r"), b"\r");
    }

    #[test]
    fn managers_are_recognised() {
        let husky = detect_managers(".husky/_", Some(".husky/_"), &[], "");
        assert_eq!(husky, [Manager::Husky]);
        let lefthook = detect_managers(
            ".git/hooks",
            None,
            &[scanned("pre-commit", true, b"#!/bin/sh\n# lefthook\n")],
            "",
        );
        assert_eq!(lefthook, [Manager::Lefthook]);
        let pre = detect_managers(".git/hooks", None, &[], ".pre-commit-config.yaml\0");
        assert_eq!(pre, [Manager::PreCommit]);
        assert!(detect_managers(".git/hooks", None, &[], "").is_empty());
    }
}
