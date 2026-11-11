//! Metadata.

use super::*;

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

/// Whether Unity makes no `.meta` for `path` (under `assets`): a hidden name (`.x`), one
/// ending in `~` (`Samples~`), a `cvs` folder or a `.tmp` file, there or in a folder above.
pub(crate) fn skipped_by_unity(path: &str, assets: &str) -> bool {
    let Some(rest) = path.strip_prefix(assets) else {
        return true;
    };
    rest.split('/').any(|name| {
        name.starts_with('.')
            || name.ends_with('~')
            || name.eq_ignore_ascii_case("cvs")
            || name.to_ascii_lowercase().ends_with(".tmp")
    })
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
    let ours = |p: &&str| {
        p.starts_with(&assets) && !skipped_by_unity(p.strip_suffix(".meta").unwrap_or(p), &assets)
    };
    for path in added.iter().copied().filter(ours) {
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
    for path in deleted.iter().copied().filter(ours) {
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
pub(super) fn is_folder_meta(
    asset: &str,
    added: &HashSet<&str>,
    tracked: &dyn Fn(&str) -> bool,
) -> bool {
    let inside = format!("{asset}/");
    added.iter().any(|p| p.starts_with(&inside)) || tracked(&inside)
}

/// What the index holds under a folder: its files, and every folder they are in.
#[derive(Clone, Debug, Default)]
pub struct IndexListing {
    pub(super) files: HashSet<String>,
    pub(super) folders: HashSet<String>,
}

impl IndexListing {
    /// Whether the index has `path`, or, for a path ending in `/`, anything in that folder.
    pub fn has(&self, path: &str) -> bool {
        match path.strip_suffix('/') {
            Some(dir) => self.folders.contains(dir),
            None => self.files.contains(path),
        }
    }

    pub(crate) fn of(files: impl IntoIterator<Item = String>) -> Self {
        let mut listing = IndexListing::default();
        for f in files {
            let mut dir = f.as_str();
            while let Some((parent, _)) = dir.rsplit_once('/') {
                if !listing.folders.insert(parent.to_owned()) {
                    break;
                }
                dir = parent;
            }
            listing.files.insert(f);
        }
        listing
    }
}

/// What the index holds under `folder` (`Game/Assets/`): one `git ls-files`.
pub fn index_under(repo: &Path, folder: &str) -> Result<IndexListing, String> {
    let spec = format!(":(literal){folder}");
    let out = run(Some(repo), &["ls-files", "-z", "--", &spec])?;
    Ok(IndexListing::of(
        out.split('\0').filter(|p| !p.is_empty()).map(str::to_owned),
    ))
}
