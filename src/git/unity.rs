//! Unity projects in a repository: finding one, checking how it is set up for git (text
//! serialization, visible `.meta` files, `.gitattributes`, `.gitignore`), and the `.meta` file
//! every asset and folder under `Assets/` needs beside it.

use std::collections::HashSet;
use std::path::Path;

use super::{StatusFile, run};

/// A Unity project found in the repository.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnityProject {
    /// Its folder, from the repository's root: `""` when it is the root, else `Game/`
    /// (with the slash).
    pub root: String,
    /// The editor version it was last opened with (`2022.3.10f1`), when it says.
    pub version: Option<String>,
}

impl UnityProject {
    /// `rel` within the project, from the repository's root.
    pub fn path(&self, rel: &str) -> String {
        format!("{}{rel}", self.root)
    }
}

/// The Unity project of the repository, at its root or one folder down: the folder with
/// `ProjectSettings/ProjectVersion.txt` (tracked or not).
pub fn unity_project(repo: &Path) -> Result<Option<UnityProject>, String> {
    let out = run(
        Some(repo),
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            ":(glob)ProjectSettings/ProjectVersion.txt",
            ":(glob)*/ProjectSettings/ProjectVersion.txt",
        ],
    )?;
    let mut found: Vec<&str> = out.split('\0').filter(|p| !p.is_empty()).collect();
    // The shallowest, then by name: the root project wins over one in a folder.
    found.sort_by_key(|p| (p.matches('/').count(), *p));
    let Some(file) = found.first() else {
        return Ok(None);
    };
    let root = file
        .strip_suffix("ProjectSettings/ProjectVersion.txt")
        .unwrap_or("")
        .to_owned();
    let version = crate::remote::read_to_string(&crate::remote::join(repo, file))
        .ok()
        .and_then(|t| editor_version(&t));
    Ok(Some(UnityProject { root, version }))
}

/// `m_EditorVersion: 2022.3.10f1` from `ProjectVersion.txt`.
pub(crate) fn editor_version(text: &str) -> Option<String> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix("m_EditorVersion:"))
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// What `.gitattributes` should say for a Unity project: its YAML files merged by Unity's
/// Smart Merge with LF line ends, and its usual binaries in LFS.
pub const UNITY_YAML: &[&str] = &[
    "*.unity",
    "*.prefab",
    "*.asset",
    "*.meta",
    "*.mat",
    "*.anim",
    "*.controller",
    "*.overrideController",
    "*.physicMaterial",
    "*.mask",
    "*.playable",
    "*.lighting",
];

pub const UNITY_BINARY: &[&str] = &[
    "*.png",
    "*.jpg",
    "*.jpeg",
    "*.psd",
    "*.tga",
    "*.tif",
    "*.tiff",
    "*.exr",
    "*.hdr",
    "*.fbx",
    "*.obj",
    "*.blend",
    "*.wav",
    "*.mp3",
    "*.ogg",
    "*.mp4",
    "*.mov",
    "*.ttf",
    "*.otf",
    "*.dll",
    "*.so",
    "*.a",
    "*.unitypackage",
    "*.cubemap",
    "*.bank",
];

/// The `.gitattributes` lines for `pattern`.
fn attribute_line(pattern: &str) -> String {
    if UNITY_YAML.contains(&pattern) {
        format!("{pattern} merge=unityyamlmerge eol=lf")
    } else {
        format!("{pattern} filter=lfs diff=lfs merge=lfs -text")
    }
}

/// The folders Unity makes and rebuilds, which never go in git.
pub const UNITY_IGNORED: &[&str] = &[
    "Library",
    "Temp",
    "Obj",
    "Logs",
    "UserSettings",
    "MemoryCaptures",
];

/// How the project is set up for git.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitySetup {
    /// Assets are saved as text (`Force Text`), which git can diff and merge; `None` when the
    /// settings can't be read.
    pub force_text: Option<bool>,
    /// The editor's version-control mode ("Visible Meta Files", "Hidden Meta Files",
    /// "Perforce"…), when it says.
    pub vcs_mode: Option<String>,
    /// Unity's YAML types git doesn't merge with Smart Merge (`merge=unityyamlmerge`).
    pub missing_yaml: Vec<String>,
    /// Unity's usual binary types git doesn't keep in LFS.
    pub missing_lfs: Vec<String>,
    /// Folders Unity makes that git doesn't ignore.
    pub unignored: Vec<String>,
    /// Folders Unity makes that have files committed in them.
    pub tracked_generated: Vec<String>,
}

impl UnitySetup {
    /// `.meta` files are visible to git (Visible Meta Files; Perforce and Plastic modes keep
    /// them visible too).
    pub fn visible_meta(&self) -> Option<bool> {
        self.vcs_mode.as_deref().map(|m| !m.contains("Hidden"))
    }

    /// The first thing that really needs fixing for git, in words; `None` when nothing does.
    /// LFS is the repository's choice and doesn't count.
    pub fn needs_fixing(&self) -> Option<String> {
        if self.force_text == Some(false) {
            return Some(
                crate::i18n::text("backend-assets-aren-t-saved-as-text-bc57da").to_owned(),
            );
        }
        if self.visible_meta() == Some(false) {
            return Some(crate::i18n::text("backend-meta-files-are-hidden-2c8bdc").to_owned());
        }
        if let Some(d) = self.tracked_generated.first() {
            return Some(crate::i18n::message(
                "backend-d-has-files-committed-90143a",
                &[("d", (d).to_string().into())],
            ));
        }
        if let Some(d) = self
            .unignored
            .iter()
            .find(|d| matches!(d.as_str(), "Library" | "Temp"))
        {
            return Some(crate::i18n::message(
                "backend-d-isn-t-ignored-560779",
                &[("d", (d).to_string().into())],
            ));
        }
        if self.missing_yaml.len() == UNITY_YAML.len() {
            return Some(
                crate::i18n::text(
                    "backend-scenes-and-prefabs-aren-t-merged-with-smart-merge-79e82c",
                )
                .to_owned(),
            );
        }
        None
    }
}

/// Checks the project's settings, and what git does with its files (`git check-attr`,
/// `check-ignore`, `ls-files`, every attributes and ignore file counted).
pub fn unity_setup(repo: &Path, project: &UnityProject) -> Result<UnitySetup, String> {
    let read = |rel: &str| crate::remote::read_to_string(&crate::remote::join(repo, rel)).ok();
    let editor = read(&project.path("ProjectSettings/EditorSettings.asset"));
    let vcs = read(&project.path("ProjectSettings/VersionControlSettings.asset"));
    let force_text = editor.as_deref().and_then(serialization_is_text);
    let vcs_mode = vcs
        .as_deref()
        .and_then(|t| setting(t, "m_Mode"))
        .or_else(|| {
            editor
                .as_deref()
                .and_then(|t| setting(t, "m_ExternalVersionControlSupport"))
        })
        .map(str::to_owned);
    let (missing_yaml, missing_lfs) = missing_attributes(repo, project)?;
    Ok(UnitySetup {
        force_text,
        vcs_mode,
        missing_yaml,
        missing_lfs,
        unignored: unignored_folders(repo, project)?,
        tracked_generated: tracked_generated(repo, project)?,
    })
}

/// A `key: value` of a Unity settings file.
fn setting<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines()
        .find_map(|l| l.trim().strip_prefix(key)?.strip_prefix(':'))
        .map(str::trim)
}

/// `m_SerializationMode: 2` is Force Text (0 mixed, 1 binary).
pub(crate) fn serialization_is_text(editor_settings: &str) -> Option<bool> {
    setting(editor_settings, "m_SerializationMode").map(|v| v == "2")
}

/// The probe file of `pattern` (`*.png`) in the project's `Assets/`.
fn probe(project: &UnityProject, pattern: &str) -> String {
    project.path(&format!(
        "Assets/gitcraft-probe{}",
        pattern.trim_start_matches('*')
    ))
}

/// The YAML and binary patterns git gives no Smart Merge and no LFS, asked of git for a file
/// of each type.
fn missing_attributes(
    repo: &Path,
    project: &UnityProject,
) -> Result<(Vec<String>, Vec<String>), String> {
    let patterns: Vec<&str> = UNITY_YAML.iter().chain(UNITY_BINARY).copied().collect();
    let probes: Vec<String> = patterns.iter().map(|p| probe(project, p)).collect();
    let mut args = vec!["check-attr", "-z", "merge", "filter", "--"];
    args.extend(probes.iter().map(String::as_str));
    let out = super::run(Some(repo), &args)?;
    let attrs = parse_attrs(&out);
    let value = |probe: &str, attr: &str| {
        attrs
            .get(&(probe.to_owned(), attr.to_owned()))
            .cloned()
            .unwrap_or_default()
    };
    let mut yaml = Vec::new();
    let mut lfs = Vec::new();
    for (pattern, probe) in patterns.iter().zip(&probes) {
        if UNITY_YAML.contains(pattern) {
            if value(probe, "merge") != "unityyamlmerge" {
                yaml.push((*pattern).to_owned());
            }
        } else if value(probe, "filter") != "lfs" {
            lfs.push((*pattern).to_owned());
        }
    }
    Ok((yaml, lfs))
}

/// `git check-attr -z`: path, attribute, value, again and again.
pub(crate) fn parse_attrs(out: &str) -> std::collections::HashMap<(String, String), String> {
    out.split('\0')
        .collect::<Vec<_>>()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|[path, attr, value]| {
            (
                ((*path).to_owned(), (*attr).to_owned()),
                (*value).to_owned(),
            )
        })
        .collect()
}

/// The folders Unity makes that git would track (`git check-ignore --no-index` of a file in
/// each).
fn unignored_folders(repo: &Path, project: &UnityProject) -> Result<Vec<String>, String> {
    let probes: Vec<String> = UNITY_IGNORED
        .iter()
        .map(|d| project.path(&format!("{d}/gitcraft-probe")))
        .collect();
    // `-z` needs `--stdin`: one path a line, unquoted (`core.quotePath`; the probes have no
    // line ends).
    let mut args = vec![
        "-c",
        "core.quotePath=false",
        "check-ignore",
        "--no-index",
        "--",
    ];
    args.extend(probes.iter().map(String::as_str));
    // Exit 1: none of them is ignored.
    let out = super::run_ok_codes(Some(repo), &args, &[0, 1])?;
    let ignored: HashSet<&str> = out.lines().filter(|p| !p.is_empty()).collect();
    Ok(UNITY_IGNORED
        .iter()
        .zip(&probes)
        .filter(|(_, probe)| !ignored.contains(probe.as_str()))
        .map(|(d, _)| (*d).to_owned())
        .collect())
}

/// The folders Unity makes that have files in the index.
fn tracked_generated(repo: &Path, project: &UnityProject) -> Result<Vec<String>, String> {
    let specs: Vec<String> = UNITY_IGNORED
        .iter()
        .map(|d| format!(":(literal){}", project.path(&format!("{d}/"))))
        .collect();
    let mut args = vec!["ls-files", "-z", "--"];
    args.extend(specs.iter().map(String::as_str));
    let out = super::run(Some(repo), &args)?;
    let found: HashSet<&str> = out
        .split('\0')
        .filter_map(|p| p.strip_prefix(project.root.as_str())?.split('/').next())
        .collect();
    Ok(UNITY_IGNORED
        .iter()
        .filter(|d| found.contains(**d))
        .map(|d| (*d).to_owned())
        .collect())
}

/// Takes the folders `folders` of the project out of the index, keeping them on disk
/// (`git rm -r --cached`).
pub fn untrack_generated(
    repo: &Path,
    project: &UnityProject,
    folders: &[String],
) -> Result<(), String> {
    let paths: Vec<String> = folders.iter().map(|d| project.path(d)).collect();
    let mut args = vec!["--literal-pathspecs", "rm", "-r", "-q", "--cached", "--"];
    args.extend(paths.iter().map(String::as_str));
    super::run(Some(repo), &args).map(|_| ())
}

/// The lines to add to `.gitattributes` for `missing`, as a block of their own.
pub fn attributes_block(missing: &[String]) -> String {
    let mut block = String::from("\n# Unity (added by gitcraft)\n");
    for p in missing {
        block.push_str(&attribute_line(p));
        block.push('\n');
    }
    block
}

/// The lines to add to `.gitignore` for the folders Unity makes, under the project's folder.
pub fn ignore_block(project: &UnityProject, folders: &[String]) -> String {
    let mut block = String::from("\n# Unity (added by gitcraft)\n");
    for d in folders {
        let pattern: String = d
            .chars()
            .enumerate()
            .map(|(i, c)| {
                if i == 0 {
                    format!("[{}{}]", c.to_ascii_uppercase(), c.to_ascii_lowercase())
                } else {
                    c.to_string()
                }
            })
            .collect();
        block.push_str(&format!("/{}{pattern}/\n", escape_glob(&project.root)));
    }
    block
}

/// `path` with the characters of a pattern escaped, so that it matches only itself.
fn escape_glob(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Adds `block` at the end of the file `rel` of the repository (made when missing), with a line
/// end before it if the file doesn't end with one.
pub fn append_block(repo: &Path, rel: &str, block: &str) -> Result<(), String> {
    let file = crate::remote::join(repo, rel);
    let text = match crate::remote::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(crate::i18n::message(
                "backend-could-not-read-rel-e-22a177",
                &[
                    ("rel", (rel).to_string().into()),
                    ("detail", (e).to_string().into()),
                ],
            ));
        }
    };
    let crlf = text.contains("\r\n");
    let mut out = text.clone();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push_str(if crlf { "\r\n" } else { "\n" });
    }
    let block = if out.is_empty() {
        block.trim_start_matches('\n')
    } else {
        block
    };
    out.push_str(&if crlf {
        block.replace('\n', "\r\n")
    } else {
        block.to_owned()
    });
    crate::remote::write(&file, out.as_bytes()).map_err(|e| {
        crate::i18n::message(
            "backend-could-not-write-rel-e-38ec61",
            &[
                ("rel", (rel).to_string().into()),
                ("detail", (e).to_string().into()),
            ],
        )
    })
}

#[cfg(test)]
#[path = "unity_tests.rs"]
pub(crate) mod tests;

mod metadata;
#[allow(unused_imports)] // Compatibility re-export.
pub use metadata::IndexListing;
pub use metadata::MetaKind;
pub use metadata::MetaProblem;
pub use metadata::index_under;
pub use metadata::meta_problems;
#[allow(unused_imports)] // Compatibility re-export.
pub(crate) use metadata::skipped_by_unity;
