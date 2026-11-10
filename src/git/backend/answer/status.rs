//! Porcelain v1 for ordinary, non-conflicted worktrees. Ambiguous cases stay with Git.
use super::*;
use std::collections::BTreeMap;

/// Produce porcelain-v1 status for ordinary worktrees; defer unsupported states to Git.
pub(super) fn answer(path: &Path, args: &[&str]) -> Option<Answer> {
    if args != ["--porcelain=v1", "-z", "--untracked-files=all"] {
        return None;
    }
    let repo = open(path)?;
    plain(&repo)?;
    let root = repo.workdir()?;
    if root.join(".gitmodules").exists() {
        return None;
    }
    let config = repo.config_snapshot();
    for key in ["status.renames", "diff.renames"] {
        if config
            .string(key)
            .is_some_and(|v| v.eq_ignore_ascii_case(b"copies"))
        {
            return None;
        }
    }
    let index = repo.index_or_empty().ok()?;
    use gix::index::entry::{Flags, Mode};
    if index.is_sparse()
        || index.entries().iter().any(|e| {
            e.mode == Mode::COMMIT
                || e.stage_raw() != 0
                || e.flags
                    .intersects(Flags::SKIP_WORKTREE | Flags::ASSUME_VALID | Flags::INTENT_TO_ADD)
        })
    {
        return None;
    }
    // gix's encoding_rs conversions don't cover all of Git's iconv/BOM rules.
    let mut attrs = repo
        .attributes_only(
            &index,
            gix::worktree::stack::state::attributes::Source::WorktreeThenIdMapping,
        )
        .ok()?;
    let mut selected = attrs.selected_attribute_matches(["working-tree-encoding", "filter"]);
    for entry in index.entries() {
        attrs
            .at_entry(entry.path(&index), Some(entry.mode))
            .ok()?
            .matching_attributes(&mut selected);
        if selected
            .iter_selected()
            .any(|m| !m.assignment.state.is_unspecified() && !m.assignment.state.is_unset())
        {
            return None;
        }
    }
    let mut tracked: BTreeMap<BString, [u8; 2]> = BTreeMap::new();
    let mut untracked = std::collections::BTreeSet::new();
    let (mut added, mut deleted) = (false, false);
    let items = repo
        .status(gix::progress::Discard)
        .ok()?
        .untracked_files(gix::status::UntrackedFiles::Files)
        .tree_index_track_renames(gix::status::tree_index::TrackRenames::Disabled)
        .index_worktree_rewrites(None)
        .into_iter(Vec::<BString>::new())
        .ok()?;
    for item in items {
        match item.ok()? {
            gix::status::Item::TreeIndex(change) => {
                use gix::diff::index::ChangeRef as C;
                let (location, code) = match change {
                    C::Addition {
                        location,
                        entry_mode,
                        ..
                    } => {
                        if entry_mode == Mode::COMMIT {
                            return None;
                        }
                        added = true;
                        (location, b'A')
                    }
                    C::Deletion {
                        location,
                        entry_mode,
                        ..
                    } => {
                        if entry_mode == Mode::COMMIT {
                            return None;
                        }
                        deleted = true;
                        (location, b'D')
                    }
                    C::Modification {
                        location,
                        previous_entry_mode,
                        entry_mode,
                        ..
                    } => {
                        if entry_mode == Mode::COMMIT || previous_entry_mode == Mode::COMMIT {
                            return None;
                        }
                        let kind = |mode: Mode| mode.bits() & 0o170000;
                        (
                            location,
                            if kind(entry_mode) == kind(previous_entry_mode) {
                                b'M'
                            } else {
                                b'T'
                            },
                        )
                    }
                    _ => return None,
                };
                tracked.entry(location.into_owned()).or_insert([b' '; 2])[0] = code;
            }
            gix::status::Item::IndexWorktree(item) => {
                use gix::status::index_worktree::Item as I;
                use gix::status::plumbing::index_as_worktree::{Change, EntryStatus};
                match item {
                    I::Modification {
                        rela_path, status, ..
                    } => {
                        let code = match status {
                            EntryStatus::NeedsUpdate(_) => continue,
                            EntryStatus::Change(Change::Removed) => b'D',
                            EntryStatus::Change(Change::Modification { .. }) => b'M',
                            // Untrackable filesystem types differ between gix and Git.
                            EntryStatus::Change(Change::Type { worktree_mode })
                                if worktree_mode == Mode::FILE
                                    || worktree_mode == Mode::SYMLINK
                                    || worktree_mode == Mode::FILE_EXECUTABLE =>
                            {
                                b'T'
                            }
                            _ => return None,
                        };
                        tracked.entry(rela_path).or_insert([b' '; 2])[1] = code;
                    }
                    I::DirectoryContents { entry, .. } => {
                        if entry.status != gix::dir::entry::Status::Untracked {
                            continue;
                        }
                        if !matches!(
                            entry.disk_kind,
                            Some(gix::dir::entry::Kind::File | gix::dir::entry::Kind::Symlink)
                        ) {
                            return None;
                        }
                        untracked.insert(entry.rela_path);
                    }
                    _ => return None,
                }
            }
        }
    }
    if added && deleted {
        return None;
    }
    let mut out = Vec::new();
    for (path, xy) in tracked {
        out.extend_from_slice(&xy);
        out.push(b' ');
        out.extend_from_slice(&path);
        out.push(0);
    }
    for path in untracked {
        out.extend_from_slice(b"?? ");
        out.extend_from_slice(&path);
        out.push(0);
    }
    Some(Answer::out(out))
}
