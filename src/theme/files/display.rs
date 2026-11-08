//! File-theme presentation only. Parsers, stored credits and I/O errors remain raw.
pub(crate) fn diagnostic(raw: &str) -> String {
    let id = match raw {
        "not a Base16 or Base24 scheme" => Some("interaction-theme-file-base16"),
        "not readable JSON" => Some("interaction-theme-file-json"),
        "a broken VS Code theme" => Some("interaction-theme-file-vscode"),
        "not a .json or .yaml file" => Some("interaction-theme-file-extension"),
        "no base theme" => Some("interaction-theme-file-base"),
        "no folder to keep themes in" => Some("interaction-theme-file-folder"),
        _ => None,
    };
    if let Some(id) = id {
        return crate::i18n::text(id).to_owned();
    }
    for (prefix, suffix, id, argument) in [
        (
            "no theme “",
            "” to extend",
            "interaction-theme-file-extends",
            "name",
        ),
        (
            "“",
            "” is not a colour like #1e1e2e",
            "interaction-theme-file-colour",
            "key",
        ),
        (
            "no colour called ",
            " (see the keys of a saved copy)",
            "interaction-theme-file-unknown",
            "keys",
        ),
    ] {
        if let Some(value) = raw
            .strip_prefix(prefix)
            .and_then(|s| s.strip_suffix(suffix))
        {
            return crate::i18n::message(id, &[(argument, value.into())]);
        }
    }
    // These two keys are supplied by from_own, never guessed from arbitrary diagnostics.
    for key in ["ansi", "lanes"] {
        for (suffix, id) in [
            (" is not a list of colours", "interaction-theme-file-list"),
            (
                " has something that is not a colour",
                "interaction-theme-file-item",
            ),
        ] {
            if raw == format!("“{key}”{suffix}") {
                return crate::i18n::message(id, &[("key", key.into())]);
            }
        }
    }
    if let Some(actual) = raw
        .strip_prefix("“ansi” needs 16 colours, has ")
        .and_then(|s| s.parse::<usize>().ok().filter(|n| n.to_string() == s))
    {
        return crate::i18n::message(
            "interaction-theme-file-length",
            &[
                ("key", "ansi".into()),
                ("needed", 16.into()),
                ("actual", actual.into()),
            ],
        );
    }
    // Native/third-party error details have no inferred theme context.
    crate::git::display_error(raw)
}

pub(crate) fn credit(raw: &str) -> String {
    for (kind, id, with_author) in [
        (
            "Base16 scheme",
            "interaction-theme-file-credit-base16",
            "interaction-theme-file-credit-base16-author",
        ),
        (
            "VS Code theme",
            "interaction-theme-file-credit-vscode",
            "interaction-theme-file-credit-vscode-author",
        ),
        (
            "Your theme",
            "interaction-theme-file-credit-own",
            "interaction-theme-file-credit-own-author",
        ),
    ] {
        if raw == kind {
            return crate::i18n::text(id).to_owned();
        }
        if let Some(author) = raw.strip_prefix(kind).and_then(|s| s.strip_prefix(" · ")) {
            return crate::i18n::message(with_author, &[("author", author.into())]);
        }
    }
    raw.to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn actual_parser_errors_render_in_both_languages_without_changing_validation() {
        let cases = [
            (json!({"extends":"User “色” {$x}"}), "kein Farbschema"),
            (
                json!({"colors":{"custom “色”":"not-a-colour"}}),
                "ist keine Farbe",
            ),
            (
                json!({"colors":{"custom “色”":"#123456"}}),
                "keine Farbe namens",
            ),
            (json!({"lanes":"red"}), "keine Liste"),
            (json!({"ansi":["not-a-colour"]}), "keine Farbe ist"),
            (json!({"ansi":["#123456"]}), "benötigt 16 Farben"),
        ];
        for (input, de) in cases {
            let raw = super::super::from_own(&input).err().unwrap_or_default();
            assert!(!raw.is_empty());
            crate::i18n::set_language("de");
            assert!(diagnostic(&raw).contains(de), "{}", diagnostic(&raw));
            crate::i18n::set_language("en-US");
            assert_eq!(diagnostic(&raw), raw);
            assert_eq!(
                super::super::from_own(&input).err().as_deref(),
                Some(raw.as_str())
            );
        }
    }

    #[test]
    fn finite_file_and_credit_templates_keep_unknowns_and_author_values_opaque() {
        crate::i18n::set_language("de");
        for raw in [
            "not a Base16 or Base24 scheme",
            "not readable JSON",
            "a broken VS Code theme",
            "not a .json or .yaml file",
            "no base theme",
            "no folder to keep themes in",
        ] {
            assert_ne!(diagnostic(raw), raw);
        }
        assert_eq!(
            credit("VS Code theme · Name · {$x} 色"),
            "VS-Code-Farbschema · Name · {$x} 色"
        );
        assert_eq!(credit("Base16 scheme"), "Base16-Farbschema");
        assert_eq!(credit("Your theme"), "Eigenes Farbschema");
        for raw in [
            "third-party provider: not readable JSON",
            "“other” is not a list of colours",
            "“ansi” needs 16 colours, has 01",
            "original credit · Author",
        ] {
            assert_eq!(diagnostic(raw), raw);
            assert_eq!(credit(raw), raw);
        }
    }
}
