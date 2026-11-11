#[test]
fn guard_display_every_catalog_contract_formats_in_both_languages_without_fallback()
-> Result<(), String> {
    use fluent_bundle::{FluentArgs, FluentBundle, FluentResource};
    use std::collections::{BTreeMap, BTreeSet};
    let mut paired = Vec::new();
    for (locale, catalog) in [
        (
            "en-US",
            include_str!("../../../../locales/en-US/interaction-guards.ftl"),
        ),
        (
            "de",
            include_str!("../../../../locales/de/interaction-guards.ftl"),
        ),
    ] {
        let mut contracts: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        let mut current = "";
        for line in catalog.lines() {
            if let Some((id, _)) = line.split_once(" = ") {
                assert!(id.starts_with("interaction-guards-"));
                current = id;
                assert!(contracts.insert(id, BTreeSet::new()).is_none());
            }
            for suffix in line.split('$').skip(1) {
                let name: String = suffix
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                contracts
                    .get_mut(current)
                    .ok_or("missing test value")?
                    .insert(name);
            }
        }
        let mut bundle =
            FluentBundle::new(vec![locale.parse().map_err(|error| format!("{error:?}"))?]);
        bundle.set_use_isolating(false);
        bundle
            .add_resource(
                FluentResource::try_new(catalog.to_owned())
                    .map_err(|error| format!("{error:?}"))?,
            )
            .map_err(|error| format!("{error:?}"))?;
        // Direct bundles make a broken German message fail instead of using English fallback.
        for (id, names) in &contracts {
            for (number, selector) in [(1u64, "yes"), (12345u64, "no")] {
                let mut args = FluentArgs::new();
                for name in names {
                    match name.as_str() {
                        "line" | "count" | "others" | "seconds" | "megabytes" | "percent"
                        | "agent" | "total" | "commits" | "edited" | "mixed" | "unknown"
                        | "files" | "unrelated_files" | "folders" | "dependencies"
                        | "signatures" | "removed" | "old" => args.set(name, number),
                        "has_agent" | "has_mixed" | "has_unknown" | "partial" | "unrelated" => {
                            args.set(name, selector)
                        }
                        _ => args.set(name, "opaque 12345 { $untouched }"),
                    }
                }
                let mut errors = Vec::new();
                let shown = bundle.format_pattern(
                    bundle
                        .get_message(id)
                        .ok_or("missing test value")?
                        .value()
                        .ok_or("missing test value")?,
                    Some(&args),
                    &mut errors,
                );
                assert!(errors.is_empty(), "{locale} {id}: {errors:?}");
                assert!(!shown.is_empty(), "{locale} {id}");
            }
        }
        paired.push(contracts);
    }
    assert_eq!(paired[0], paired[1]);

    Ok(())
}

#[test]
fn guard_display_target_labels_preserve_ambiguous_paths_and_canonical_redaction() {
    let literal_local = "[sensitive path]";
    let literal_ssh = "example.invalid:[sensitive path]";
    let secret_local = "/owned/ghp_abcdefghijklmnop/repo";
    let secret_ssh = "example.invalid:ghp_abcdefghijklmnop/repo";
    let stored_local = "Local repository: [sensitive path]";
    let stored_remote = "example.invalid/[sensitive path]";
    assert_eq!(
        crate::push_preflight::target_label(literal_local),
        stored_local
    );
    assert_eq!(
        crate::push_preflight::target_label(literal_ssh),
        stored_remote
    );
    assert_eq!(
        crate::push_preflight::target_label(secret_local),
        stored_local
    );
    assert_eq!(
        crate::push_preflight::target_label(secret_ssh),
        stored_remote
    );
    for (locale, local_caption) in [
        ("en-US", "Local repository: [sensitive path]"),
        ("de", "Lokales Repository: [sensitive path]"),
    ] {
        set_language(locale);
        // Stored labels have no redaction provenance. Literal and genuine redactions
        // deliberately preserve the same canonical technical marker.
        for raw in [literal_local, secret_local] {
            assert_eq!(target(raw), local_caption);
        }
        for raw in [literal_ssh, secret_ssh] {
            assert_eq!(target(raw), stored_remote);
        }
        assert_eq!(target_label(stored_local), local_caption);
        assert_eq!(target_label(stored_remote), stored_remote);
        let raw_finding =
            format!("Author email differs from the profile policy for {stored_local}.");
        assert!(diagnostic(&raw_finding).contains(local_caption));
        assert!(!target(secret_local).contains("ghp_"));
        assert!(!target(secret_ssh).contains("ghp_"));
        // Path-only projection has the actual source and can localize real redaction.
        assert_eq!(path(literal_local), literal_local);
        assert_eq!(
            path(secret_local),
            if locale == "de" {
                "[sensibler Pfad]"
            } else {
                "[sensitive path]"
            }
        );
    }
    set_language("en-US");
}
