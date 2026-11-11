//! RF-R1-1: finite known-child rendering at the existing owned composition seams.
//! No calls back into diagnostic/render: the call graph below is acyclic. Unknown
//! details, provider stderr and multiline payloads never undergo recursive matching.
use super::{known, message, pair};

// NativePlan::{prepare,complete}, WorkspaceAuthority record validation. These are
// raw source identities, not translated control strings. Source bindings are in the
// r1 inventory; native/helper/runtime producers themselves remain byte-identical.
const LEDGER: &[&str] = &[
    "cached owner proof changed",
    "cleaned manifest exceeds limit; external proofs retained",
    "cleanup cancelled or expired; ledger retained",
    "cleanup cancelled; external evidence retained",
    "cleanup owner is not durably indexed",
    "external ledger directory replaced",
    "invalid cleaned ledger manifest",
    "invalid workspace owner binding",
    "invalid workspace owner nonce/generation",
    "invalid workspace owner record name",
    "ledger evidence changed or collided",
    "ledger receipt does not authenticate its exact owner",
    "non-text workspace ledger name",
    "orphan owner proof without cleaned manifest; recovery unknown",
    "oversized authenticated owner proof",
    "oversized ledger entry",
    "oversized ledger proof",
    "oversized owner record",
    "oversized workspace owner",
    "owner changed after cleaned proof; evidence retained",
    "owner record generation/kind mismatch",
    "owner record set changed during recovery",
    "proof changed before pruning; manifest retained",
    "proof filename mismatch",
    "workspace ledger inventory limit; evidence retained",
    "workspace owner anchor/version mismatch",
    "workspace owner record exceeds 2048 bytes",
    "workspace owner record is outside its ledger",
    "workspace recovery exceeds 4096 owners",
    "workspace recovery exceeds 4096 owners; evidence retained",
];

const NATIVE: &[&str] = &[
    "foreign native command encoding",
    "invalid native request",
    "invalid native request identity/deadline",
    "cleanup requires full actual-machine mutation authority",
    "missing actual workspace fence",
    "private directory creation requires workspace authority",
    "private directory is outside claimed workspace",
    "private directory creation cancelled before mutation",
    "missing directory fence",
    "cleanup authority missing",
    "oversized native input",
    "native cleanup unconfirmed",
];
const STOP: &[&str] = &[
    "invalid native owner endpoint",
    "native terminal receipt unavailable",
    "unconfirmed native terminal receipt",
    "native owner unavailable; cleanup unknown",
    "native stop was not acknowledged",
    "oversized protocol frame",
    "incomplete protocol frame",
    "protocol write stopped",
    "absolute protocol deadline expired",
];
const CLAIM: &[&str] = &[
    "invalid workspace claim request",
    "invalid workspace claim lifetime/root",
    "workspace claim cancelled before acknowledgement",
];

fn exact(raw: &str, sources: &[&str]) -> Option<String> {
    sources.contains(&raw).then(|| known(raw)).flatten()
}
fn single_line(raw: &str) -> bool {
    !raw.contains(['\n', '\r'])
}

// Endpoint::stop_exchange + Endpoint::terminal. The error and proof have different
// source provenance; only the proof's two actual owned receipt errors may translate.
fn stop(raw: &str) -> Option<String> {
    exact(raw, STOP).or_else(|| {
        let (error, proof) = raw.split_once("; atomic cleanup proof unavailable: ")?;
        atomic_proof(error, proof)
    })
}

fn ledger(raw: &str) -> Option<String> {
    exact(raw, LEDGER)
        .or_else(|| {
            exact(
                raw,
                &[
                    "oversized native owner record",
                    "invalid cached native retirement proof",
                ],
            )
        })
        .or_else(|| stop(raw))
        .or_else(|| {
            if !single_line(raw) {
                return None;
            }
            for (prefix, suffix, id) in [
                (
                    "oversized ledger entry ",
                    "",
                    "interaction-remote-ledger-entry",
                ),
                (
                    "unknown ledger evidence ",
                    "; preserved",
                    "interaction-remote-ledger-evidence",
                ),
            ] {
                if let Some(name) = raw
                    .strip_prefix(prefix)
                    .and_then(|s| s.strip_suffix(suffix))
                {
                    return Some(message(id, &[("name", name.into())]));
                }
            }
            None
        })
}

fn native_leaf(raw: &str) -> Option<String> {
    exact(raw, NATIVE).or_else(|| ledger(raw))
}
fn native(raw: &str) -> Option<String> {
    native_leaf(raw).or_else(|| {
        let (error, proof) = raw.split_once("; NotDispatched proof publication failed: ")?;
        native_proof("interaction-remote-not-dispatched-proof", error, proof)
    })
}
fn claim(raw: &str) -> Option<String> {
    exact(raw, CLAIM).or_else(|| {
        let body = raw.strip_prefix("gitcraft workspace claim: ")?;
        let child = exact(body, CLAIM)?;
        Some(message(
            "interaction-remote-claim-helper",
            &[("detail", child.into())],
        ))
    })
}

// eprintln adds exactly one LF. Strip it only after selecting this owned log
// envelope; restore it outside the Fluent message. Never trim or split stderr.
fn framed(raw: &str, child: fn(&str) -> Option<String>) -> Option<String> {
    let (body, newline) = raw
        .strip_suffix('\n')
        .map_or((raw, ""), |body| (body, "\n"));
    if !single_line(body) {
        return None;
    }
    child(body).map(|value| format!("{value}{newline}"))
}

fn envelope(id: &str, raw: &str) -> Option<String> {
    match id {
        "interaction-remote-owned-endpoint" => framed(raw, native),
        "interaction-remote-claim-helper" => framed(raw, |body| exact(body, CLAIM)),
        "interaction-remote-claim" => framed(raw, claim),
        "interaction-remote-cleanup-partial" if single_line(raw) => ledger(raw),
        _ => None,
    }
}

// Select the actual outer producer before looking for an inner proof separator.
// Even an unknown child finishes here, keeping all of its bytes opaque instead of
// allowing another top-level display rule to reinterpret the nested payload.
pub(super) fn owned_envelope(raw: &str) -> Option<String> {
    for (prefix, id) in [
        (
            "gitcraft owned endpoint: ",
            "interaction-remote-owned-endpoint",
        ),
        (
            "gitcraft workspace claim: ",
            "interaction-remote-claim-helper",
        ),
        ("workspace claim unavailable: ", "interaction-remote-claim"),
        (
            "cleanup partial; external ledger retained: ",
            "interaction-remote-cleanup-partial",
        ),
    ] {
        if let Some(detail) = raw.strip_prefix(prefix) {
            let child = envelope(id, detail);
            return Some(message(
                id,
                &[("detail", child.as_deref().unwrap_or(detail).into())],
            ));
        }
    }
    None
}

pub(super) fn filesystem(raw: &str) -> Option<String> {
    if !single_line(raw) {
        return None;
    }
    if raw == "not a file" {
        return known(raw);
    }
    if let Some(path) = raw.strip_suffix(" isn't gitcraft's") {
        return Some(message(
            "interaction-remote-not-owned",
            &[("path", path.into())],
        ));
    }
    if let Some(code) = raw.strip_prefix("exit status ")
        && let Ok(value) = code.parse::<i32>()
        && value.to_string() == code
    {
        return Some(message(
            "interaction-remote-exit-status",
            &[("code", value.into())],
        ));
    }
    None
}

fn atomic_proof(error: &str, proof: &str) -> Option<String> {
    let error_value = exact(error, STOP);
    let proof_value = exact(
        proof,
        &[
            "native terminal receipt unavailable",
            "unconfirmed native terminal receipt",
        ],
    );
    if error_value.is_none() && proof_value.is_none() {
        return None;
    }
    Some(pair(
        "interaction-remote-atomic-proof",
        ("detail", error_value.as_deref().unwrap_or(error)),
        ("proof", proof_value.as_deref().unwrap_or(proof)),
    ))
}

pub(super) fn native_proof(id: &str, error: &str, proof: &str) -> Option<String> {
    match id {
        "interaction-remote-atomic-proof" => atomic_proof(error, proof),
        // terminal_receipt returns only opaque OS/serialization errors. Do not
        // translate a lookalike known string in that proof value.
        "interaction-remote-not-dispatched-proof" => {
            native_leaf(error).map(|error| pair(id, ("detail", &error), ("proof", proof)))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::diagnostic;

    // Inert models of exact frozen production compositions, bound to their source
    // templates. No fs::failed, helper, process, request, native or FS operation runs.
    #[test]
    fn actual_owned_native_and_filesystem_compositions_render_in_both_languages() {
        let native_source = concat!(
            include_str!("../native_owner.rs"),
            include_str!("../native_owner/controller.rs")
        );
        let fs_source = include_str!("../fs.rs");
        assert!(native_source.contains("eprintln!(\"gitcraft owned endpoint: {error}\");"));
        assert!(native_source.contains("return Err(\"invalid native request\".into());"));
        assert!(fs_source.contains("String::from_utf8_lossy(&out.stderr).trim().to_owned()"));
        assert!(fs_source.contains("\"could not write a file on {}: {}\""));
        assert!(fs_source.contains("\"could not make a scratch folder on {}: {}\""));
        assert!(fs_source.contains("$d isn't gitcraft's"));
        let host = crate::remote::Host::Ssh("build".into()).label();
        let path = "/tmp/gitcraft-501";
        let stderr = format!("{path} isn't gitcraft's\n");
        let failed = stderr.trim();
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            let error = "invalid native request";
            let emitted = format!("gitcraft owned endpoint: {error}\n");
            assert_eq!(
                diagnostic(&emitted),
                if locale == "de" {
                    "gitcraft-Eigentümerendpunkt: Die native Anfrage ist ungültig.\n"
                } else {
                    &emitted
                }
            );
            assert_eq!(
                diagnostic(emitted.trim_end_matches('\n')),
                if locale == "de" {
                    "gitcraft-Eigentümerendpunkt: Die native Anfrage ist ungültig."
                } else {
                    emitted.trim_end_matches('\n')
                }
            );
            for (raw, expected_de) in [
                (
                    format!("could not write a file on {host}: {failed}"),
                    format!(
                        "Auf {host} konnte keine Datei geschrieben werden: {path} gehört nicht zu gitcraft."
                    ),
                ),
                (
                    format!("could not make a scratch folder on {host}: {failed}"),
                    format!(
                        "Auf {host} konnte kein temporärer Arbeitsordner erstellt werden: {path} gehört nicht zu gitcraft."
                    ),
                ),
                (
                    format!("could not write a file on {host}: exit status 125"),
                    format!("Auf {host} konnte keine Datei geschrieben werden: Exit-Status 125"),
                ),
                (
                    format!("could not write a file on {host}: not a file"),
                    format!(
                        "Auf {host} konnte keine Datei geschrieben werden: Dies ist keine Datei."
                    ),
                ),
            ] {
                assert_eq!(
                    diagnostic(&raw),
                    if locale == "de" { expected_de } else { raw }
                );
            }
        }
    }

    #[test]
    fn finite_owned_proof_cleanup_and_claim_children_render_with_framing() {
        let native = concat!(
            include_str!("../native_owner.rs"),
            include_str!("../native_owner/controller.rs")
        );
        let claim = include_str!("../workspace_claim.rs");
        assert!(native.contains("{error}; atomic cleanup proof unavailable: {proof}"));
        assert!(native.contains("{error}; NotDispatched proof publication failed: {proof}"));
        assert!(native.contains("cleanup partial; external ledger retained: {error}"));
        assert!(claim.contains("eprintln!(\"gitcraft workspace claim: {e}\");"));
        assert!(claim.contains("\"workspace claim unavailable: {}\""));
        let cases = [
            (
                "gitcraft owned endpoint: vendor; atomic cleanup proof unavailable: native terminal receipt unavailable\n",
                "gitcraft-Eigentümerendpunkt: vendor; der atomare Bereinigungsnachweis ist nicht verfügbar: Die native Abschlussbestätigung ist nicht verfügbar.\n",
            ),
            (
                "native owner unavailable; cleanup unknown; atomic cleanup proof unavailable: native terminal receipt unavailable",
                "Der native Eigentümer ist nicht verfügbar; der Bereinigungsstatus ist unbekannt.; der atomare Bereinigungsnachweis ist nicht verfügbar: Die native Abschlussbestätigung ist nicht verfügbar.",
            ),
            (
                "cleanup authority missing; NotDispatched proof publication failed: invalid native request",
                "Die Bereinigungsberechtigung fehlt.; die Veröffentlichung des NotDispatched-Nachweises ist fehlgeschlagen: invalid native request",
            ),
            (
                "cleanup partial; external ledger retained: external ledger directory replaced",
                "Die Bereinigung ist unvollständig; das externe Protokoll bleibt erhalten: Das externe Protokollverzeichnis wurde ersetzt.",
            ),
            (
                "gitcraft owned endpoint: cleanup authority missing; NotDispatched proof publication failed: OS error 5\n",
                "gitcraft-Eigentümerendpunkt: Die Bereinigungsberechtigung fehlt.; die Veröffentlichung des NotDispatched-Nachweises ist fehlgeschlagen: OS error 5\n",
            ),
            (
                "gitcraft workspace claim: invalid workspace claim request\n",
                "gitcraft-Arbeitsbereichsanspruch: Die Arbeitsbereichsanspruchsanfrage ist ungültig.\n",
            ),
            (
                "workspace claim unavailable: gitcraft workspace claim: invalid workspace claim request\n",
                "Der Arbeitsbereichsanspruch ist nicht verfügbar: gitcraft-Arbeitsbereichsanspruch: Die Arbeitsbereichsanspruchsanfrage ist ungültig.\n",
            ),
            (
                "workspace claim unavailable: invalid workspace claim lifetime/root",
                "Der Arbeitsbereichsanspruch ist nicht verfügbar: Nutzungsdauer oder Stammverzeichnis des Arbeitsbereichsanspruchs sind ungültig.",
            ),
        ];
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            for (raw, german) in cases {
                assert_eq!(diagnostic(raw), if locale == "de" { german } else { raw });
            }
        }
    }

    #[test]
    fn unknown_lookalikes_multiline_and_terminal_controls_stay_opaque() {
        for locale in ["en-US", "de"] {
            crate::i18n::set_language(locale);
            for detail in [
                "vendor: invalid native request",
                "not a file", // Known elsewhere, not an owned native-helper child.
                "invalid native request\nsecond line",
                "invalid native request\n\n",
                "invalid native request\r\n",
                "Traceback:\nValueError: invalid native request",
                "vendor: atomic cleanup proof unavailable: native terminal receipt unavailable",
                "invalid native request; NotDispatched proof publication failed: vendor\nsecond line",
            ] {
                let raw = format!("gitcraft owned endpoint: {detail}");
                let prefix = if locale == "de" {
                    "gitcraft-Eigentümerendpunkt: "
                } else {
                    "gitcraft owned endpoint: "
                };
                assert_eq!(diagnostic(&raw), format!("{prefix}{detail}"));
            }
            for detail in [
                "invalid native request", // Known elsewhere, not fs::failed's own output.
                "vendor: no access\n/tmp/gitcraft-501 isn't gitcraft's",
                "exit status 001",
                "/tmp/ä { $count } --force: vendor error\n",
            ] {
                let raw = format!("could not write a file on SSH · build: {detail}");
                let prefix = if locale == "de" {
                    "Auf SSH · build konnte keine Datei geschrieben werden: "
                } else {
                    "could not write a file on SSH · build: "
                };
                assert_eq!(diagnostic(&raw), format!("{prefix}{detail}"));
            }
            // Source-origin restrictions: provider/SSH envelopes never opt into the
            // new native/FS composition rules, even for exactly known text.
            let raw = "could not reach SSH · build: invalid native request";
            assert_eq!(
                diagnostic(raw),
                if locale == "de" {
                    "Die Verbindung zu SSH · build konnte nicht hergestellt werden: invalid native request"
                } else {
                    raw
                }
            );
            let raw = "cleanup partial; external ledger retained: invalid native request";
            assert_eq!(
                diagnostic(raw),
                if locale == "de" {
                    "Die Bereinigung ist unvollständig; das externe Protokoll bleibt erhalten: invalid native request"
                } else {
                    raw
                }
            );
            let raw = "gitcraft workspace claim: invalid native request\n";
            assert_eq!(
                diagnostic(raw),
                if locale == "de" {
                    "gitcraft-Arbeitsbereichsanspruch: invalid native request\n"
                } else {
                    raw
                }
            );
        }
    }
}
