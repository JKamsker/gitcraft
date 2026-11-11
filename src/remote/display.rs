//! Finite presentation adapters for raw remote diagnostics and queued notices.
//!
//! Call only at the error/notice paint boundary, after every raw protocol/control check.
//! Hosts (including Host::label), paths, commands and external details keep their bytes.
//! This module never launches a helper, reads a repository or changes a producer result.

use crate::i18n::{message, text};

mod composition;

// Exact application-owned diagnostics, source-bound in the handoff inventory. Shared
// helper spellings intentionally resolve to one ID; unknown external text stays opaque.
mod catalog;

// Only these complete envelopes may interpolate a single opaque value. Never replace
// words or substrings inside an external payload. Numeric slots must parse canonically.
const ENVELOPES: &[(&str, &str, &str, &str)] = &[
    ("exit status ", "", "interaction-remote-exit-status", "code"),
    (
        "Forgot ",
        " saved sign-ins.",
        "interaction-remote-forgot-many",
        "count",
    ),
    ("could not reach ", "", "interaction-remote-reach", "host"),
    (
        "invalid ownership record: ",
        "",
        "interaction-remote-invalid-record",
        "detail",
    ),
    (
        "cleanup partial; external ledger retained: ",
        "",
        "interaction-remote-cleanup-partial",
        "detail",
    ),
    (
        "gitcraft owned endpoint: ",
        "",
        "interaction-remote-owned-endpoint",
        "detail",
    ),
    (
        "workspace claim unavailable: ",
        "",
        "interaction-remote-claim",
        "detail",
    ),
    (
        "gitcraft workspace claim: ",
        "",
        "interaction-remote-claim-helper",
        "detail",
    ),
    (
        "oversized ledger entry ",
        "",
        "interaction-remote-ledger-entry",
        "name",
    ),
    (
        "unknown ledger evidence ",
        "; preserved",
        "interaction-remote-ledger-evidence",
        "name",
    ),
    (
        "",
        " isn't gitcraft's",
        "interaction-remote-not-owned",
        "path",
    ),
    (
        "",
        " isn't a scratch folder",
        "interaction-remote-not-scratch",
        "path",
    ),
    (
        "A folder on ",
        ": /home/me/folder or ~/folder",
        "interaction-remote-folder-example",
        "host",
    ),
    (
        "That folder is on ",
        ", the repository here",
        "interaction-remote-folder-here",
        "host",
    ),
    (
        "private file: ",
        "",
        "interaction-remote-private-file",
        "detail",
    ),
    (
        "workspace open failed: ",
        "",
        "interaction-remote-workspace-open",
        "detail",
    ),
    (
        "owned signal refused: ",
        "",
        "interaction-remote-owned-signal",
        "detail",
    ),
    (
        "",
        "; command delivery/cleanup unknown",
        "interaction-remote-delivery-unknown",
        "detail",
    ),
    (
        "gitcraft: no such folder: ",
        "",
        "interaction-remote-no-folder",
        "path",
    ),
];

fn known(raw: &str) -> Option<String> {
    let (_, id, limit) = catalog::entries().find(|(source, _, _)| *source == raw)?;
    Some(match limit {
        Some(limit) => message(id, &[("limit", (*limit).into())]),
        None => text(id).to_owned(),
    })
}

/// Render only the finite remote-owned error grammar. Pass original producer bytes;
/// never store this result or use it for retry, cleanup, authentication or selection.
#[must_use]
pub fn diagnostic(raw: &str) -> String {
    render(raw).unwrap_or_else(|| raw.to_owned())
}

fn render(raw: &str) -> Option<String> {
    if let Some(value) = known(raw) {
        return Some(value);
    }
    // Preserve the exact newline of the known marker/Perl control diagnostics.
    if let Some(line) = raw.strip_suffix('\n')
        && matches!(
            line,
            "gitcraft: invalid ownership handshake"
                | "gitcraft: oversized ownership handshake"
                | "workspace lease expired"
                | "workspace controller lost"
        )
    {
        return known(line).map(|line| format!("{line}\n"));
    }
    match raw {
        "No saved sign-ins to forget." => {
            return Some(text("interaction-remote-no-saved").to_owned());
        }
        "Forgot 1 saved sign-in." => {
            return Some(message(
                "interaction-remote-forgot-many",
                &[("count", 1.into())],
            ));
        }
        _ => {}
    }
    if let Some(value) = composition::owned_envelope(raw) {
        return Some(value);
    }
    if let Some(value) = two_arguments(raw) {
        return Some(value);
    }
    for &(prefix, suffix, id, name) in ENVELOPES {
        if let Some(value) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
        {
            if name == "code" {
                let number = value.parse::<i32>().ok()?;
                if number.to_string() == value {
                    return Some(message(id, &[(name, number.into())]));
                }
            } else if name == "count" {
                let number = value.parse::<u32>().ok()?;
                if number.to_string() == value {
                    return Some(message(id, &[(name, number.into())]));
                }
            } else {
                return Some(message(id, &[(name, value.into())]));
            }
        }
    }
    None
}

fn pair(id: &str, first: (&str, &str), second: (&str, &str)) -> String {
    message(
        id,
        &[(first.0, first.1.into()), (second.0, second.1.into())],
    )
}

fn two_arguments(raw: &str) -> Option<String> {
    if let Some((host, detail)) = raw
        .strip_suffix(").")
        .and_then(|s| s.split_once(": the password couldn't be saved ("))
    {
        // vault errors are raw, and may include the exact portable-store diagnostic.
        let detail = known(detail).unwrap_or_else(|| detail.to_owned());
        return Some(pair(
            "interaction-remote-password-save",
            ("host", host),
            ("detail", &detail),
        ));
    }
    for (prefix, separator, id) in [
        ("could not reach ", ": ", "interaction-remote-reach-detail"),
        (
            "could not prepare ",
            " input: ",
            "interaction-remote-prepare-input",
        ),
        (
            "could not write a file on ",
            ": ",
            "interaction-remote-write-file",
        ),
        (
            "could not make a scratch folder on ",
            ": ",
            "interaction-remote-scratch-folder",
        ),
        (
            "lost the connection to ",
            ": ",
            "interaction-remote-lost-connection",
        ),
        (
            "That folder is on ",
            ", the repository on ",
            "interaction-remote-folder-there",
        ),
    ] {
        if let Some((host, detail)) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.split_once(separator))
        {
            if id == "interaction-remote-folder-there" {
                return Some(pair(id, ("host", host), ("repository_host", detail)));
            }
            if id == "interaction-remote-reach-detail" {
                for (suffix, id) in [
                    (
                        " - its host key wasn't trusted",
                        "interaction-remote-reach-untrusted",
                    ),
                    (
                        " - check the user, the key or the password",
                        "interaction-remote-reach-permission",
                    ),
                ] {
                    if let Some(detail) = detail.strip_suffix(suffix) {
                        return Some(pair(id, ("host", host), ("detail", detail)));
                    }
                }
            }
            if matches!(
                id,
                "interaction-remote-write-file" | "interaction-remote-scratch-folder"
            ) {
                let child = composition::filesystem(detail);
                return Some(pair(
                    id,
                    ("host", host),
                    ("detail", child.as_deref().unwrap_or(detail)),
                ));
            }
            return Some(pair(id, ("host", host), ("detail", detail)));
        }
    }
    if let Some(body) = raw.strip_suffix(": programs of this machine can't open it")
        && let Some((path, host)) = body.rsplit_once(" is on ")
    {
        return Some(if host == "another machine" {
            message(
                "interaction-remote-cannot-open-other",
                &[("path", path.into())],
            )
        } else {
            pair(
                "interaction-remote-cannot-open",
                ("path", path),
                ("host", host),
            )
        });
    }
    for (separator, id) in [
        (
            "; atomic cleanup proof unavailable: ",
            "interaction-remote-atomic-proof",
        ),
        (
            "; NotDispatched proof publication failed: ",
            "interaction-remote-not-dispatched-proof",
        ),
    ] {
        if let Some((detail, proof)) = raw.split_once(separator) {
            return Some(
                composition::native_proof(id, detail, proof)
                    .unwrap_or_else(|| pair(id, ("detail", detail), ("proof", proof))),
            );
        }
    }
    if let Some(body) = raw
        .strip_prefix("every connection to ")
        .and_then(|s| s.strip_suffix(" is busy; try again when what runs there is done"))
    {
        return Some(message(
            "interaction-remote-connections-busy",
            &[("host", body.into())],
        ));
    }
    if let Some((count, body)) = raw.split_once(" diff or merge tools are already open on ")
        && let Some(host) = body.strip_suffix(": close one of them first")
        && let Ok(count_value) = count.parse::<u32>()
        && count_value.to_string() == count
    {
        return Some(message(
            "interaction-remote-tools-busy",
            &[("count", count_value.into()), ("host", host.into())],
        ));
    }
    if let Some(command) = raw.strip_prefix("gitcraft asked another machine for ")
        .and_then(|s| s.strip_suffix(" from the window's thread, which must never wait for a network: please report this"))
    {
        return Some(message("interaction-remote-ui-thread", &[("command", command.into())]));
    }
    None
}

/// Session errors include an opaque command before a host. The caller must supply the
/// same command/host that produced the error; never infer a command from arbitrary stderr.
#[must_use]
#[cfg(test)]
pub fn execution_error(raw: &str, command: &str, host: &super::Host) -> String {
    let label = host.label();
    let prefix = format!("{command} on {label}: ");
    if let Some(detail) = raw.strip_prefix(&prefix) {
        return message(
            "interaction-remote-execution",
            &[
                ("command", command.into()),
                ("host", label.into()),
                ("detail", diagnostic(detail).into()),
            ],
        );
    }
    let prefix = format!("{command} didn't answer in time: ");
    if let Some(detail) = raw.strip_prefix(&prefix) {
        return pair(
            "interaction-remote-no-answer",
            ("command", command),
            ("detail", &diagnostic(detail)),
        );
    }
    diagnostic(raw)
}

/// Python's exception type and Perl's source location are diagnostic metadata, kept
/// verbatim. Only an exact known application message is translated; tracebacks and
/// unknown OS/JSON errors must be passed through unchanged by the caller.
#[must_use]
#[cfg(test)]
pub fn python_exception(kind: &str, detail: &str) -> String {
    let detail = known(detail).unwrap_or_else(|| detail.to_owned());
    pair(
        "interaction-remote-python-exception",
        ("kind", kind),
        ("detail", &detail),
    )
}

#[must_use]
#[cfg(test)]
pub fn perl_exception(detail: &str, location: &str) -> String {
    let detail = diagnostic(detail);
    pair(
        "interaction-remote-perl-exception",
        ("detail", &detail),
        ("location", location),
    )
}

/// Render the owned UI-thread debug envelope from supplied values only. The caller
/// retains the original backtrace payload and never uses this for control decisions.
// Not wired: ui_guard::off_ui_thread writes its raw backtrace directly to stderr.
// Keep the display seam until a presentation consumer receives those fields.
#[must_use]
#[allow(
    dead_code,
    reason = "Localization adapter awaits a source-owned presentation handoff"
)]
pub fn ui_thread_debug(command: &str, detail: &str) -> String {
    pair(
        "interaction-remote-ui-thread-debug",
        ("command", command),
        ("detail", detail),
    )
}

#[cfg(test)]
mod tests;
