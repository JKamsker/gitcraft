//! Apply completed transfer and close results to the presentation bridge.
use super::*;

// Pure finishing seams used by the actual transfer and close branches.
pub(super) fn transfer_finished(
    result: Result<(), TransferFailure>,
    removed: Result<(), String>,
    presentation: &mut Option<super::super::presentation::Presentation>,
) -> Result<(), TransferFailure> {
    *presentation = None;
    match (result, removed) {
        (result, Ok(())) => result,
        (Ok(()), Err(error)) => Err(TransferFailure::Operation(error)),
        (Err(primary), Err(cleanup)) => {
            let primary = primary.message();
            let raw = format!("{primary}; transfer cleanup: {cleanup}");
            *presentation = Some(super::super::presentation::Presentation::capture(
                raw.clone(),
                super::super::presentation::Cleanup::WorkspaceTransfer { primary, cleanup },
            ));
            Err(TransferFailure::Operation(raw))
        }
    }
}

pub(crate) fn close_finished(
    cleared: Result<(), String>,
    released: Result<(), String>,
    presentation: &mut Option<super::super::presentation::Presentation>,
) -> Result<(), String> {
    *presentation = None;
    match (cleared, released) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(e), Ok(())) | (Ok(()), Err(e)) => Err(e),
        (Err(primary), Err(release)) => {
            let raw = format!("{primary}; lease release: {release}");
            *presentation = Some(super::super::presentation::Presentation::capture(
                raw.clone(),
                super::super::presentation::Cleanup::WorkspaceLeaseRelease { primary, release },
            ));
            Err(raw)
        }
    }
}

pub(super) fn replay_close(
    error: &Option<String>,
    saved: &Option<super::super::presentation::Presentation>,
    presentation: &mut Option<super::super::presentation::Presentation>,
) -> Result<(), String> {
    let result = error.clone().map_or(Ok(()), Err);
    *presentation = saved.clone();
    super::super::presentation::retain_error(&result, presentation);
    result
}

#[cfg(test)]
pub(crate) fn supplied_transfer(
    primary: &str,
    cleanup: Option<&str>,
    presentation: &mut Option<super::super::presentation::Presentation>,
) -> Result<(), String> {
    transfer_finished(
        Err(TransferFailure::Operation(primary.into())),
        cleanup.map_or(Ok(()), |e| Err(e.into())),
        presentation,
    )
    .map_err(TransferFailure::message)
}
