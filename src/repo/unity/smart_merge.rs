//! Unity's Smart Merge (UnityYAMLMerge): finding the tool of an installed editor, registering it
//! as the repository's merge driver for Unity's YAML files, and solving a conflicted file with
//! it in the conflict editor. The tool runs here, on copies of the file's three versions, so it
//! also solves files of a repository elsewhere; git is asked for the versions as for any other
//! call.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::git;

/// The repository's own config key for a UnityYAMLMerge picked by hand.
pub(super) const TOOL_KEY: &str = "gitcraft.unityYamlMerge";

/// The name of the merge driver `.gitattributes` names (`merge=unityyamlmerge`).
pub(super) const DRIVER: &str = "unityyamlmerge";

const EXE: &str = if cfg!(windows) {
    "UnityYAMLMerge.exe"
} else {
    "UnityYAMLMerge"
};

/// The extensions of Unity's YAML files, which Smart Merge understands.
pub(super) fn is_unity_yaml(path: &str) -> bool {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    git::UNITY_YAML
        .iter()
        .any(|p| p.strip_prefix("*.").map(str::to_ascii_lowercase) == ext)
}

/// The folders Unity Hub installs editors in, on this machine.
fn hub_folders() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if cfg!(windows) {
        for var in ["ProgramFiles", "ProgramW6432"] {
            if let Some(p) = std::env::var_os(var) {
                out.push(PathBuf::from(p).join("Unity").join("Hub").join("Editor"));
            }
        }
    } else if cfg!(target_os = "macos") {
        out.push(PathBuf::from("/Applications/Unity/Hub/Editor"));
    } else if let Some(home) = std::env::var_os("HOME") {
        out.push(PathBuf::from(home).join("Unity").join("Hub").join("Editor"));
    }
    out
}

/// Where an editor installed in `editor_dir` keeps UnityYAMLMerge.
fn tool_in(editor_dir: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        editor_dir.join("Unity.app/Contents/Tools").join(EXE)
    } else {
        editor_dir
            .join("Editor")
            .join("Data")
            .join("Tools")
            .join(EXE)
    }
}

/// UnityYAMLMerge: the one picked by hand (`configured`), else that of the installed editor of
/// the project's `version`, else of the newest editor installed.
pub(super) fn find_tool(version: Option<&str>, configured: Option<&str>) -> Option<PathBuf> {
    if let Some(c) = configured.map(PathBuf::from).filter(|p| p.is_file()) {
        return Some(c);
    }
    let mut editors: Vec<PathBuf> = hub_folders()
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.flatten().map(|e| e.path()))
        .collect();
    // Newest last: "2022.3.10f1" > "2021.3.1f1" as text is near enough, by its parts.
    editors.sort_by_key(|p| {
        version_key(
            &p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )
    });
    let named = |p: &&PathBuf| version.is_some_and(|v| p.file_name().is_some_and(|n| n == v));
    editors
        .iter()
        .find(named)
        .into_iter()
        .chain(editors.iter().rev())
        .map(|e| tool_in(e))
        .find(|t| t.is_file())
}

/// An editor version as numbers to sort by: `2022.3.10f1` as [2022, 3, 10, 1].
pub(super) fn version_key(v: &str) -> Vec<u64> {
    v.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect()
}

/// The tool as `sh` reads it in a git config value (git runs merge drivers with `sh`, Git for
/// Windows' own included): quoted, `/` between folders.
pub(super) fn sh_tool(tool: &Path) -> String {
    let p = tool.to_string_lossy().replace('\\', "/");
    crate::remote::sh_quote(&p)
}

/// Registers `tool` as this repository's merge driver for Unity's YAML files and as a merge
/// tool (`git mergetool --tool=unityyamlmerge`), in its own config.
pub(super) fn register(repo: &Path, tool: &Path) -> Result<(), String> {
    if crate::remote::is_remote(repo) {
        return Err(crate::i18n::text("repo-workflows-unity-register-b9569c8c").to_owned());
    }
    let t = sh_tool(tool);
    let settings = [
        (
            format!("merge.{DRIVER}.name"),
            "Unity Smart Merge (UnityYAMLMerge)".to_owned(),
        ),
        (
            format!("merge.{DRIVER}.driver"),
            format!("{t} merge -p --fallback none %O %B %A %A"),
        ),
        (format!("merge.{DRIVER}.recursive"), "binary".to_owned()),
        (
            format!("mergetool.{DRIVER}.cmd"),
            format!("{t} merge -p \"$BASE\" \"$REMOTE\" \"$LOCAL\" \"$MERGED\""),
        ),
        (
            format!("mergetool.{DRIVER}.trustExitCode"),
            "false".to_owned(),
        ),
    ];
    for (key, value) in settings {
        git::run(Some(repo), &["config", "--local", &key, &value])?;
    }
    Ok(())
}

/// The merge driver registered for Unity's YAML files, if any.
pub(super) fn registered(repo: &Path) -> Option<String> {
    git::run(
        Some(repo),
        &["config", "--get", &format!("merge.{DRIVER}.driver")],
    )
    .ok()
    .map(|v| v.trim().to_owned())
    .filter(|v| !v.is_empty())
}

/// How long UnityYAMLMerge may take before it is stopped.
const SOLVE_FOR: std::time::Duration = std::time::Duration::from_secs(120);

/// Runs `cmd` (no input, nothing read from its output but its errors), stopped after `limit`.
fn run_for(cmd: &mut Command, limit: std::time::Duration) -> Result<std::process::Output, String> {
    use std::process::Stdio;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            crate::i18n::message(
                "repo-workflows-unity-run-for-1f1e4005",
                &[("e", (e).to_string().into())],
            )
        })?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => {
                return child.wait_with_output().map_err(|e| {
                    crate::i18n::message(
                        "repo-workflows-unity-run-for-a1f34048",
                        &[("e", (e).to_string().into())],
                    )
                });
            }
            Ok(None) if started.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(crate::i18n::message(
                    "repo-workflows-unity-run-for-ca1ae15d",
                    &[("limit-as-secs-1", (limit.as_secs()).into())],
                ));
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
            Err(e) => {
                return Err(crate::i18n::message(
                    "repo-workflows-unity-run-for-a1f34048",
                    &[("e", (e).to_string().into())],
                ));
            }
        }
    }
}

/// UnityYAMLMerge's take on the conflicted `path`: its three versions (base, ours, theirs)
/// read from git into a folder of their own here, merged, and the result read back. The file
/// itself is left as it is. Blocking: from a thread of its own.
pub(super) fn solve(repo: &Path, path: &str, tool: &Path) -> Result<String, String> {
    let stage = |n: u8| git::run_bytes(Some(repo), &["show", &format!(":{n}:{path}")]);
    let ours = stage(2).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-unity-solve-6ee66918",
            &[("path", (path).into()), ("e", (e).to_string().into())],
        )
    })?;
    let theirs = stage(3).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-unity-solve-ac6f5e99",
            &[("path", (path).into()), ("e", (e).to_string().into())],
        )
    })?;
    // Added on both sides: no common version, an empty one.
    let base = stage(1).unwrap_or_default();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("gitcraft-unity-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| {
        crate::i18n::message(
            "repo-workflows-unity-solve-8c26b794",
            &[("path", (path).into()), ("e", (e).to_string().into())],
        )
    })?;
    let result = (|| {
        let name = path.rsplit('/').next().unwrap_or("file");
        let files =
            ["base", "theirs", "ours", "merged"].map(|side| dir.join(format!("{side}.{name}")));
        for (file, bytes) in files.iter().zip([&base, &theirs, &ours, &ours]) {
            std::fs::write(file, bytes).map_err(|e| {
                crate::i18n::message(
                    "repo-workflows-unity-solve-8c26b794",
                    &[("path", (path).into()), ("e", (e).to_string().into())],
                )
            })?;
        }
        let mut cmd = Command::new(tool);
        crate::shell_path::apply(&mut cmd);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        // No fallback tool: on what it can't merge it would open a merge window of its own.
        let out = run_for(
            cmd.args(["merge", "-p", "--fallback", "none"]).args(&files),
            SOLVE_FOR,
        )?;
        let merged = std::fs::read(&files[3]).map_err(|e| {
            crate::i18n::message(
                "repo-workflows-unity-solve-f176e899",
                &[("e", (e).to_string().into())],
            )
        })?;
        let text = String::from_utf8_lossy(&merged).into_owned();
        if out.status.success() || text.contains("<<<<<<<") {
            Ok(text)
        } else {
            let said = String::from_utf8_lossy(&out.stderr);
            let said = said.lines().rev().map(str::trim).find(|l| !l.is_empty());
            Err(match said {
                Some(line) => crate::i18n::message(
                    "repo-workflows-unity-solve-183ec676",
                    &[("line", (line).into())],
                ),
                None => crate::i18n::message(
                    "repo-workflows-unity-solve-ac1127b1",
                    &[
                        ("path", (path).into()),
                        ("out-status-1", (out.status).to_string().into()),
                    ],
                ),
            })
        }
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}
