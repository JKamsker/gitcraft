use super::Repository;
use std::path::Path;
pub(crate) struct Receipt;
pub(crate) struct Outcome;
pub(crate) fn restore(
    _repository: &Repository,
    _parent: &Path,
    _source: Option<&Path>,
    _receipt: Option<Receipt>,
    _active: &dyn Fn() -> bool,
) -> Outcome {
    Outcome
}
