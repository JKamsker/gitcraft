//! Finite human presentation of startup/desktop failures. Wire replies and producers stay raw.
pub(super) fn message(raw: &str) -> Option<String> {
    inner(raw, 0)
}
fn inner(raw: &str, depth: usize) -> Option<String> {
    if depth >= 8 {
        return None;
    }
    let id = match raw {
        "gitcraft is already running, but it doesn't answer. Close it (or end it in the task manager) and start gitcraft again." => {
            Some("interaction-start-error-unresponsive")
        }
        "gitcraft is already running, and this folder's name can't be handed to it: it isn't valid UTF-8." => {
            Some("interaction-start-error-cwd-text")
        }
        "gitcraft is already running, and a name on this command line can't be handed to it: it isn't valid UTF-8. Open it from the running window instead." => {
            Some("interaction-start-error-arg-text")
        }
        "The native event did not contain a supported gitcraft link" => {
            Some("interaction-start-error-native-supported")
        }
        "The native URL event did not contain a gitcraft:// link" => {
            Some("interaction-start-error-native-link")
        }
        "The gitcraft link is too long (maximum 4096 bytes)" => {
            Some("interaction-start-error-native-long")
        }
        "The native URL event had no URL string" => Some("interaction-start-error-native-missing"),
        "Native URL events require the main thread" => {
            Some("interaction-start-error-native-thread")
        }
        "The native URL handler is already installed" => {
            Some("interaction-start-error-native-installed")
        }
        "Open a repository first" => Some("interaction-start-error-open-first"),
        "no version" => Some("interaction-start-error-no-version"),
        "no command line" => Some("interaction-start-error-no-cmd"),
        "the command line isn't text" => Some("interaction-start-error-cmd-text"),
        "no kind" => Some("interaction-start-error-no-kind"),
        "no repository, or not one" => Some("interaction-start-error-no-repo"),
        "not an operation this gitcraft knows" => Some("interaction-start-error-unknown-op"),
        "no path" => Some("interaction-start-error-no-path"),
        "the path is too long" => Some("interaction-start-error-path-long"),
        "the path has control characters" => Some("interaction-start-error-path-control"),
        "not a command line gitcraft takes" => Some("interaction-start-error-cmd-invalid"),
        "the command line has control characters or is too long" => {
            Some("interaction-start-error-cmd-long")
        }
        _ => None,
    };
    if let Some(id) = id {
        return Some(crate::i18n::text(id).to_owned());
    }
    if let Some(value) = raw.strip_prefix("gitcraft can't tell which folder it was started in: ") {
        return Some(crate::i18n::message(
            "interaction-start-error-cwd",
            &[("detail", value.into())],
        ));
    }
    if let Some(value) = raw.strip_suffix(" is not an absolute path") {
        return Some(crate::i18n::message(
            "interaction-start-error-abs-path",
            &[("path", value.into())],
        ));
    }
    if let Some(value)=raw.strip_prefix("gitcraft is already running, and ").and_then(|s|s.strip_suffix(" can't be handed to it: the name isn't valid UTF-8. Open it from the running window instead.")) {return Some(crate::i18n::message("interaction-start-error-path-text",&[("path",value.into())]));}
    if let Some(count) = raw
        .strip_suffix(" Dock actions exceeded the pending-action limit; select them again")
        .and_then(|n| n.parse::<u64>().ok())
    {
        return Some(crate::i18n::message(
            "interaction-start-error-dock-overflow",
            &[("count", count.into())],
        ));
    }
    if let Some(count) = raw
        .strip_suffix(" native links exceeded the pending-link limit; open them again")
        .and_then(|n| n.parse::<u64>().ok())
    {
        return Some(crate::i18n::message(
            "interaction-start-error-link-overflow",
            &[("count", count.into())],
        ));
    }
    if let Some(reason) = raw
        .strip_prefix("The running gitcraft couldn't take this: ")
        .and_then(|s| s.strip_suffix('.'))
    {
        let reason = inner(reason, depth + 1)
            .or_else(|| crate::launch::display_message(reason))
            .unwrap_or_else(|| crate::git::display_error(reason));
        return Some(crate::i18n::message(
            "interaction-start-error-refused",
            &[("reason", reason.into())],
        ));
    }
    if let Some(body) = raw.strip_suffix('.') {
        for (at, _) in body.match_indices(" can't be opened: ") {
            let path = &body[..at];
            let reason = &body[at + " can't be opened: ".len()..];
            if let Some(reason) = inner(reason, depth + 1) {
                return Some(crate::i18n::message(
                    "interaction-start-error-open-path",
                    &[("path", path.into()), ("reason", reason.into())],
                ));
            }
        }
    }
    if let Some((version, other)) = raw
        .strip_prefix("this gitcraft speaks version ")
        .and_then(|s| s.split_once(", not "))
    {
        if [version, other]
            .iter()
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        {
            return Some(crate::i18n::message(
                "interaction-start-error-wire-version",
                &[("version", version.into()), ("other", other.into())],
            ));
        }
    }
    raw.strip_prefix("this gitcraft can't do \"")
        .and_then(|s| s.strip_suffix('"'))
        .map(|kind| {
            crate::i18n::message(
                "interaction-start-error-wire-kind",
                &[("kind", kind.into())],
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_human_display_keeps_wire_reasons_and_opaque_names_while_translating_known_envelopes()
    {
        crate::i18n::set_language("de");
        let cases = [
            (
                "gitcraft is already running, but it doesn't answer. Close it (or end it in the task manager) and start gitcraft again.",
                "gitcraft läuft bereits, antwortet aber nicht. Schließe es (oder beende es im Task-Manager) und starte gitcraft erneut.",
            ),
            (
                "The running gitcraft couldn't take this: the path has control characters.",
                "Das laufende gitcraft konnte die Anfrage nicht übernehmen: der Pfad enthält Steuerzeichen.",
            ),
            (
                "The running gitcraft couldn't take this: this gitcraft speaks version 1, not 99.",
                "Das laufende gitcraft konnte die Anfrage nicht übernehmen: dieses gitcraft verwendet Version 1, nicht 99.",
            ),
            (
                "The running gitcraft couldn't take this: this gitcraft can't do \"opaque 日本語\".",
                "Das laufende gitcraft konnte die Anfrage nicht übernehmen: dieses gitcraft unterstützt \"opaque 日本語\" nicht.",
            ),
            (
                "日本語 can't be opened: no path.",
                "日本語 kann nicht geöffnet werden: kein Pfad.",
            ),
            (
                "relative 日本語 is not an absolute path",
                "relative 日本語 ist kein absoluter Pfad",
            ),
            (
                "gitcraft is already running, and 日本語 can't be handed to it: the name isn't valid UTF-8. Open it from the running window instead.",
                "gitcraft läuft bereits. 日本語 kann nicht übergeben werden, weil der Name kein gültiges UTF-8 ist. Öffne ihn stattdessen im laufenden Fenster.",
            ),
            (
                "gitcraft can't tell which folder it was started in: external 日本語: detail",
                "gitcraft kann den Startordner nicht bestimmen: external 日本語: detail",
            ),
            (
                "2345 Dock actions exceeded the pending-action limit; select them again",
                "2.345 Dock-Aktionen überschritten die Warteschlangengrenze; wähle sie erneut aus",
            ),
            (
                "2345 native links exceeded the pending-link limit; open them again",
                "2.345 native Links überschritten die Warteschlangengrenze; öffne sie erneut",
            ),
            (
                "The native event did not contain a supported gitcraft link",
                "Das native Ereignis enthielt keinen unterstützten gitcraft-Link",
            ),
            (
                "The native URL event did not contain a gitcraft:// link",
                "Das native URL-Ereignis enthielt keinen gitcraft://-Link",
            ),
            (
                "The gitcraft link is too long (maximum 4096 bytes)",
                "Der gitcraft-Link ist zu lang (höchstens 4096 Bytes)",
            ),
            (
                "The native URL event had no URL string",
                "Das native URL-Ereignis enthielt keinen URL-Text",
            ),
            (
                "Native URL events require the main thread",
                "Native URL-Ereignisse erfordern den Anwendungsthread",
            ),
            (
                "The native URL handler is already installed",
                "Der Handler für native URLs ist bereits installiert",
            ),
        ];
        for (raw, german) in cases {
            assert_eq!(super::super::display_start_message(raw), german);
            // The raw source remains the value handed to protocols/state.
            assert_eq!(message(raw).as_deref(), Some(german));
        }
        let unknown = "external 日本語\n: opaque";
        let wrapped = format!("The running gitcraft couldn't take this: {unknown}.");
        assert_eq!(
            super::super::display_start_message(&wrapped),
            format!("Das laufende gitcraft konnte die Anfrage nicht übernehmen: {unknown}.")
        );
        assert_eq!(super::super::display_start_message(unknown), unknown);
        assert_eq!(
            message(
                "native unknown count Dock actions exceeded the pending-action limit; select them again"
            ),
            None
        );
        assert_eq!(
            message("日本語 can't be opened: external unknown reason."),
            None
        );
        let nested = (0..10).fold("Open a repository first".to_owned(), |reason, _| {
            format!("The running gitcraft couldn't take this: {reason}.")
        });
        let displayed = message(&nested).unwrap_or_default();
        assert_eq!(displayed.matches("Das laufende gitcraft").count(), 8);
        assert!(displayed.contains("The running gitcraft couldn't take this:"));
        crate::i18n::set_language("en-US");
        for (raw, _) in cases {
            let expected = raw.replace("2345 ", "2,345 ");
            assert_eq!(super::super::display_start_message(raw), expected);
        }
    }

    #[test]
    fn portable_warning_paint_keeps_cached_raw_mode_and_named_os_detail_without_probing() {
        crate::i18n::set_language("de");
        let raw = "Portable mode is off: gitcraft can't write to C:\\日本語 (literal): folder (external OS detail: opaque). Settings are kept in your user's app data this time.";
        let mode = crate::portable::Mode::Unusable(raw.into());
        assert_eq!(
            super::super::display_start_message(raw),
            "Der portable Modus ist aus: gitcraft kann hier nicht schreiben: C:\\日本語 (literal): folder (external OS detail: opaque). Die Einstellungen werden diesmal in deinen Anwendungsdaten gespeichert."
        );
        assert!(matches!(&mode,crate::portable::Mode::Unusable(value) if value==raw));
        let translocated = "Portable mode is off: macOS runs this downloaded copy from a temporary folder. Run xattr -dr com.apple.quarantine on gitcraft.app, then start it again.";
        let shown = crate::portable::display_message(translocated);
        assert!(
            shown
                .as_ref()
                .is_some_and(|s| s.starts_with("Der portable Modus ist aus: macOS")
                    && s.contains("xattr -dr com.apple.quarantine")
                    && s.contains("gitcraft.app"))
        );
        assert_eq!(
            crate::portable::display_message("Portable mode is off: unknown external reason"),
            None
        );
        crate::i18n::set_language("en-US");
        for original in [raw, translocated] {
            assert_eq!(
                crate::portable::display_message(original).as_deref(),
                Some(original)
            );
        }
        // Native installation still occurs at the original point; only the error presentation changed.
        let main = include_str!("../main.rs");
        assert!(
            main.contains(
                "eframe::Error::AppCreation(app::display_start_message(&message).into())"
            )
        );
        let app = include_str!("mod.rs");
        assert!(app.contains("(false, true) => report_displayed_start(message)"));
    }
}
