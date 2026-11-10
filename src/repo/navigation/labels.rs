//! Human-readable labels for navigation destinations.

use super::*;

pub(super) fn diff_label(key: &DiffKey) -> String {
    let name = file_name(key.path());
    match key {
        DiffKey::Remerge { hash, .. } => crate::i18n::message(
            "repo-core-2911",
            &[
                ("name", name.to_string().into()),
                ("hash", short(hash).to_string().into()),
            ],
        ),
        DiffKey::Commit { hash, .. } => crate::i18n::message(
            "repo-core-2912",
            &[
                ("name", name.to_string().into()),
                ("hash", short(hash).to_string().into()),
            ],
        ),
        DiffKey::Work(file) => match file.area {
            Area::Unstaged => {
                crate::i18n::message("repo-core-2913", &[("name", name.to_string().into())])
            }
            Area::Staged => {
                crate::i18n::message("repo-core-2914", &[("name", name.to_string().into())])
            }
            Area::Conflicted => {
                crate::i18n::message("repo-core-2915", &[("name", name.to_string().into())])
            }
            Area::Untracked => {
                crate::i18n::message("repo-core-2916", &[("name", name.to_string().into())])
            }
        },
        DiffKey::Range {
            from, to: Some(to), ..
        } => crate::i18n::message(
            "repo-core-2917",
            &[
                ("name", name.to_string().into()),
                ("from", short(from).to_string().into()),
                ("to", short(to).to_string().into()),
            ],
        ),
        DiffKey::Range { from, to: None, .. } => crate::i18n::message(
            "repo-core-2918",
            &[
                ("name", name.to_string().into()),
                ("from", short(from).to_string().into()),
            ],
        ),
    }
}

/// Why a change of the working tree can't be shown again, in its area's words.
pub(super) fn gone_change(file: &git::StatusFile) -> String {
    let path = &file.path;
    match file.area {
        Area::Unstaged => {
            crate::i18n::message("repo-core-2919", &[("path", path.to_string().into())])
        }
        Area::Staged => {
            crate::i18n::message("repo-core-2920", &[("path", path.to_string().into())])
        }
        Area::Conflicted => {
            crate::i18n::message("repo-core-2921", &[("path", path.to_string().into())])
        }
        Area::Untracked => {
            crate::i18n::message("repo-core-2922", &[("path", path.to_string().into())])
        }
    }
}

/// The last part of a path, as the tooltips name a file.
pub(super) fn file_name(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or(path)
}
