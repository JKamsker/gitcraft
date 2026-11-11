//! Opt-in local history of working changes. Roots are trees (not WIP commits); separate
//! index/work trees keep staged contents distinct and make every saved blob GC-reachable.
mod context;
pub mod deferred;
pub(in crate::git) mod index_transaction;
pub mod lists;
mod native_files;
mod permissions;
pub mod publication;
mod restore;
#[cfg(test)]
mod tests;
pub mod turn;
pub mod virtuals;
mod working_transaction;

use crate::{git, remote};
pub(in crate::git) use context::Context;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub use context::{
    configure as set_enabled, configured as enabled, configured_with as enabled_while,
};
pub use restore::{RestoreHunk, RestorePreview, preview, restore};
const MAX_INDEX: usize = 20_000;
const MAX_FILES: usize = 2_000;
pub const FILE_LIMIT: usize = 8 * 1024 * 1024;
const TOTAL_LIMIT: usize = 32 * 1024 * 1024;
const PREFIX: &str = "refs/gitcraft/drafts/";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Id {
    pub reference: String,
    pub tree: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub mode: String,
    pub oid: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct File {
    pub path: String,
    pub index: Option<Entry>,
    pub work: Option<Entry>,
    pub untracked: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub version: u32,
    pub created: i64,
    pub reason: String,
    pub worktree: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub index_tree: String,
    pub work_tree: String,
    pub files: Vec<File>,
}
#[derive(Clone, Debug)]
pub struct Draft {
    pub id: Id,
    pub metadata: Metadata,
}
#[derive(Clone, Copy)]
pub struct Retention {
    pub count: usize,
    pub days: i64,
}
impl Default for Retention {
    fn default() -> Self {
        Self {
            count: 50,
            days: 14,
        }
    }
}

#[derive(PartialEq, Eq)]
struct Source {
    raw: Vec<u8>,
    cache: Vec<u8>,
    cache_snapshot: Option<std::sync::Arc<index_transaction::IndexSnapshot>>,
    shared: Option<(String, Vec<u8>)>,
    index: BTreeMap<String, Entry>,
    skip: BTreeSet<String>,
    head: Option<String>,
    branch: Option<String>,
}

pub fn list(repo: &Path, active: &dyn Fn() -> bool) -> Result<Vec<Draft>, String> {
    let ctx = Context::new(repo, active)?;
    let drafts = list_in(&ctx, active)?;
    let now = git::now();
    let policy = Retention::default();
    cleanup_in(&ctx, &drafts, now, policy, None, active)?;
    Ok(drafts
        .into_iter()
        .enumerate()
        .filter(|(i, d)| {
            *i < policy.count && now.saturating_sub(d.metadata.created) <= policy.days * 86400
        })
        .map(|(_, d)| d)
        .collect())
}

fn list_in(ctx: &Context, active: &dyn Fn() -> bool) -> Result<Vec<Draft>, String> {
    let namespace = format!("{PREFIX}{}/", ctx.id);
    let refs = ctx.source(
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname)",
            &namespace,
        ],
        vec![],
        &[0],
        active,
    )?;
    let refs = std::str::from_utf8(&refs)
        .map_err(|_| crate::i18n::text("backend-invalid-draft-refs-3d3d89"))?;
    if refs.lines().count() > 200 {
        return Err(crate::i18n::text(
            "backend-too-many-draft-refs-inspect-refs-gitcraft-drafts-in-the-terminal-814e26",
        )
        .into());
    }
    let mut drafts = Vec::new();
    for line in refs.lines() {
        let Some((reference, tree)) = line.split_once(' ') else {
            continue;
        };
        let id = Id {
            reference: reference.into(),
            tree: tree.into(),
        };
        if let Ok(metadata) = metadata(ctx, &id, active) {
            drafts.push(Draft { id, metadata });
        }
        if drafts.len() > 200 {
            return Err(crate::i18n::text(
                "backend-too-many-draft-refs-inspect-refs-gitcraft-drafts-in-the-terminal-814e26",
            )
            .into());
        }
    }
    drafts.sort_by(|a, b| {
        b.metadata
            .created
            .cmp(&a.metadata.created)
            .then_with(|| b.id.reference.cmp(&a.id.reference))
    });
    Ok(drafts)
}
fn metadata(ctx: &Context, id: &Id, active: &dyn Fn() -> bool) -> Result<Metadata, String> {
    let namespace = format!("{PREFIX}{}/", ctx.id);
    let leaf = id
        .reference
        .strip_prefix(&namespace)
        .ok_or(crate::i18n::text("backend-invalid-draft-identity-be5f3f"))?;
    let parts: Vec<_> = leaf.split('-').collect();
    if parts.len() != 3
        || parts
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
        || !oid_valid(&id.tree)
    {
        return Err(crate::i18n::text("backend-invalid-draft-identity-be5f3f").into());
    }
    if ctx.text(&["cat-file", "-t", &id.tree], vec![], active)? != "tree" {
        return Err(crate::i18n::text("backend-draft-root-is-not-a-tree-cf96bf").into());
    }
    let bytes = ctx.run(
        &["cat-file", "blob", &format!("{}:metadata.json", id.tree)],
        vec![],
        &[0],
        active,
    )?;
    if bytes.len() > 1024 * 1024 {
        return Err(crate::i18n::text("backend-draft-metadata-is-too-large-a9a01f").into());
    }
    let meta: Metadata = serde_json::from_slice(&bytes)
        .map_err(|_| crate::i18n::text("backend-incompatible-draft-metadata-ea032a"))?;
    if meta.version != 1
        || meta.worktree != ctx.id
        || !oid_valid(&meta.index_tree)
        || !oid_valid(&meta.work_tree)
        || meta.files.len() > MAX_FILES
        || meta.reason.chars().count() > 160
        || meta.reason.chars().any(char::is_control)
        || meta.head.as_ref().is_some_and(|oid| !oid_valid(oid))
        || meta
            .branch
            .as_ref()
            .is_some_and(|branch| branch.len() > 4096 || branch.chars().any(char::is_control))
    {
        return Err(crate::i18n::text(
            "backend-unsupported-draft-metadata-version-or-contents-7828f5",
        )
        .into());
    }
    for (name, oid) in [("index", &meta.index_tree), ("work", &meta.work_tree)] {
        if ctx.text(
            &[
                "rev-parse",
                "--verify",
                "--end-of-options",
                &format!("{}:{name}", id.tree),
            ],
            vec![],
            active,
        )? != *oid
        {
            return Err(crate::i18n::text(
                "backend-draft-metadata-does-not-match-its-protected-trees-a37158",
            )
            .into());
        }
    }
    let mut paths = BTreeSet::new();
    for file in &meta.files {
        if !paths.insert(&file.path) {
            return Err(crate::i18n::text("backend-duplicate-draft-file-entry-414dd7").into());
        }
        valid_path(&file.path)?;
        for entry in [&file.index, &file.work].into_iter().flatten() {
            if !oid_valid(&entry.oid)
                || !matches!(
                    entry.mode.as_str(),
                    "100644" | "100755" | "120000" | "160000"
                )
            {
                return Err(crate::i18n::text("backend-invalid-draft-file-entry-ad48eb").into());
            }
        }
    }
    Ok(meta)
}
fn current(ctx: &Context, id: &Id, active: &dyn Fn() -> bool) -> Result<(), String> {
    metadata(ctx, id, active)?;
    let now = ctx.source(
        &["rev-parse", "--verify", "--end-of-options", &id.reference],
        vec![],
        &[0],
        active,
    )?;
    if std::str::from_utf8(&now)
        .map_err(|_| crate::i18n::text("backend-invalid-draft-ref-f6f5f2"))?
        .trim()
        != id.tree
    {
        return Err(crate::i18n::text(
            "backend-this-draft-ref-changed-reopen-earlier-versions-a2e9f9",
        )
        .into());
    }
    Ok(())
}
fn cleanup_in(
    ctx: &Context,
    drafts: &[Draft],
    now: i64,
    policy: Retention,
    keep: Option<&Id>,
    active: &dyn Fn() -> bool,
) -> Result<(), String> {
    for (i, draft) in drafts.iter().enumerate() {
        if keep.is_some_and(|id| *id == draft.id) {
            continue;
        }
        if i >= policy.count
            || now.saturating_sub(draft.metadata.created) > policy.days.saturating_mul(86400)
        {
            // Never delete a ref replaced since it was listed, nor another feature/version.
            metadata(ctx, &draft.id, active)?;
            ctx.source(
                &[
                    "update-ref",
                    "--no-deref",
                    "-d",
                    &draft.id.reference,
                    &draft.id.tree,
                ],
                vec![],
                &[0],
                active,
            )?;
        }
    }
    Ok(())
}

mod files;
#[cfg(all(test, unix))]
use files::missing_inside;

use files::oid_valid;
use files::read_file;
use files::read_file_limited;
use files::read_script;
use files::valid_path;

mod index;
use index::cached_index;
use index::changed;
pub(in crate::git) use index::inspection_index;
use index::internal_file;
use index::private_index_file;
use index::tree;
use index::untracked_paths;

mod capture;
pub use capture::capture;
use capture::capture_preserving;
use capture::capture_preserving_paths;
use capture::source;
