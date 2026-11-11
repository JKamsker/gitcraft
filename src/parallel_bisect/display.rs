//! Paint-only adapters for this partition. Never feed these strings back into control,
//! protocol, cache, storage, command or worker-event identities. Unknown details stay opaque.
use crate::i18n;

mod exact_fixtures;
#[cfg(test)]
use exact_fixtures::*;

mod detail_fixtures;
#[cfg(test)]
use detail_fixtures::*;

/// Only pass application-generated diagnostics from this partition, at paint time.
/// stdout/stderr, custom-command output and arbitrary provider text must bypass this API.
/// Nested detail captures are intentionally opaque; no recursive search/replacement.
/// A Result<String> from bundle/observe does not prove cleanup-combinator ownership.
/// Use cleanup_diagnostic only with separately retained source-bound fields; raw strings
/// alone cannot select that seam. This function leaves unknown cleanup/keep lookalikes raw.
#[cfg(test)]
pub(crate) fn diagnostic(raw: &str) -> String {
    let exact = raw.strip_suffix('\n').unwrap_or(raw);
    let shown = raw;
    if let Some((_, id)) = EXACT.iter().find(|(source, _)| *source == exact) {
        return i18n::text(id).to_owned();
    }
    if let Some(text) = envelope(shown) {
        return text;
    }
    for &(prefix, suffix, id, name, _) in DETAIL {
        if let Some(value) = if prefix.starts_with("gitcraft: ") {
            exact
        } else {
            shown
        }
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(suffix))
        {
            return i18n::message(
                id,
                &[(
                    name,
                    if name == "machine" {
                        machine_location(value).into()
                    } else {
                        value.into()
                    },
                )],
            );
        }
    }
    trailing_envelope(shown).unwrap_or_else(|| raw.to_owned())
}
/// Finite templates emitted by session/runner/observe/workspace/bundle/verdict.
/// Split only known outer envelopes; captured external errors and output remain raw.
#[cfg(test)]
fn envelope(raw: &str) -> Option<String> {
    let message = |id, args: &[(&str, i18n::Value<'_>)]| Some(i18n::message(id, args));
    if let Some(rest) = raw.strip_prefix("could not check out ")
        && let Some((hash, rest)) = rest.split_once(" in a worktree on ")
        && let Some((machine, detail)) = rest.split_once(": ")
    {
        return message(
            "interaction-bisect-checkout",
            &[
                ("hash", hash.into()),
                ("machine", machine_location(machine).into()),
                ("detail", detail.into()),
            ],
        );
    }
    if let Some(rest) = raw.strip_prefix("The worktrees on ")
        && let Some((machine, rest)) = rest.split_once(" couldn't all be removed (")
        && let Some(detail) = rest.strip_suffix("); recovery evidence was retained. Resolve the reported ownership or removal failure before trying again.")
    {
        return message("interaction-bisect-worktrees-retained", &[("machine", machine_location(machine).into()), ("detail", detail.into())]);
    }
    if let Some(rest) = raw.strip_prefix("The test of ")
        && let Some((hash, rest)) = rest.split_once(" ended with exit code ")
        && let Some(code) = rest.strip_suffix(" (a crash or a signal): it counts as skipped.")
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-crash-skipped",
            &[("hash", hash.into()), ("code", code.into())],
        );
    }
    if let Some((hash, rest)) = raw.split_once(" ended its test with exit code ")
        && let Some((code, output)) = rest.split_once(
            "; 128 and more (a crash or a signal) stops a bisect, as with git bisect run",
        )
        && (output.is_empty() || output.starts_with('\n'))
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-end-test-failed",
            &[
                ("hash", hash.into()),
                ("code", code.into()),
                ("output", output.strip_prefix('\n').unwrap_or(output).into()),
            ],
        );
    }
    if let Some(rest) = raw.strip_prefix("ended its test with exit code ")
        && let Some(code) = rest.strip_suffix(
            "; 128 and more (a crash or a signal) stops a bisect, as with git bisect run",
        )
        && let Ok(code) = code.parse::<i32>()
    {
        return message("interaction-bisect-interpret", &[("code", code.into())]);
    }
    // Observe's stderr may itself contain the separator; use the first outer stderr marker.
    if let Some((before, stderr)) = raw.split_once("); stderr: ")
        && let Some((detail, code)) = before.rsplit_once(" (exit ")
        && known_observation_detail(detail)
        && (!detail.starts_with("PNG frame ")
            || (raw.matches(" (exit ").count() == 1 && raw.matches("); stderr: ").count() == 1))
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-observation-exit",
            &[
                ("detail", observation_detail(detail).into()),
                ("code", code.into()),
                ("stderr", stderr.into()),
            ],
        );
    }
    if let Some(rest) = raw.strip_prefix("could not make a repository for the bisect on ")
        && let Some((machine, detail)) = rest.split_once(": ")
    {
        return message(
            "interaction-bisect-repository-make",
            &[
                ("machine", machine_location(machine).into()),
                ("detail", detail.into()),
            ],
        );
    }
    if let Some(detail) = raw.strip_prefix("Contained Git failed (no exit code): ") {
        return message(
            "interaction-bisect-contained-no-code",
            &[("detail", detail.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("Contained Git failed (")
        && let Some((code, detail)) = rest.split_once("): ")
        && let Ok(code) = code.parse::<i32>()
    {
        return message(
            "interaction-bisect-contained-failed",
            &[("code", code.into()), ("detail", detail.into())],
        );
    }
    if let Some(rest) = raw.strip_prefix("bisect shell ended with status ") {
        if rest == "None" {
            return message("interaction-bisect-shell-no-status", &[]);
        }
        if let Some(code) = rest.strip_prefix("Some(").and_then(|s| s.strip_suffix(')'))
            && let Ok(code) = code.parse::<i32>()
        {
            return message(
                "interaction-bisect-shell-status",
                &[("status", code.into())],
            );
        }
    }
    for (prefix, id) in [
        (
            "Ref publication completed, but quarantine cleanup failed: ",
            "interaction-bisect-publication-cleanup",
        ),
        (
            "Verification completed, but quarantine cleanup failed: ",
            "interaction-bisect-verification-cleanup",
        ),
    ] {
        if let Some(detail) = raw.strip_prefix(prefix) {
            return message(id, &[("detail", detail.into())]);
        }
    }

    None
}

/// A paint-only name. Machine::label remains the raw cache/session identity.
pub(crate) fn machine_label(machine: &super::Machine) -> String {
    match machine {
        super::Machine::Here => i18n::text("interaction-bisect-machine-here").to_owned(),
        super::Machine::There(host) => host.label(),
    }
}

/// Display a captured Machine::label without changing its cached identity.
#[cfg(test)]
pub(crate) fn machine_name(raw: &str) -> &str {
    if raw == "this computer" {
        i18n::text("interaction-bisect-machine-here")
    } else {
        raw
    }
}

/// Verdict::word and its consumers that classify/store results remain unchanged.
#[cfg(test)]
pub(crate) fn verdict(verdict: super::Verdict) -> &'static str {
    i18n::text(match verdict {
        super::Verdict::Good => "interaction-bisect-verdict-good",
        super::Verdict::Bad => "interaction-bisect-verdict-bad",
        super::Verdict::Skip => "interaction-bisect-verdict-skipped",
        super::Verdict::TimedOut => "interaction-bisect-verdict-timed-out",
    })
}

/// Supplied numeric benchmark value only; never use this to parse or cache worker output.
#[cfg(test)]
pub(crate) fn scalar(value: f64) -> String {
    i18n::message("interaction-bisect-scalar", &[("value", value.into())])
}

/// Session warnings may prefix an app-owned error with its short commit hash.
/// Do not pass worker output to this adapter.
#[cfg(test)]
pub(crate) fn warning(raw: &str) -> String {
    if let Some((hash, detail)) = raw.split_once(": ")
        && matches!(hash.len(), 8 | 40 | 64)
        && hash.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return i18n::message(
            "interaction-bisect-warning",
            &[("hash", hash.into()), ("detail", diagnostic(detail).into())],
        );
    }
    diagnostic(raw)
}

pub(crate) fn machine_location(raw: &str) -> &str {
    if raw == "this computer" {
        i18n::text("interaction-bisect-machine-here-location")
    } else {
        raw
    }
}

#[cfg(test)]
fn observation_detail(raw: &str) -> String {
    let id = match raw {
        "Frame output exceeds 2 MiB; no result cached" => "interaction-bisect-frame-too-large",
        "Text output is not UTF-8" => "interaction-bisect-text-encoding",
        "Benchmark output is not UTF-8" => "interaction-bisect-benchmark-encoding",
        "Benchmark must print one finite number" => "interaction-bisect-benchmark-finite",
        _ => {
            return raw
                .strip_prefix("PNG frame could not be decoded (maximum 1024×1024): ")
                .map_or_else(
                    || raw.to_owned(),
                    |detail| {
                        i18n::message(
                            "interaction-bisect-png-decode",
                            &[("detail", detail.into())],
                        )
                    },
                );
        }
    };
    i18n::text(id).to_owned()
}

// This source-owned repository-path form is separate from cleanup-combinator metadata.
// Cleanup/keep suffixes are never inferred here: lower-level errors can contain them.
#[cfg(test)]
fn trailing_envelope(raw: &str) -> Option<String> {
    let message = |id, args: &[(&str, i18n::Value<'_>)]| Some(i18n::message(id, args));
    if let Some(rest) = raw.strip_suffix(" is not a git repository")
        && let Some((path, machine)) = rest.rsplit_once(" on ")
    {
        return message(
            "interaction-bisect-not-repository",
            &[
                ("path", path.into()),
                ("machine", machine_location(machine).into()),
            ],
        );
    }
    None
}

mod cleanup;
pub(crate) use cleanup::*;

#[cfg(test)]
mod execution_fluent_tests;

#[cfg(test)]
mod execution_fluent_payload_tests;

#[cfg(test)]
mod execution_fluent_r1_tests;
