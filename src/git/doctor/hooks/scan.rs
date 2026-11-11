//! Hook file discovery, interpreter checks, and installation detection.

use super::*;

/// How much of each hook is read.
pub(super) const HEAD: usize = 512;

/// Files hook managers keep beside the hooks, which aren't hooks (husky's `h` and
/// `husky.sh`), and hidden files (husky's `.gitignore`).
pub(super) fn helper_file(name: &str) -> bool {
    name.starts_with('.') || matches!(name, "h" | "husky.sh")
}

/// Whether git looks at the executable bit where it runs: everywhere but Windows (Git for
/// Windows runs a hook through its `sh` whatever the bit).
pub(super) fn executable_bit_counts(repo: &Path) -> bool {
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
pub(super) fn is_shell(program: &str) -> bool {
    let name = program.rsplit('/').next().unwrap_or(program);
    matches!(name, "sh" | "bash" | "dash" | "zsh" | "ksh" | "ash")
}

/// A compiled program: ELF, Mach-O (both orders, universal) or a Windows executable.
pub(super) fn is_binary(head: &[u8]) -> bool {
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
pub(super) fn scan(location: &Path) -> Result<Option<Vec<Scanned>>, String> {
    match crate::remote::parse(location) {
        Some(t) => scan_there(&t),
        None => scan_here(location),
    }
}

pub(super) fn scan_here(dir: &Path) -> Result<Option<Vec<Scanned>>, String> {
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

pub(super) fn read_head(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut head = Vec::with_capacity(HEAD);
    std::fs::File::open(path)?
        .take(HEAD as u64)
        .read_to_end(&mut head)?;
    Ok(head)
}

#[cfg(unix)]
pub(super) fn is_executable(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    // The owner's bit: gitcraft runs git as the files' owner.
    meta.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
pub(super) fn is_executable(_meta: &std::fs::Metadata) -> bool {
    true
}

/// [`scan`] on another machine: one script lists every file, whether it is executable and
/// its first bytes in base64.
pub(super) fn scan_there(t: &crate::remote::Target) -> Result<Option<Vec<Scanned>>, String> {
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
                crate::git::display_error(String::from_utf8_lossy(&out.stderr).trim()).into(),
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
pub(super) fn installed(location: &Path, interpreters: &[String]) -> Vec<String> {
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

pub(super) fn installed_here(program: &str) -> bool {
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

pub(super) fn installed_there(t: &crate::remote::Target, wanted: &[&String]) -> Vec<String> {
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
