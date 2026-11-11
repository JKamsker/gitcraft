//! Command-palette contributions owned by this feature.
use crate::repo::{Action, reflog};

use crate::repo::actions::PaletteContribution;

pub(in crate::repo) fn items() -> [PaletteContribution; 2] {
    [
        (
            "Reflog".into(),
            "recovery".into(),
            Action::Reflog(reflog::ReflogAct::Show(reflog::Source::head())),
        ),
        (
            "Find lost commits".into(),
            "recovery".into(),
            Action::Reflog(reflog::ReflogAct::FindLost),
        ),
    ]
}
