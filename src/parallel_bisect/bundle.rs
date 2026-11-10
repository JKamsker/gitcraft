//! Bundle quarantine uses the existing machine-side workspace claim and durable owners.
//! Only the final expected-absent transaction publishes refs; no checkout or fetch.
use super::{
    Machine, runner,
    workspace::{self, Workspace},
};
use crate::git::BundleRef;
use crate::{git, remote};
use std::path::Path;
use std::time::Duration;

pub(crate) fn identity(repo: &Path) -> Result<String, String> {
    workspace::canonical_common(repo)
}

#[expect(
    clippy::too_many_arguments,
    reason = "Preserves the original seven import operands with the optional producer-owned presentation out-slot"
)]
pub(crate) fn run_with_presentation(
    repo: &Path,
    expected: &str,
    pack: &[u8],
    refs: &[BundleRef],
    prerequisites: &[BundleRef],
    mapped: &[BundleRef],
    import: bool,
    presentation: &mut Option<super::presentation::Presentation>,
) -> Result<(), String> {
    *presentation = None;
    if !remote::is_remote(repo) && !cfg!(any(target_os = "linux", windows)) {
        return Err("This platform cannot establish enforced bundle memory limits. Import/unpack is supported on Linux (including SSH/WSL) and Windows; inspection and export remain available. No pack was opened.".into());
    }
    if identity(repo)? != expected {
        return Err(
            "The destination repository changed since inspection; reopen the bundle".into(),
        );
    }
    let mut ws = Workspace::open(repo, &Machine::of(repo), None)?;
    // Every fallible post-claim path reaches explicit close so cleanup errors cannot
    // disappear through Workspace::Drop while a setup error is being propagated.
    let result = (|| {
        let common = identity(&ws.repo)?;
        if common != expected {
            return Err("The destination repository changed before quarantine setup".into());
        }
        let mut backend = Native {
            common,
            quarantine: format!("{}/bundle-quarantine.git", ws.base_there),
            ws: &ws,
        };
        pipeline_with_presentation(
            &mut backend,
            pack,
            refs,
            prerequisites,
            mapped,
            import,
            presentation,
        )
    })();
    let mut cleanup_presentation = None;
    let cleanup = ws.close_with_presentation(&mut cleanup_presentation);
    completed_with_presentation(result, cleanup, import, presentation, cleanup_presentation)
}

pub(crate) fn completed_with_presentation(
    result: Result<(), String>,
    cleanup: Result<(), String>,
    import: bool,
    presentation: &mut Option<super::presentation::Presentation>,
    cleanup_presentation: Option<super::presentation::Presentation>,
) -> Result<(), String> {
    use super::presentation::{Cleanup, Presentation};
    let context = match (&result, &cleanup) {
        (Err(primary), Err(cleanup)) => Some(Cleanup::BundleCompleted {
            primary: primary.clone(),
            cleanup: cleanup.clone(),
        }),
        _ => None,
    };
    let cleanup_only = result
        .is_ok()
        .then(|| cleanup.as_ref().err().cloned())
        .flatten();
    let result = completed(result, cleanup, import);
    if let (Err(raw), Some(context)) = (&result, context) {
        *presentation = Some(Presentation::capture(raw.clone(), context));
    } else if let (Err(raw), Some(cleanup)) = (&result, cleanup_only) {
        *presentation = Some(Presentation::completed(
            raw.clone(),
            import,
            cleanup,
            cleanup_presentation,
        ));
    }
    super::presentation::retain_error(&result, presentation);
    result
}

fn completed(
    result: Result<(), String>,
    cleanup: Result<(), String>,
    import: bool,
) -> Result<(), String> {
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(primary), Ok(())) => Err(primary),
        (Ok(()), Err(cleanup)) => Err(format!(
            "{} completed, but quarantine cleanup failed: {cleanup}",
            if import {
                "Ref publication"
            } else {
                "Verification"
            }
        )),
        (Err(primary), Err(cleanup)) => {
            Err(format!("{primary}; quarantine cleanup failed: {cleanup}"))
        }
    }
}
mod pipeline;
use pipeline::*;

mod native;
use native::*;

#[cfg(test)]
mod recording_tests;

#[cfg(test)]
mod execution_fluent_r1_tests;

pub(crate) fn keep_retained(
    primary: String,
    hash: &str,
    presentation: &mut Option<super::presentation::Presentation>,
) -> String {
    let raw = format!("{primary}; owned pack-{hash}.keep retained");
    *presentation = Some(super::presentation::Presentation::capture(
        raw.clone(),
        super::presentation::Cleanup::BundleKeepRetained {
            primary,
            hash: hash.into(),
        },
    ));
    raw
}
