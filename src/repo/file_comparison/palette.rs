//! Command-palette contributions owned by this feature.
use crate::repo::Action;

use crate::repo::actions::PaletteContribution;

pub(in crate::repo) fn items() -> [PaletteContribution; 4] {
    [
        (
            "Code moved across files".into(),
            "refactor moved block".into(),
            Action::OpenFileComparison(crate::repo::file_comparison::Page::CodeMoves),
        ),
        (
            "File migration map".into(),
            "files".into(),
            Action::OpenFileComparison(crate::repo::file_comparison::Page::Migration),
        ),
        (
            "Three-state staging lens".into(),
            "staging".into(),
            Action::OpenFileComparison(crate::repo::file_comparison::Page::Lens),
        ),
        (
            "File versions across branches".into(),
            "files".into(),
            Action::OpenFileComparison(crate::repo::file_comparison::Page::Matrix),
        ),
    ]
}
