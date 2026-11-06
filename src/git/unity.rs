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
    "Build",
    "Builds",
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
    /// `.meta` files are visible, so they are committed with their assets.
    pub visible_meta: Option<bool>,
    /// Recommended `.gitattributes` patterns that are missing.
    pub missing_attributes: Vec<String>,
    /// Folders Unity makes that git doesn't ignore.
    pub unignored: Vec<String>,
}

impl UnitySetup {
    pub fn all_good(&self) -> bool {
        self.force_text != Some(false)
            && self.visible_meta != Some(false)
            && self.missing_attributes.is_empty()
            && self.unignored.is_empty()
    }
}

/// Checks the project's settings and the repository's `.gitattributes` and `.gitignore`.
pub fn unity_setup(repo: &Path, project: &UnityProject) -> Result<UnitySetup, String> {
    let read = |rel: &str| crate::remote::read_to_string(&crate::remote::join(repo, rel)).ok();
    let editor = read(&project.path("ProjectSettings/EditorSettings.asset"));
    let vcs = read(&project.path("ProjectSettings/VersionControlSettings.asset"));
    let force_text = editor.as_deref().and_then(serialization_is_text);
    let visible_meta = vcs
        .as_deref()
        .and_then(|t| setting(t, "m_Mode"))
        .or_else(|| {
            editor
                .as_deref()
                .and_then(|t| setting(t, "m_ExternalVersionControlSupport"))
        })
        .map(|mode| mode.contains("Visible Meta Files"));
    let attributes = read(".gitattributes").unwrap_or_default();
    let missing_attributes = missing_patterns(&attributes);
    let unignored = unignored_folders(repo, project)?;
    Ok(UnitySetup {
        force_text,
        visible_meta,
        missing_attributes,
        unignored,
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

/// The recommended patterns `.gitattributes` has no line for.
pub(crate) fn missing_patterns(attributes: &str) -> Vec<String> {
    let present: HashSet<&str> = attributes
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_whitespace().next())
        .collect();
    UNITY_YAML
        .iter()
        .chain(UNITY_BINARY)
        .filter(|p| !present.contains(**p))
        .map(|p| (*p).to_owned())
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
        block.push_str(&format!("/{}{pattern}/\n", project.root));
    }
    block
}

/// Adds `block` at the end of the file `rel` of the repository (made when missing), with a line
/// end before it if the file doesn't end with one.
pub fn append_block(repo: &Path, rel: &str, block: &str) -> Result<(), String> {
    let file = crate::remote::join(repo, rel);
    let text = match crate::remote::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("Could not read {rel}: {e}")),
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
    crate::remote::write(&file, out.as_bytes()).map_err(|e| format!("Could not write {rel}: {e}"))
}

/// An asset or folder staged without its `.meta`, or a `.meta` without its asset.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaProblem {
    /// The asset (or `.meta`) staged.
    pub path: String,
    /// The `.meta` (or asset) that should go with it.
    pub partner: String,
    pub kind: MetaKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MetaKind {
    /// Added, its `.meta` neither committed nor staged.
    AddedWithoutMeta,
    /// Deleted, its `.meta` still there.
    DeletedWithoutMeta,
    /// A `.meta` added with no asset beside it.
    MetaWithoutAsset,
    /// A `.meta` deleted while its asset stays.
    MetaDeletedAssetKept,
}

/// What is wrong with the `.meta` pairs of the staged changes under the project's `Assets/`;
/// `tracked` says whether a path is in the index (staged or committed).
pub fn meta_problems(
    project: &UnityProject,
    staged: &[StatusFile],
    tracked: &dyn Fn(&str) -> bool,
) -> Vec<MetaProblem> {
    let assets = project.path("Assets/");
    let added: HashSet<&str> = staged
        .iter()
        .filter(|f| matches!(f.code, 'A' | 'R' | 'C'))
        .map(|f| f.path.as_str())
        .collect();
    let deleted: HashSet<&str> = staged
        .iter()
        .filter(|f| f.code == 'D')
        .map(|f| f.path.as_str())
        .chain(
            staged
                .iter()
                .filter_map(|f| (f.code == 'R').then_some(f.old_path.as_deref()).flatten()),
        )
        .collect();
    let mut problems = Vec::new();
    let mut folders_seen = HashSet::new();
    for path in added.iter().copied().filter(|p| p.starts_with(&assets)) {
        let push = |problems: &mut Vec<MetaProblem>, path: &str, partner: String, kind| {
            problems.push(MetaProblem {
                path: path.to_owned(),
                partner,
                kind,
            });
        };
        if let Some(asset) = path.strip_suffix(".meta") {
            if !added.contains(asset) && !tracked(asset) && !is_folder_meta(asset, &added, tracked)
            {
                push(
                    &mut problems,
                    path,
                    asset.to_owned(),
                    MetaKind::MetaWithoutAsset,
                );
            }
            continue;
        }
        let meta = format!("{path}.meta");
        if !added.contains(meta.as_str()) && !tracked(&meta) {
            push(&mut problems, path, meta, MetaKind::AddedWithoutMeta);
        }
        // Each folder between Assets/ and the file has a .meta of its own.
        let mut dir = path;
        while let Some((parent, _)) = dir.rsplit_once('/') {
            if parent.len() < assets.len() || !folders_seen.insert(parent.to_owned()) {
                break;
            }
            let meta = format!("{parent}.meta");
            if !added.contains(meta.as_str()) && !tracked(&meta) {
                push(&mut problems, parent, meta, MetaKind::AddedWithoutMeta);
            }
            dir = parent;
        }
    }
    for path in deleted.iter().copied().filter(|p| p.starts_with(&assets)) {
        if let Some(asset) = path.strip_suffix(".meta") {
            if !deleted.contains(asset) && tracked(asset) && !added.contains(path) {
                problems.push(MetaProblem {
                    path: path.to_owned(),
                    partner: asset.to_owned(),
                    kind: MetaKind::MetaDeletedAssetKept,
                });
            }
        } else {
            let meta = format!("{path}.meta");
            if !deleted.contains(meta.as_str()) && tracked(&meta) {
                problems.push(MetaProblem {
                    path: path.to_owned(),
                    partner: meta,
                    kind: MetaKind::DeletedWithoutMeta,
                });
            }
        }
    }
    problems.sort_by(|a, b| a.path.cmp(&b.path));
    problems
}

/// Whether `asset` (the name of a `.meta` without its extension) is a folder: something staged
/// or tracked is inside it.
fn is_folder_meta(asset: &str, added: &HashSet<&str>, tracked: &dyn Fn(&str) -> bool) -> bool {
    let inside = format!("{asset}/");
    added.iter().any(|p| p.starts_with(&inside)) || tracked(&inside)
}

/// The paths [`meta_problems`] asks `tracked` about, for [`in_index`] to answer in one go.
pub fn meta_candidates(project: &UnityProject, staged: &[StatusFile]) -> Vec<String> {
    let assets = project.path("Assets/");
    let mut out = HashSet::new();
    for f in staged
        .iter()
        .filter(|f| matches!(f.code, 'A' | 'R' | 'C' | 'D'))
    {
        let paths = std::iter::once(f.path.as_str()).chain(f.old_path.as_deref());
        for path in paths.filter(|p| p.starts_with(&assets)) {
            if let Some(asset) = path.strip_suffix(".meta") {
                out.insert(asset.to_owned());
                out.insert(format!("{asset}/"));
                continue;
            }
            out.insert(format!("{path}.meta"));
            let mut dir = path;
            while let Some((parent, _)) = dir.rsplit_once('/') {
                if parent.len() < assets.len() {
                    break;
                }
                out.insert(format!("{parent}.meta"));
                dir = parent;
            }
        }
    }
    let mut out: Vec<String> = out.into_iter().collect();
    out.sort();
    out
}

/// The paths of `candidates` that are in the index (`git ls-files`), and the folders among
/// them (a candidate ending in `/` is in the index when something inside it is).
pub fn in_index(repo: &Path, candidates: &[String]) -> Result<HashSet<String>, String> {
    let mut found = HashSet::new();
    for batch in candidates.chunks(100) {
        let mut args = vec!["ls-files", "-z", "--"];
        let literal: Vec<String> = batch.iter().map(|c| format!(":(literal){c}")).collect();
        args.extend(literal.iter().map(String::as_str));
        let out = run(Some(repo), &args)?;
        for listed in out.split('\0').filter(|p| !p.is_empty()) {
            for c in batch {
                if listed == c || (c.ends_with('/') && listed.starts_with(c.as_str())) {
                    found.insert(c.clone());
                }
            }
        }
    }
    Ok(found)
}

#[cfg(test)]
#[path = "unity_tests.rs"]
pub(crate) mod tests;
