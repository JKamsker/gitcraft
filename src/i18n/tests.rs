use super::*;

#[test]
fn embedded_catalogs_parse_without_duplicates_and_have_matching_message_ids() -> Result<(), String>
{
    for (language, sources) in [(Language::English, ENGLISH), (Language::German, GERMAN)] {
        Catalog::load(language, sources)?;
    }
    let ids = |sources: &[(&'static str, &'static str)]| {
        sources
            .iter()
            .flat_map(|(_, s)| message_ids(s))
            .collect::<BTreeSet<_>>()
    };
    assert_eq!(ids(ENGLISH), ids(GERMAN));
    // Keep each named interpolation contract aligned, including plural selectors.
    let variables = |sources: &[(&'static str, &'static str)]| {
        let mut result: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
        for (_, source) in sources {
            let mut at = "";
            for line in source.lines() {
                if let Some(id) = message_ids(line).next() {
                    at = id;
                }
                let entry = result.entry(at).or_default();
                for suffix in line.split('$').skip(1) {
                    let name: String = suffix
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                        .collect();
                    if !name.is_empty() {
                        entry.insert(name);
                    }
                }
            }
        }
        result
    };
    assert_eq!(variables(ENGLISH), variables(GERMAN));
    assert!(Catalog::load(Language::English, &[("bad.ftl", "broken = {")]).is_err());
    assert!(Catalog::load(Language::English, &[("a.ftl", "same = A\nsame = B")]).is_err());
    Ok(())
}

#[test]
fn locale_resolution_uses_preferences_parent_languages_and_explicit_fallback() {
    assert_eq!(
        resolve("", &["fr-CH".into(), "de_AT.UTF-8".into(), "en-US".into()]),
        Language::German
    );
    assert_eq!(resolve("en_GB", &["de".into()]), Language::English);
    assert_eq!(resolve("de-CH", &[]), Language::German);
    assert_eq!(resolve("unknown", &["de".into()]), Language::English);
    assert_eq!(resolve("", &["ja-JP".into()]), Language::English);
}

#[test]
fn actual_fluent_plural_interpolation_number_and_date_render_in_both_languages()
-> Result<(), String> {
    set_language("en-US");
    assert_eq!(
        message("foundation-repositories", &[("count", 1.into())]),
        "Watching one repository"
    );
    assert_eq!(
        message("foundation-repositories", &[("count", 2345.into())]),
        "Watching 2,345 repositories"
    );
    assert_eq!(number(1234.5), "1,234.5");
    assert_eq!(date_iso("2024-02-29"), "02/29/2024");
    assert_eq!(date_iso("2023-02-29"), "2023-02-29");
    set_language("de");
    assert_eq!(
        message("foundation-repositories", &[("count", 1.into())]),
        "Ein Repository wird überwacht"
    );
    assert_eq!(
        message("foundation-repositories", &[("count", 2345.into())]),
        "2.345 Repositorys werden überwacht"
    );
    assert_eq!(number(1234.5), "1.234,5");
    assert_eq!(date_iso("2024-02-29"), "29.02.2024");
    let german = Catalog::load(
        Language::German,
        &[(
            "number.ftl",
            "fixed = { NUMBER($n, minimumFractionDigits: 2, maximumFractionDigits: 2) }",
        )],
    )?;
    assert_eq!(
        german.format("fixed", &[("n", 1234.567.into())]).as_deref(),
        Some("1.234,57")
    );
    Ok(())
}

#[test]
fn missing_selected_messages_and_format_errors_have_explicit_english_fallback() -> Result<(), String>
{
    let english = Catalog::load(
        Language::English,
        &[("en.ftl", "only-english = Available\nitem = Item { $name }")],
    )?;
    let german = Catalog::load(
        Language::German,
        &[("de.ftl", "item = Element { $missing }")],
    )?;
    let format = |id, args: &[(&str, Value<'_>)]| {
        german.format(id, args).or_else(|| english.format(id, args))
    };
    assert_eq!(format("only-english", &[]).as_deref(), Some("Available"));
    assert_eq!(
        format("item", &[("name", "branch".into())]).as_deref(),
        Some("Item branch")
    );
    assert_eq!(format("nonexistent", &[]), None);
    set_language("de");
    assert_eq!(text("untranslated-stable-id"), "untranslated-stable-id");
    assert_eq!(
        message("untranslated-stable-id", &[]),
        "untranslated-stable-id"
    );
    Ok(())
}

#[test]
fn persisted_language_is_backward_compatible_and_real_menu_uses_current_catalog()
-> Result<(), String> {
    let legacy: crate::settings::Settings =
        serde_json::from_str("{}").map_err(|e| e.to_string())?;
    assert!(legacy.language.is_empty());
    let unknown: crate::settings::Settings =
        serde_json::from_str(r#"{"language":"xx-unknown"}"#).map_err(|e| e.to_string())?;
    let roundtrip: crate::settings::Settings =
        serde_json::from_str(&serde_json::to_string(&unknown).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    assert_eq!(roundtrip.language, "xx-unknown");
    set_language("en-US");
    let old_generation = generation();
    assert!(set_language("de"));
    assert!(generation() > old_generation);
    assert!(!set_language("de-AT"));
    let menu = crate::os_shell::menu_items(&[]);
    assert_eq!(menu[0].title, "Klonen…");
    assert_eq!(menu[0].args, ["--clone"]);
    assert_eq!(menu[1].title, "Öffnen…");
    assert_eq!(menu[1].args, ["--open"]);
    assert_eq!(text("settings-language-label"), "Sprache der Oberfläche");
    set_language("xx-unknown");
    assert_eq!(crate::os_shell::menu_items(&[])[0].title, "Clone…");
    Ok(())
}
