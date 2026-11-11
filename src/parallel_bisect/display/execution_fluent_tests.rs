use super::*;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn execution_catalogs_parse_pair_variables_and_render_every_complete_message() -> Result<(), String>
{
    let sources = [
        (
            "en-US",
            include_str!("../../../locales/en-US/interaction-execution.ftl"),
        ),
        (
            "de",
            include_str!("../../../locales/de/interaction-execution.ftl"),
        ),
    ];
    let variables = regex::Regex::new(r"\$([a-zA-Z][a-zA-Z0-9_-]*)").map_err(|e| e.to_string())?;
    let mut contracts = Vec::new();
    for (locale, source) in sources {
        let mut entries: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        let mut at = "";
        for line in source
            .lines()
            .filter(|s| !s.starts_with('#') && !s.is_empty())
        {
            if !line.starts_with(' ') {
                at = line
                    .split_once(" = ")
                    .ok_or("expected top-level message")?
                    .0;
                if entries.insert(at, BTreeSet::new()).is_some() {
                    return Err(format!("duplicate {at}"));
                }
            }
            for capture in variables.captures_iter(line) {
                entries
                    .get_mut(at)
                    .ok_or("missing message")?
                    .insert(capture.get(1).ok_or("missing name")?.as_str());
            }
        }
        let resource = fluent_bundle::FluentResource::try_new(source.to_owned())
            .map_err(|e| format!("{e:?}"))?;
        let mut bundle = fluent_bundle::concurrent::FluentBundle::new_concurrent(vec![
            locale.parse().map_err(|e| format!("{e:?}"))?,
        ]);
        bundle
            .add_resource(resource)
            .map_err(|e| format!("{e:?}"))?;
        for (id, names) in &entries {
            let mut args = fluent_bundle::FluentArgs::new();
            for &name in names {
                if matches!(name, "code" | "status" | "count" | "value") {
                    args.set(name, 2345.25);
                } else {
                    args.set(name, "opaque $HOME {count} · 01234567");
                }
            }
            let pattern = bundle
                .get_message(id)
                .and_then(|m| m.value())
                .ok_or_else(|| format!("missing {id}"))?;
            let mut errors = Vec::new();
            let rendered = bundle.format_pattern(pattern, Some(&args), &mut errors);
            assert!(errors.is_empty(), "{locale} {id}: {errors:?}");
            assert!(!rendered.is_empty(), "{locale} {id}");
        }
        contracts.push(entries);
    }
    assert_eq!(contracts[0], contracts[1]);
    Ok(())
}

#[test]
fn exact_and_single_capture_adapters_render_real_messages_without_touching_details() {
    let opaque = "provider $HOME {code} · refs/heads/me: input\nunknown payload\n";
    for locale in ["en-US", "de"] {
        i18n::set_language(locale);
        for &(raw, id) in EXACT {
            assert_eq!(diagnostic(raw), i18n::text(id));
        }
        for &(prefix, suffix, id, name, _) in DETAIL {
            let raw = format!("{prefix}{opaque}{suffix}");
            let shown = diagnostic(&raw);
            assert!(shown.contains(opaque), "{id}: {shown}");
            assert!(!shown.contains("interaction-bisect-"), "{id}");
            assert!(!shown.contains(&format!("${name}")), "{id}");
        }
        assert_eq!(diagnostic(opaque), opaque);
        assert_eq!(diagnostic("cancelled"), "cancelled");
        assert_eq!(
            diagnostic("process timed out and was stopped"),
            "process timed out and was stopped"
        );
        assert_eq!(
            diagnostic("stdout: Frame output exceeds 2 MiB; no result cached"),
            "stdout: Frame output exceeds 2 MiB; no result cached"
        );
    }
    i18n::set_language("en-US");
}

#[test]
fn supplied_multi_value_diagnostics_preserve_opaque_payloads_and_use_numeric_values() {
    i18n::set_language("de");
    let detail = "provider {code} $HOME: ä\ntrailing output\n";
    let cases = [
        format!("could not check out 01234567 in a worktree on SSH · host: {detail}"),
        format!(
            "The worktrees on SSH · host couldn't all be removed ({detail}); recovery evidence was retained. Resolve the reported ownership or removal failure before trying again."
        ),
        format!("could not make a repository for the bisect on SSH · host: {detail}"),
        format!("Contained Git failed (2345): {detail}"),
        format!("Contained Git failed (no exit code): {detail}"),
        format!("Ref publication completed, but quarantine cleanup failed: {detail}"),
        format!("Verification completed, but quarantine cleanup failed: {detail}"),
    ];
    for raw in cases {
        let shown = diagnostic(&raw);
        assert!(shown.contains(detail), "{shown}");
        assert_ne!(shown, raw);
        assert!(!shown.contains("interaction-bisect-"));
    }
    let raw = format!("{detail}; owned pack-01234567.keep retained");
    let shown = cleanup_diagnostic(
        &raw,
        CleanupContext::BundleKeepRetained {
            primary: detail,
            hash: "01234567",
        },
    );
    assert!(shown.contains(detail), "{shown}");
    assert_ne!(shown, raw);
    assert!(!shown.contains("interaction-bisect-"));
    for raw in [
        "The test of 01234567 ended with exit code 2345 (a crash or a signal): it counts as skipped.",
        "ended its test with exit code 2345; 128 and more (a crash or a signal) stops a bisect, as with git bisect run",
        "bisect shell ended with status Some(2345)",
    ] {
        assert!(diagnostic(raw).contains("2.345"));
    }
    assert_eq!(
        diagnostic("bisect shell ended with status None"),
        "Die Bisektions-Shell wurde ohne Exit-Code beendet."
    );
    assert_eq!(scalar(1234.5), "1.234,5");
    assert_eq!(machine_name("this computer"), "dieser Computer");
    assert_eq!(
        machine_label(&super::super::Machine::Here),
        "dieser Computer"
    );
    assert_eq!(machine_name("SSH · host"), "SSH · host");
    assert!(diagnostic("Setting up on this computer…").contains("diesem Computer"));
    assert_eq!(verdict(super::super::Verdict::Good), "gut");
    assert_eq!(super::super::Verdict::Good.word(), "good");
    assert_eq!(super::super::Machine::Here.label(), "this computer");
    assert_eq!(
        diagnostic(
            "The test of a ended with exit code opaque (a crash or a signal): it counts as skipped."
        ),
        "The test of a ended with exit code opaque (a crash or a signal): it counts as skipped."
    );
    let observation = diagnostic(&format!(
        "Benchmark must print one finite number (exit 2345); stderr: {detail}"
    ));
    assert!(observation.contains("genau eine endliche Zahl"));
    assert!(observation.contains("2.345"));
    assert!(observation.contains(detail));
    let output = format!(
        "01234567 ended its test with exit code 2345; 128 and more (a crash or a signal) stops a bisect, as with git bisect run\n{detail}"
    );
    assert!(diagnostic(&output).contains(detail));
    let warning = warning("01234567: could not read stopped test log: native payload");
    assert!(
        warning.contains("01234567")
            && warning.contains("konnte nicht gelesen")
            && warning.contains("native payload")
    );
    i18n::set_language("en-US");
}
