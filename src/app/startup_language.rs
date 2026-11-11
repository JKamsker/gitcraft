//! Early presentation only: do not construct/apply Settings or initialize connection profiles.
use std::collections::HashMap;

/// Select the saved language (or OS preferences) before command help/file-tool UI exists.
/// Only main's presentation paths call this; endpoint, credentials, askpass, MCP and hooks
/// retain their earlier exits. This reads existing state and never creates/probes storage.
pub fn initialize_language() {
    let source =
        crate::portable::startup_state_path().and_then(|path| std::fs::read_to_string(path).ok());
    initialize_from_saved(source.as_deref());
}

fn initialize_from_saved(source: Option<&str>) {
    let selected = source.and_then(saved_language).unwrap_or_default();
    crate::i18n::set_language(&selected);
}

/// Project just one field out of eframe's key -> serialized Saved map. Unknown fields are
/// ignored: no Settings defaults, profile normalization, auth or native hooks are invoked.
fn saved_language(source: &str) -> Option<String> {
    #[derive(Default, serde::Deserialize)]
    #[serde(default)]
    struct LanguageOnly {
        language: String,
    }
    #[derive(Default, serde::Deserialize)]
    #[serde(default)]
    struct SavedLanguage {
        settings: LanguageOnly,
    }
    let kv: HashMap<String, String> = ron::from_str(source).ok()?;
    let saved: SavedLanguage = ron::from_str(kv.get(eframe::APP_KEY)?).ok()?;
    Some(saved.settings.language)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(saved: &str) -> Result<String, String> {
        ron::to_string(&HashMap::from([(eframe::APP_KEY, saved)])).map_err(|e| e.to_string())
    }

    #[test]
    fn saved_language_projects_only_the_language_and_drives_actual_early_renderers()
    -> Result<(), String> {
        // Other fields deliberately cannot deserialize as Settings. Language-only projection
        // must not need profile/auth deserialization, constructors or Settings::set.
        let source = state(
            r#"(settings:(language:"de-AT",user_profiles:"opaque foreign state"),theme:"future theme",open:["$() ü"] )"#,
        )?;
        assert_eq!(saved_language(&source).as_deref(), Some("de-AT"));
        initialize_from_saved(Some(&source));
        assert_eq!(crate::i18n::language(), crate::i18n::Language::German);
        assert!(crate::launch::args::usage().starts_with("Aufruf:"));
        assert!(crate::standalone::usage().starts_with("Verwendung:"));
        assert_eq!(crate::standalone::window_title(), "GitCraft-Dateiwerkzeuge");
        for literal in [
            "gitcraft --diff [--] BEFORE AFTER",
            "gitcraft --merge [--] BASE LOCAL REMOTE MERGED",
        ] {
            assert!(crate::standalone::usage().contains(literal));
        }
        for literal in [
            "gitcraft log <file>",
            "gitcraft --quit",
            "gitcraft://open?url=<link>",
        ] {
            assert!(crate::launch::args::usage().contains(literal));
        }
        let english = state(r#"(settings:(language:"en-US"))"#)?;
        initialize_from_saved(Some(&english));
        assert_eq!(crate::standalone::usage(), crate::standalone::USAGE);
        assert_eq!(crate::launch::args::usage(), crate::launch::args::USAGE);
        assert_eq!(crate::standalone::window_title(), "GitCraft file tools");
        Ok(())
    }

    #[test]
    fn missing_legacy_corrupt_and_unsupported_state_use_the_existing_locale_policy()
    -> Result<(), String> {
        for source in [None, Some("broken"), Some("{}")] {
            assert_eq!(source.and_then(saved_language), None);
            initialize_from_saved(source);
            assert_eq!(crate::i18n::language(), crate::i18n::Language::English);
        }
        for saved in [
            "()",
            "(settings:())",
            r#"(settings:(language:"xx-unknown"))"#,
        ] {
            let source = state(saved)?;
            initialize_from_saved(Some(&source));
            assert_eq!(crate::i18n::language(), crate::i18n::Language::English);
        }
        // Native preference access is held. These are the production resolver's recorded
        // preferences, including a supported parent language after an unsupported first one.
        assert_eq!(
            crate::i18n::resolve("", &["fr-FR".into(), "de_AT.UTF-8".into()]),
            crate::i18n::Language::German
        );
        assert_eq!(
            crate::i18n::resolve("en-US", &["de".into()]),
            crate::i18n::Language::English
        );
        Ok(())
    }

    #[test]
    fn actual_main_keeps_helper_and_version_exits_before_normal_presentation_initialization()
    -> Result<(), String> {
        // Source-order regression: running main would execute native/protocol helpers. Verify
        // actual call placement without invoking main or reconstructing helper protocols.
        let main = include_str!("../main.rs");
        let calls: Vec<_> = main
            .match_indices("app::initialize_language();")
            .map(|(at, _)| at)
            .collect();
        let [help, normal] = calls.as_slice() else {
            return Err("expected exactly help and normal presentation locale calls".into());
        };
        let at = |needle: &str| {
            main.find(needle)
                .ok_or_else(|| format!("missing source seam {needle}"))
        };
        assert!(at("gitcraft::owned_endpoint_helper()")? < *help);
        assert!(*help < at("gitcraft::standalone::usage()")?);
        for helper in [
            "gitcraft::connections::credential_helper()",
            "app::askpass_helper()",
            "gitcraft::mcp::main(",
            "app::checkpoint_cli()",
        ] {
            let position = at(helper)?;
            assert!(*help < position && position < *normal);
        }
        assert!(at("println!(\"gitcraft {}\"")? < *normal);
        assert!(*normal < at("gitcraft::standalone::parse(")?);
        assert!(*normal < at("gitcraft::standalone::window_title()")?);
        assert!(*normal < at("gitcraft::launch::parse(")?);
        Ok(())
    }
}
