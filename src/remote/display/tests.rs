use super::*;

// Pure display-only tests: no App/RepoTab, environment discovery, subprocesses,
// sockets, filesystem fixtures, credential store, Git, timers or native helpers.
#[test]
fn raw_failure_and_pool_producers_stay_canonical_in_both_languages() {
    let host = super::super::Host::Ssh("me@build:2222".into());
    for locale in ["en-US", "de"] {
        crate::i18n::set_language(locale);
        assert_eq!(host.label(), "SSH · me@build:2222");
        let confirmed = super::super::shell::Failure::TimedOut(true).message();
        assert_eq!(
            confirmed,
            "it took too long; the owned supervisor confirmed stop and reap"
        );
        let unknown = super::super::shell::Failure::TimedOut(false).message();
        assert_eq!(
            unknown,
            "it took too long; cleanup is unknown and recovery evidence was retained"
        );
        let busy = super::super::pool::Lane::Tool.busy(&host);
        assert_eq!(
            busy,
            "4 diff or merge tools are already open on SSH · me@build:2222: close one of them first"
        );
        if locale == "de" {
            assert_eq!(
                diagnostic(&confirmed),
                "Der Vorgang dauerte zu lange; der eigene Supervisor hat das Beenden und Einlesen des Prozessstatus bestätigt."
            );
            assert_eq!(
                diagnostic(&unknown),
                "Der Vorgang dauerte zu lange; der Bereinigungsstatus ist unbekannt und die Wiederherstellungsnachweise bleiben erhalten."
            );
            assert_eq!(
                diagnostic(&busy),
                "Auf SSH · me@build:2222 sind bereits 4 Diff- oder Merge-Werkzeuge geöffnet: Schließen Sie zuerst eines davon."
            );
        } else {
            assert_eq!(diagnostic(&confirmed), confirmed);
            assert_eq!(diagnostic(&unknown), unknown);
            assert_eq!(diagnostic(&busy), busy);
        }
    }
}

#[test]
fn notices_and_typed_numeric_values_use_actual_fluent_rendering() {
    crate::i18n::set_language("de");
    assert_eq!(
        diagnostic("No saved sign-ins to forget."),
        "Es gibt keine gespeicherten Anmeldungen zum Vergessen."
    );
    assert_eq!(
        diagnostic("Forgot 1 saved sign-in."),
        "Eine gespeicherte Anmeldung wurde vergessen."
    );
    assert_eq!(
        diagnostic("Forgot 2345 saved sign-ins."),
        "2.345 gespeicherte Anmeldungen wurden vergessen."
    );
    assert_eq!(
        diagnostic("workspace recovery exceeds 4096 owners"),
        "Die Wiederherstellung des Arbeitsbereichs überschreitet die Grenze von 4.096 Eigentümern."
    );
    assert_eq!(diagnostic("exit status 125"), "Exit-Status 125");
    for malformed in [
        "Forgot 01 saved sign-ins.",
        "Forgot -1 saved sign-ins.",
        "exit status 001",
        "exit status NaN",
    ] {
        assert_eq!(diagnostic(malformed), malformed);
    }
    crate::i18n::set_language("en-US");
    assert_eq!(
        diagnostic("Forgot 2345 saved sign-ins."),
        "Forgot 2,345 saved sign-ins."
    );
}

#[test]
fn known_envelopes_keep_external_payloads_and_identities_opaque() {
    crate::i18n::set_language("de");
    let detail = "permission denied: /tmp/ä { $count } --force\nunknown device error";
    let raw = format!("could not reach SSH · me@build:2222: {detail}");
    assert_eq!(
        diagnostic(&raw),
        format!("Die Verbindung zu SSH · me@build:2222 konnte nicht hergestellt werden: {detail}")
    );
    assert_eq!(diagnostic(detail), detail);
    let external = "Traceback (most recent call last):\nValueError: invalid owner fields";
    assert_eq!(diagnostic(external), external);
    assert_eq!(
        diagnostic("user said invalid owner fields"),
        "user said invalid owner fields"
    );
    assert_eq!(
        diagnostic("gitcraft: no such folder: /home/ä/--force"),
        "gitcraft: Dieser Ordner ist nicht vorhanden: /home/ä/--force"
    );
    assert_eq!(
        diagnostic("gitcraft: invalid ownership handshake\n"),
        "gitcraft: Der Eigentümer-Handshake ist ungültig.\n"
    );
    assert_eq!(
        python_exception("ValueError", "invalid owner fields"),
        "ValueError: Die Felder des Eigentümers sind ungültig."
    );
    assert_eq!(
        python_exception("OSError", detail),
        format!("OSError: {detail}")
    );
    assert_eq!(
        perl_exception("invalid owner fields", "-e line 19."),
        "Die Felder des Eigentümers sind ungültig. at -e line 19."
    );
    let host = super::super::Host::Ssh("build".into());
    let command = "git -c x=' on y: z' status";
    let raw = format!("{command} on SSH · build: remote shell unavailable");
    assert_eq!(
        execution_error(&raw, command, &host),
        format!("{command} auf SSH · build: Die Remote-Shell ist nicht verfügbar.")
    );
    assert_eq!(execution_error(&raw, "wrong", &host), raw);
}

#[test]
fn every_known_message_and_envelope_has_valid_paired_contracts_without_fallback()
-> Result<(), String> {
    use fluent_bundle::{FluentArgs, FluentResource, concurrent::FluentBundle};
    use std::collections::{BTreeMap, BTreeSet};
    let mut contracts = Vec::new();
    for (locale, source) in [
        (
            "en-US",
            include_str!("../../../locales/en-US/interaction-remote.ftl"),
        ),
        (
            "de",
            include_str!("../../../locales/de/interaction-remote.ftl"),
        ),
    ] {
        let resource =
            FluentResource::try_new(source.to_owned()).map_err(|(_, e)| format!("{e:?}"))?;
        let mut bundle =
            FluentBundle::new_concurrent(vec![locale.parse().map_err(|e| format!("{e}"))?]);
        bundle.set_use_isolating(false);
        bundle
            .add_resource(resource)
            .map_err(|e| format!("{e:?}"))?;
        let mut variables = BTreeMap::<&str, BTreeSet<&str>>::new();
        let mut current = "";
        for line in source.lines() {
            if line.starts_with("interaction-remote-") {
                current = line.split_once('=').ok_or("missing equals")?.0.trim();
                if variables.insert(current, BTreeSet::new()).is_some() {
                    return Err(format!("duplicate ID {current}"));
                }
            }
            for after in line.split('$').skip(1) {
                let end = after
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(after.len());
                variables.entry(current).or_default().insert(&after[..end]);
            }
        }
        for (id, names) in &variables {
            let mut args = FluentArgs::new();
            for name in names {
                if ["count", "limit", "code"].contains(name) {
                    args.set(*name, 2345);
                } else {
                    args.set(*name, "/opaque/ä --force { $count }");
                }
            }
            let mut errors = Vec::new();
            let pattern = bundle
                .get_message(id)
                .and_then(|m| m.value())
                .ok_or_else(|| format!("missing {id}"))?;
            let rendered = bundle.format_pattern(pattern, Some(&args), &mut errors);
            assert!(errors.is_empty(), "{locale} {id}: {errors:?}");
            assert!(!rendered.is_empty());
        }
        for &(raw, id, limit) in catalog::entries() {
            let expected: BTreeSet<_> = if limit.is_some() {
                BTreeSet::from(["limit"])
            } else {
                BTreeSet::new()
            };
            assert_eq!(variables.get(id), Some(&expected), "{raw}");
        }
        for &(_, _, id, name) in ENVELOPES {
            assert_eq!(variables.get(id), Some(&BTreeSet::from([name])));
        }
        contracts.push(variables);
    }
    assert_eq!(contracts[0], contracts[1]);
    Ok(())
}
