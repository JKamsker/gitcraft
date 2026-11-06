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
    ("pre-merge-commit", "git merge"),
    ("prepare-commit-msg", "git commit, merge"),
    ("commit-msg", "git commit, merge"),
    ("post-commit", "git commit"),
    ("pre-rebase", "git rebase"),
    ("post-checkout", "git checkout, switch, clone, worktree add"),
    ("post-merge", "git merge, pull"),
    ("pre-push", "git push"),
    ("post-rewrite", "git commit --amend, rebase"),
    ("reference-transaction", "every change of a ref"),
    ("pre-auto-gc", "git gc --auto"),
    ("post-index-change", "every write of the index"),
    ("sendemail-validate", "git send-email"),
    ("fsmonitor-watchman", "git status (core.fsmonitor)"),
    ("push-to-checkout", "a push into this repository"),
    ("pre-receive", "a push into this repository (server side)"),
    ("update", "a push into this repository (server side)"),
    ("proc-receive", "a push into this repository (server side)"),
    ("post-receive", "a push into this repository (server side)"),
    ("post-update", "a push into this repository (server side)"),
    ("p4-changelist", "git p4 submit"),
    ("p4-prepare-changelist", "git p4 submit"),
    ("p4-post-changelist", "git p4 submit"),
    ("p4-pre-submit", "git p4 submit"),
];

/// What runs the hook `name`, `None` for a name git doesn't run.
pub fn runs_on(name: &str) -> Option<&'static str> {
    KNOWN.iter().find(|(n, _)| *n == name).map(|(_, on)| *on)
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
}

impl Problem {
    pub fn describe(&self) -> String {
        match self {
            Problem::NotExecutable => {
                "Not executable: git skips it. Make it executable (chmod +x).".to_owned()
            }
            Problem::CrlfShebang => "Its #! line ends in CRLF: the system looks for an \
                                     interpreter whose name ends in a carriage return, and \
                                     the hook fails with \"bad interpreter\". Save it with LF \
                                     line endings."
                .to_owned(),
            Problem::CrlfLines => "CRLF line endings: shells read the carriage returns as part \
                                   of each command. Save it with LF line endings."
                .to_owned(),
            Problem::NoShebang => {
                "No #! line: it may not run at all. Start it with #!/bin/sh (or the interpreter it \
                 needs)."
                    .to_owned()
            }
            Problem::MissingInterpreter(i) => {
                format!("Its interpreter, {i}, isn't installed where git runs.")
            }
            Problem::UnknownName => {
                "Git runs no hook of this name: it never runs (a typo, or a copy kept aside)."
                    .to_owned()
            }
        }
    }
}

/// A file in the hooks folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hook {
    pub name: String,
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

    /// Where its own configuration is, which is what to change rather than the hooks.
    pub fn configured_in(self) -> &'static str {
        match self {
            Manager::Husky => ".husky/ (and package.json's prepare script)",
            Manager::Lefthook => "lefthook.yml",
            Manager::PreCommit => ".pre-commit-config.yaml",
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
    /// The folder doesn't exist.
    pub missing: bool,
    pub active: Vec<Hook>,
    /// How many `.sample` files there are (never run).
    pub samples: usize,
    pub managers: Vec<Manager>,
}

/// What is read of each file of the folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scanned {
    pub name: String,
    pub executable: bool,
    /// Its first bytes.
    pub head: Vec<u8>,
}

/// How much of each hook is read.
const HEAD: usize = 512;

/// The repository's hooks, with their problems.
pub fn hooks(repo: &Path) -> Result<Hooks, String> {
    let printed = run(Some(repo), &["rev-parse", "--git-path", "hooks"])
        .map_err(|e| format!("Could not find the hooks folder: {e}"))?;
    let folder = printed.trim_end_matches(['\n', '\r']).to_owned();
    let hooks_path = run(Some(repo), &["config", "--get", "core.hooksPath"])
        .ok()
        .map(|v| v.trim_end_matches(['\n', '\r']).to_owned())
        .filter(|v| !v.is_empty());
    let location = crate::remote::resolve(repo, &folder);
    let scanned = scan(&location)?;
    let missing = scanned.is_none();
    let scanned = scanned.unwrap_or_default();
    let interpreters: Vec<String> = scanned
        .iter()
        .filter_map(|s| interpreter(&s.head))
        .collect();
    let found = installed(&location, &interpreters);
    let samples = scanned
        .iter()
        .filter(|s| s.name.ends_with(".sample"))
        .count();
    let active = scanned
        .iter()
        .filter(|s| !s.name.ends_with(".sample"))
        .map(|s| check(s, &found, executable_bit_counts(repo)))
        .collect();
    let managers = managers(repo, &folder, hooks_path.as_deref(), &scanned);
    Ok(Hooks {
        folder,
        hooks_path,
        missing,
        active,
        samples,
        managers,
    })
}

/// Whether git looks at the executable bit where it runs: everywhere but Windows (Git for
/// Windows runs a hook through its `sh` whatever the bit).
fn executable_bit_counts(repo: &Path) -> bool {
    cfg!(unix) || crate::remote::is_remote(repo)
}

/// The problems of one scanned file.
pub fn check(s: &Scanned, installed: &[String], exec_counts: bool) -> Hook {
    let mut problems = Vec::new();
    let runs_on = runs_on(&s.name);
    if runs_on.is_none() {
        problems.push(Problem::UnknownName);
    }
    if exec_counts && !s.executable {
        problems.push(Problem::NotExecutable);
    }
    let first_line_end = s.head.iter().position(|&b| b == b'\n');
    let interpreter = interpreter(&s.head);
    if s.head.starts_with(b"#!") {
        if first_line_end.is_some_and(|end| end > 0 && s.head[end - 1] == b'\r') {
            problems.push(Problem::CrlfShebang);
        } else if s.head.windows(2).any(|w| w == b"\r\n") {
            problems.push(Problem::CrlfLines);
        }
    } else if !s.head.is_empty() {
        problems.push(Problem::NoShebang);
    }
    if let Some(i) = &interpreter
        && !installed.contains(i)
    {
        problems.push(Problem::MissingInterpreter(i.clone()));
    }
    Hook {
        name: s.name.clone(),
        runs_on,
        interpreter,
        problems,
    }
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
        // `env -S x y` and `env x`.
        words.find(|w| !w.starts_with('-')).map(str::to_owned)
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Could not read {}: {e}", dir.display())),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // Links are followed: git runs what they point to.
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let head = read_head(&path).unwrap_or_default();
        out.push(Scanned {
            name: entry.file_name().to_string_lossy().into_owned(),
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
    meta.permissions().mode() & 0o111 != 0
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
            "for f in * .[!.]*; do\n",
            "  [ -f \"$f\" ] || continue\n",
            "  x=0; [ -x \"$f\" ] && x=1\n",
            "  printf '%s\\n%s\\n' \"$(printf '%s' \"$f\" | base64 | tr -d '\\n')\" \"$x\"\n",
            "  head -c {} \"$f\" | base64 | tr -d '\\n'; printf '\\n'\n",
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
    .map_err(|e| format!("Could not read the hooks folder: {e}"))?;
    match out.code {
        0 => Ok(Some(parse_scan(&String::from_utf8_lossy(&out.stdout)))),
        3 => Ok(None),
        _ => Err(format!(
            "Could not read the hooks folder: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
    }
}

/// Reads what [`scan_there`]'s script prints: per file three lines, its name in base64,
/// `1` or `0` for executable, its first bytes in base64.
pub fn parse_scan(out: &str) -> Vec<Scanned> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let lines: Vec<&str> = out.lines().collect();
    let mut list: Vec<Scanned> = lines
        .chunks_exact(3)
        .filter_map(|c| {
            let name = String::from_utf8(b64.decode(c[0]).ok()?).ok()?;
            Some(Scanned {
                name,
                executable: c[1] == "1",
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
    let Some(paths) = std::env::var_os("PATH") else {
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
    let failed = |e: String| format!("Could not make {} executable: {e}", file.display());
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
    let failed = |e: String| format!("Could not convert {}: {e}", file.display());
    if crate::remote::try_is_link(file).map_err(|e| failed(e.to_string()))? {
        return Err(failed("it is a symbolic link".to_owned()));
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
        let bare = check(&scanned("post-merge", true, b"echo hi\n"), &[], true);
        assert_eq!(bare.problems, [Problem::NoShebang]);
    }

    #[test]
    fn the_remote_scan_is_read() {
        let b64 = base64::engine::general_purpose::STANDARD;
        let out = format!(
            "{}\n1\n{}\n{}\n0\n\nbroken\n",
            b64.encode("pré commit"),
            b64.encode(b"#!/bin/sh\n"),
            b64.encode("empty"),
        );
        let list = parse_scan(&out);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "empty");
        assert!(!list[0].executable && list[0].head.is_empty());
        assert_eq!(list[1].name, "pré commit");
        assert_eq!(list[1].head, b"#!/bin/sh\n");
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
