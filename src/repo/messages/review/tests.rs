use super::*;
use crate::repo::messages::{display_operation, display_status};

#[test]
fn review_finite_map_all_entries_render_in_both_locales_without_mutating_raw() {
    let fixed_expected: &[(&str, &str, &str)] = &[
        (
            "Add 2–6 repositories to the release board",
            "Add 2–6 repositories to the release board",
            "Füge dem Release-Board 2–6 Repositorys hinzu",
        ),
        (
            "Component names must be unique ASCII identifiers, ignoring case",
            "Component names must be unique ASCII identifiers, ignoring case",
            "Komponentennamen müssen eindeutige ASCII-Kennungen sein, unabhängig von Groß- und Kleinschreibung",
        ),
        (
            "Use absolute repository paths or encoded SSH/WSL paths",
            "Use absolute repository paths or encoded SSH/WSL paths",
            "Verwende absolute Repository-Pfade oder kodierte SSH-/WSL-Pfade",
        ),
        (
            "Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host",
            "Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host",
            "Erprobe verknüpfte Repositorys auf demselben Rechner; wähle lokale Kopien oder Pfade auf einem gemeinsamen SSH-/WSL-Host",
        ),
        (
            "Too many combinations",
            "Too many combinations",
            "Zu viele Kombinationen",
        ),
        (
            "More than 256 in-between combinations: split this release into smaller boards",
            "More than 256 in-between combinations: split this release into smaller boards",
            "Mehr als 256 Zwischenkombinationen: Teile dieses Release in kleinere Boards auf",
        ),
        (
            "A rule names an unknown dependent component",
            "A rule names an unknown dependent component",
            "Eine Regel nennt eine unbekannte abhängige Komponente",
        ),
        (
            "A rule names an unknown prerequisite component",
            "A rule names an unknown prerequisite component",
            "Eine Regel nennt eine unbekannte vorausgesetzte Komponente",
        ),
        (
            "Rule stage thresholds must name non-deployed stages present on their components",
            "Rule stage thresholds must name non-deployed stages present on their components",
            "Stufenschwellen der Regeln müssen vorhandene, noch nicht ausgelieferte Stufen der jeweiligen Komponenten bezeichnen",
        ),
        (
            "Stopped before constructing the full combination",
            "Stopped before constructing the full combination",
            "Vor dem Aufbau der vollständigen Kombination gestoppt",
        ),
        (
            "Enter and explicitly trust your compatibility command before rehearsal",
            "Enter and explicitly trust your compatibility command before rehearsal",
            "Gib vor der Probe deinen Kompatibilitätsbefehl ein und bestätige ausdrücklich dein Vertrauen",
        ),
        (
            "Not tested: rehearsal stopped",
            "Not tested: rehearsal stopped",
            "Nicht getestet: Probe gestoppt",
        ),
        (
            "Changes moved since this page was loaded; reload before applying a hunk",
            "Changes moved since this page was loaded; reload before applying a hunk",
            "Die Änderungen haben sich seit dem Laden dieser Seite verschoben; lade vor dem Anwenden eines Hunks neu",
        ),
        ("Review hunk", "Review hunk", "Review-Hunk"),
        (
            "Hunk submitted. Reload after the operation finishes.",
            "Hunk submitted. Reload after the operation finishes.",
            "Hunk übermittelt. Lade nach Abschluss der Operation neu.",
        ),
        (
            "operation could not start",
            "operation could not start",
            "Operation konnte nicht gestartet werden",
        ),
        (
            "Pull request is refreshing; reload after its current changes arrive",
            "Pull request is refreshing; reload after its current changes arrive",
            "Der Pull Request wird aktualisiert; lade neu, sobald seine aktuellen Änderungen vorliegen",
        ),
        (
            "Current pull request details and changes must finish loading before review",
            "Current pull request details and changes must finish loading before review",
            "Vor dem Review müssen die aktuellen Pull-Request-Details und Änderungen vollständig geladen sein",
        ),
        (
            "Working tree and index · current membership",
            "Working tree and index · current membership",
            "Arbeitsverzeichnis und Index · aktueller Dateiumfang",
        ),
        ("working tree", "working tree", "Arbeitsverzeichnis"),
        (
            "PR snapshot missing",
            "PR snapshot missing",
            "PR-Momentaufnahme fehlt",
        ),
        (
            "This document has more than 500 files; select a narrower comparison",
            "This document has more than 500 files; select a narrower comparison",
            "Dieses Dokument enthält mehr als 500 Dateien; wähle einen engeren Vergleich",
        ),
        (
            "Review loading cancelled",
            "Review loading cancelled",
            "Laden des Reviews abgebrochen",
        ),
        (
            "Review exceeds 250,000 lines or 32 MiB; select a narrower comparison. No partial document was accepted.",
            "Review exceeds 250,000 lines or 32 MiB; select a narrower comparison. No partial document was accepted.",
            "Das Review überschreitet 250.000 Zeilen oder 32 MiB; wähle einen engeren Vergleich. Es wurde kein unvollständiges Dokument übernommen.",
        ),
        (
            "Choose between 1 and 32 target revisions, one per line",
            "Choose between 1 and 32 target revisions, one per line",
            "Wähle zwischen 1 und 32 Zielrevisionen, eine pro Zeile",
        ),
        (
            "Explicitly trust and enter the regression command first",
            "Explicitly trust and enter the regression command first",
            "Bestätige zuerst ausdrücklich dein Vertrauen und gib den Regressionsbefehl ein",
        ),
        (
            "Cancelled before patch application; behavior not tested",
            "Cancelled before patch application; behavior not tested",
            "Vor dem Anwenden des Patches abgebrochen; Verhalten nicht getestet",
        ),
        (
            "No personal data directory",
            "No personal data directory",
            "Kein persönliches Datenverzeichnis",
        ),
        (
            "Rulebook writer unavailable",
            "Rulebook writer unavailable",
            "Schreibzugriff auf das Regelwerk nicht verfügbar",
        ),
        (
            "No rulebook folder",
            "No rulebook folder",
            "Kein Regelwerkordner",
        ),
        (
            "The forge did not identify your signed-in account",
            "The forge did not identify your signed-in account",
            "Die Forge hat dein angemeldetes Konto nicht identifiziert",
        ),
        (
            "Rule has no imported comment citation",
            "Rule has no imported comment citation",
            "Die Regel hat keinen Verweis auf einen importierten Kommentar",
        ),
        (
            "Rule pattern is empty or too large",
            "Rule pattern is empty or too large",
            "Das Regelmuster ist leer oder zu groß",
        ),
        (
            "Open a pull request to import your published review comments",
            "Open a pull request to import your published review comments",
            "Öffne einen Pull Request, um deine veröffentlichten Review-Kommentare zu importieren",
        ),
        (
            "Opt in before sending comments to your selected AI tool",
            "Opt in before sending comments to your selected AI tool",
            "Stimme zuerst zu, bevor Kommentare an dein ausgewähltes KI-Werkzeug gesendet werden",
        ),
        (
            "Wait for the current operation or reviewer echo before sending",
            "Wait for the current operation or reviewer echo before sending",
            "Warte vor dem Senden auf die aktuelle Operation oder das Reviewer-Echo",
        ),
        (
            "The prepared profile or identity policy changed; reopen push preflight",
            "The prepared profile or identity policy changed; reopen push preflight",
            "Das vorbereitete Profil oder die Identitätsrichtlinie wurde geändert; öffne die Push-Vorprüfung erneut",
        ),
        (
            "Reviewed send profile, sign-in or repository changed; prepare again",
            "Reviewed send profile, sign-in or repository changed; prepare again",
            "Profil, Anmeldung oder Repository des geprüften Sendevorgangs wurde geändert; bereite ihn erneut vor",
        ),
        (
            "Send reviewed exact commit",
            "Send reviewed exact commit",
            "Geprüften exakten Commit senden",
        ),
        (
            "Review or cancel the pending reviewer-echo operation first",
            "Review or cancel the pending reviewer-echo operation first",
            "Prüfe zuerst die ausstehende Reviewer-Echo-Operation oder brich sie ab",
        ),
        (
            "Push source or remote configuration changed during reviewer echo; start the push again",
            "Push source or remote configuration changed during reviewer echo; start the push again",
            "Push-Quelle oder Remote-Konfiguration wurde während des Reviewer-Echos geändert; starte den Push erneut",
        ),
        (
            "Reviewer echo inputs or rulebook changed before execution; start the push again",
            "Reviewer echo inputs or rulebook changed before execution; start the push again",
            "Eingaben des Reviewer-Echos oder Regelwerk wurden vor der Ausführung geändert; starte den Push erneut",
        ),
        (
            "Push source or remote configuration changed after reviewer echo; start the push again",
            "Push source or remote configuration changed after reviewer echo; start the push again",
            "Push-Quelle oder Remote-Konfiguration wurde nach dem Reviewer-Echo geändert; starte den Push erneut",
        ),
        (
            "Reviewed push already has execution evidence; resume its retained operation",
            "Reviewed push already has execution evidence; resume its retained operation",
            "Der geprüfte Push hat bereits einen Ausführungsnachweis; setze die gespeicherte Operation fort",
        ),
        (
            "Push destination is not unambiguous; review its source manually",
            "Push destination is not unambiguous; review its source manually",
            "Das Push-Ziel ist nicht eindeutig; prüfe seine Quelle manuell",
        ),
        (
            "This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation",
            "This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation",
            "Diese Operation ändert den Zustand vor dem Push. Führe die Review-Prüfung nach diesen Änderungen aus oder prüfe diese Operation ausdrücklich und setze sie fort",
        ),
        (
            "Invalid wildcard source",
            "Invalid wildcard source",
            "Ungültige Platzhalterquelle",
        ),
        (
            "Unsupported push wildcard; review it explicitly",
            "Unsupported push wildcard; review it explicitly",
            "Nicht unterstützter Push-Platzhalter; prüfe ihn ausdrücklich",
        ),
        (
            "Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review",
            "Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review",
            "Das Reviewer-Echo kann kontextändernde oder unbekannte Git-Präfixe nicht untersuchen; diese Operation erfordert eine ausdrückliche manuelle Prüfung",
        ),
        (
            "More than eight receipt revisions: further Rust dependency snapshots remain unchecked",
            "More than eight receipt revisions: further Rust dependency snapshots remain unchecked",
            "Mehr als acht Nachweisrevisionen: Weitere Momentaufnahmen von Rust-Abhängigkeiten bleiben ungeprüft",
        ),
        (
            "Configure your Git name and email before posting an intent",
            "Configure your Git name and email before posting an intent",
            "Konfiguriere vor dem Anlegen einer Absicht deinen Git-Namen und deine E-Mail-Adresse",
        ),
        (
            "Give a relative code path and a short intent",
            "Give a relative code path and a short intent",
            "Gib einen relativen Codepfad und eine kurze Absicht an",
        ),
        (
            "Invalid intent cache identity",
            "Invalid intent cache identity",
            "Ungültige Identität des Absichtscaches",
        ),
        (
            "Intent is outside the selected source",
            "Intent is outside the selected source",
            "Die Absicht liegt außerhalb der ausgewählten Quelle",
        ),
        (
            "Invalid intent ref identity",
            "Invalid intent ref identity",
            "Ungültige Ref-Identität der Absicht",
        ),
        (
            "Intent identity does not match its source ref",
            "Intent identity does not match its source ref",
            "Die Identität der Absicht stimmt nicht mit ihrem Quell-Ref überein",
        ),
        (
            "Intent fields exceed the short-note bounds or contain an unsafe path/control character",
            "Intent fields exceed the short-note bounds or contain an unsafe path/control character",
            "Die Absichtsfelder überschreiten die Grenzen für Kurznotizen oder enthalten einen unsicheren Pfad bzw. ein Steuerzeichen",
        ),
        (
            "Intent expiry must be within 24 hours of its creation",
            "Intent expiry must be within 24 hours of its creation",
            "Die Absicht muss innerhalb von 24 Stunden nach ihrer Erstellung ablaufen",
        ),
        (
            "More than 5000 refs in the selected intent source; archive old notes before loading",
            "More than 5000 refs in the selected intent source; archive old notes before loading",
            "Mehr als 5000 Refs in der ausgewählten Absichtsquelle; archiviere alte Notizen vor dem Laden",
        ),
        (
            "Selected intent metadata exceeds the 4MiB import budget; read refused because more notes remain",
            "Selected intent metadata exceeds the 4MiB import budget; read refused because more notes remain",
            "Die ausgewählten Absichtsmetadaten überschreiten das Importlimit von 4 MiB; das Lesen wurde verweigert, weil weitere Notizen verbleiben",
        ),
        (
            "Intent exceeds the metadata import budget",
            "Intent exceeds the metadata import budget",
            "Die Absicht überschreitet das Importlimit für Metadaten",
        ),
        (
            "Malformed intent ref listing",
            "Malformed intent ref listing",
            "Fehlerhafte Ref-Liste der Absichten",
        ),
        (
            "Invalid intent blob size",
            "Invalid intent blob size",
            "Ungültige Blob-Größe der Absicht",
        ),
        (
            "Pick a configured remote",
            "Pick a configured remote",
            "Wähle ein konfiguriertes Remote",
        ),
        (
            "You can only end your own local intents",
            "You can only end your own local intents",
            "Du kannst nur deine eigenen lokalen Absichten beenden",
        ),
        (
            "Intent no longer present",
            "Intent no longer present",
            "Die Absicht ist nicht mehr vorhanden",
        ),
        (
            "No captured sharing destination",
            "No captured sharing destination",
            "Kein erfasstes Ziel zum Teilen",
        ),
        (
            "No local intents to publish",
            "No local intents to publish",
            "Keine lokalen Absichten zum Veröffentlichen",
        ),
        (
            "Pin a nonempty, nonmerge fix patch",
            "Pin a nonempty, nonmerge fix patch",
            "Fixiere einen nicht leeren Fix-Patch ohne Merge",
        ),
        (
            "Give the pinned fix a regression command",
            "Give the pinned fix a regression command",
            "Gib dem fixierten Fix einen Regressionsbefehl",
        ),
        (
            "Choose 1–32 release targets",
            "Choose 1–32 release targets",
            "Wähle 1–32 Release-Ziele",
        ),
        (
            "Reload the pinned fixes",
            "Reload the pinned fixes",
            "Lade die fixierten Fixes neu",
        ),
        (
            "Explicitly trust the regression command before running it",
            "Explicitly trust the regression command before running it",
            "Bestätige vor der Ausführung ausdrücklich dein Vertrauen in den Regressionsbefehl",
        ),
        (
            "Reload pinned fixes",
            "Reload pinned fixes",
            "Fixierte Fixes neu laden",
        ),
        (
            "Generated result belongs to the earlier form. Your edits were preserved; inspect and explicitly use the result below if wanted.",
            "Generated result belongs to the earlier form. Your edits were preserved; inspect and explicitly use the result below if wanted.",
            "Das generierte Ergebnis gehört zum früheren Formular. Deine Änderungen wurden beibehalten; prüfe das Ergebnis unten und übernimm es bei Bedarf ausdrücklich.",
        ),
        (
            "Inspect the source and explicitly approve sandbox execution first",
            "Inspect the source and explicitly approve sandbox execution first",
            "Prüfe zuerst den Quelltext und stimme der Sandbox-Ausführung ausdrücklich zu",
        ),
        (
            "The generated integration test must be 1–65536 bytes",
            "The generated integration test must be 1–65536 bytes",
            "Der generierte Integrationstest muss 1–65536 Byte umfassen",
        ),
        (
            "Cleanup needs attention",
            "Cleanup needs attention",
            "Bereinigung erfordert Aufmerksamkeit",
        ),
        (
            "Stop requested. Bounded cleanup and partial observations are preserved; load preserved observations after the worker finishes.",
            "Stop requested. Bounded cleanup and partial observations are preserved; load preserved observations after the worker finishes.",
            "Stopp angefordert. Begrenzte Bereinigung und Teilbeobachtungen werden gesichert; lade die gesicherten Beobachtungen nach Abschluss des Auftrags.",
        ),
        (
            "Select a smaller file diff before generating a reproducer",
            "Select a smaller file diff before generating a reproducer",
            "Wähle vor der Generierung eines Reproduktionsfalls einen kleineren Datei-Diff",
        ),
        (
            "The model produced more than 64KiB; narrow the objection",
            "The model produced more than 64KiB; narrow the objection",
            "Das Modell hat mehr als 64 KiB erzeugt; grenze den Einwand ein",
        ),
        (
            "Invalid immutable image/owner identity",
            "Invalid immutable image/owner identity",
            "Ungültige unveränderliche Image-/Eigentümeridentität",
        ),
        (
            "Container ownership inspection was interrupted",
            "Container ownership inspection was interrupted",
            "Prüfung des Container-Eigentümers wurde unterbrochen",
        ),
        (
            "Invalid container ownership response",
            "Invalid container ownership response",
            "Ungültige Antwort zum Container-Eigentümer",
        ),
        (
            "Invalid immutable container ownership response",
            "Invalid immutable container ownership response",
            "Ungültige Antwort zum unveränderlichen Container-Eigentümer",
        ),
        (
            "Name belongs to another owner; cleanup refused",
            "Name belongs to another owner; cleanup refused",
            "Der Name gehört einem anderen Eigentümer; Bereinigung verweigert",
        ),
        (
            "Creation outcome is unknown; no owned ID was found. A stopped container may remain if creation finished after inspection; no removal by name was attempted.",
            "Creation outcome is unknown; no owned ID was found. A stopped container may remain if creation finished after inspection; no removal by name was attempted.",
            "Das Erstellungsergebnis ist unbekannt; keine eigene ID gefunden. Wenn die Erstellung nach der Prüfung abgeschlossen wurde, kann ein gestoppter Container verbleiben; eine Entfernung anhand des Namens wurde nicht versucht.",
        ),
        (
            "No immutable container ID",
            "No immutable container ID",
            "Keine unveränderliche Container-ID",
        ),
        (
            "Invalid cleanup ID",
            "Invalid cleanup ID",
            "Ungültige Bereinigungs-ID",
        ),
        (
            "Container was already absent",
            "Container was already absent",
            "Container war bereits nicht vorhanden",
        ),
        (
            "Immutable container owner/ID mismatch; cleanup refused",
            "Immutable container owner/ID mismatch; cleanup refused",
            "Unveränderlicher Container-Eigentümer und ID stimmen nicht überein; Bereinigung verweigert",
        ),
        (
            "Container cleanup",
            "Container cleanup",
            "Container-Bereinigung",
        ),
        (
            "Owned container removed by immutable ID",
            "Owned container removed by immutable ID",
            "Eigener Container anhand der unveränderlichen ID entfernt",
        ),
        (
            "Choose a preloaded Rust image by name or digest",
            "Choose a preloaded Rust image by name or digest",
            "Wähle ein vorab geladenes Rust-Image nach Name oder Digest",
        ),
        (
            "Inspect preloaded image",
            "Inspect preloaded image",
            "Vorab geladenes Image prüfen",
        ),
        ("Invalid image ID", "Invalid image ID", "Ungültige Image-ID"),
        (
            "Selected image is unavailable locally or did not return an immutable SHA256 ID; images are never pulled",
            "Selected image is unavailable locally or did not return an immutable SHA256 ID; images are never pulled",
            "Das ausgewählte Image ist lokal nicht verfügbar oder hat keine unveränderliche SHA256-ID geliefert; Images werden niemals heruntergeladen",
        ),
        ("Create sandbox", "Create sandbox", "Sandbox erstellen"),
        (
            "Invalid container creation response",
            "Invalid container creation response",
            "Ungültige Antwort bei der Container-Erstellung",
        ),
        (
            "Docker did not return a complete immutable container ID",
            "Docker did not return a complete immutable container ID",
            "Docker hat keine vollständige unveränderliche Container-ID geliefert",
        ),
        (
            "Created container ownership could not be established; execution refused",
            "Created container ownership could not be established; execution refused",
            "Der Eigentümer des erstellten Containers ließ sich nicht feststellen; Ausführung verweigert",
        ),
        (
            "Read sandbox exit status",
            "Read sandbox exit status",
            "Sandbox-Exitstatus lesen",
        ),
        (
            "Invalid sandbox exit status",
            "Invalid sandbox exit status",
            "Ungültiger Sandbox-Exitstatus",
        ),
        (
            "Sandbox exit status is outside Docker's range",
            "Sandbox exit status is outside Docker's range",
            "Der Sandbox-Exitstatus liegt außerhalb des Docker-Wertebereichs",
        ),
        (
            "Source export cancelled",
            "Source export cancelled",
            "Quellexport abgebrochen",
        ),
        (
            "Raw source export exceeded its 60s preparation budget",
            "Raw source export exceeded its 60s preparation budget",
            "Der direkte Quellexport hat seine Vorbereitungszeit von 60 Sekunden überschritten",
        ),
        (
            "Sandbox source exceeds 20,000 files or 128MiB",
            "Sandbox source exceeds 20,000 files or 128MiB",
            "Die Sandbox-Quellen überschreiten 20.000 Dateien oder 128 MiB",
        ),
        (
            "Submodule nesting exceeds the eight-level source-export limit",
            "Submodule nesting exceeds the eight-level source-export limit",
            "Die Submodulverschachtelung überschreitet das Quellexportlimit von acht Ebenen",
        ),
        (
            "Malformed raw tree record",
            "Malformed raw tree record",
            "Fehlerhafter Datensatz im Git-Rohbaum",
        ),
        (
            "Invalid raw tree metadata",
            "Invalid raw tree metadata",
            "Ungültige Metadaten im Git-Rohbaum",
        ),
        (
            "Malformed raw tree metadata",
            "Malformed raw tree metadata",
            "Fehlerhafte Metadaten im Git-Rohbaum",
        ),
        (
            "Raw tree contains an invalid object ID",
            "Raw tree contains an invalid object ID",
            "Der Git-Rohbaum enthält eine ungültige Objekt-ID",
        ),
        (
            "Sandbox source paths must be UTF-8",
            "Sandbox source paths must be UTF-8",
            "Sandbox-Quellpfade müssen UTF-8 verwenden",
        ),
        (
            "Unsupported raw tree object mode",
            "Unsupported raw tree object mode",
            "Nicht unterstützter Objektmodus im Git-Rohbaum",
        ),
        (
            "Invalid raw blob size",
            "Invalid raw blob size",
            "Ungültige Größe des Git-Rohblobs",
        ),
        (
            "Sandbox source exceeds 16MiB per blob or 128MiB total",
            "Sandbox source exceeds 16MiB per blob or 128MiB total",
            "Die Sandbox-Quellen überschreiten 16 MiB pro Blob oder insgesamt 128 MiB",
        ),
        (
            "Raw blob size changed or output was truncated",
            "Raw blob size changed or output was truncated",
            "Die Größe des Git-Rohblobs wurde geändert oder die Ausgabe wurde abgeschnitten",
        ),
        (
            "Duplicate raw source path",
            "Duplicate raw source path",
            "Doppelter Pfad im direkten Quellexport",
        ),
        (
            "Sandbox symlink targets must be UTF-8",
            "Sandbox symlink targets must be UTF-8",
            "Sandbox-Symlink-Ziele müssen UTF-8 verwenden",
        ),
        (
            "Symlink points into Git metadata",
            "Symlink points into Git metadata",
            "Symlink verweist auf Git-Metadaten",
        ),
        (
            "Archive field overflow",
            "Archive field overflow",
            "Überlauf eines Archivfelds",
        ),
        (
            "Archive header requires a PAX path record",
            "Archive header requires a PAX path record",
            "Der Archiv-Header erfordert einen PAX-Pfaddatensatz",
        ),
        (
            "Non-UTF-8 or newline-containing Rust paths were not analyzed",
            "Non-UTF-8 or newline-containing Rust paths were not analyzed",
            "Rust-Pfade ohne UTF-8 oder mit Zeilenumbrüchen wurden nicht analysiert",
        ),
        (
            "More than 4096 Rust files; dependency analysis was not performed",
            "More than 4096 Rust files; dependency analysis was not performed",
            "Mehr als 4096 Rust-Dateien; Abhängigkeitsanalyse nicht durchgeführt",
        ),
        (
            "Rust sources exceed the 32MiB analysis budget; dependencies unchecked",
            "Rust sources exceed the 32MiB analysis budget; dependencies unchecked",
            "Die Rust-Quellen überschreiten das Analyselimit von 32 MiB; Abhängigkeiten ungeprüft",
        ),
        (
            "Incomplete Rust blob header",
            "Incomplete Rust blob header",
            "Unvollständiger Rust-Blob-Header",
        ),
        (
            "Invalid Rust blob size",
            "Invalid Rust blob size",
            "Ungültige Rust-Blob-Größe",
        ),
        (
            "A Rust source is not a blob",
            "A Rust source is not a blob",
            "Eine Rust-Quelle ist kein Blob",
        ),
        (
            "Incomplete Rust blob",
            "Incomplete Rust blob",
            "Unvollständiger Rust-Blob",
        ),
        (
            "Incomplete Rust blob separator",
            "Incomplete Rust blob separator",
            "Unvollständiges Rust-Blob-Trennzeichen",
        ),
        (
            "Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types",
            "Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types",
            "Der Rust-Syntaxnachweis löst cfg-Features, benutzerdefinierte #[path]-Module, Re-Exporte oder Typen nicht auf",
        ),
        (
            "Static call traversal exceeded 1024 symbols",
            "Static call traversal exceeded 1024 symbols",
            "Die Verfolgung statischer Aufrufe hat 1024 Symbole überschritten",
        ),
        (
            "Dynamic, external, local-import, or macro-generated calls remain unchecked",
            "Dynamic, external, local-import, or macro-generated calls remain unchecked",
            "Dynamische, externe, lokal importierte oder durch Makros erzeugte Aufrufe bleiben ungeprüft",
        ),
        (
            "Duplicate/cfg-dependent symbol identities were not resolved",
            "Duplicate/cfg-dependent symbol identities were not resolved",
            "Doppelte oder cfg-abhängige Symbolidentitäten wurden nicht aufgelöst",
        ),
        (
            "No unchanged uniquely identified Rust functions were analyzed",
            "No unchanged uniquely identified Rust functions were analyzed",
            "Keine unveränderten eindeutig identifizierten Rust-Funktionen analysiert",
        ),
        (
            "Review-artifact sharing requires exactly one fetch and one push URL",
            "Review-artifact sharing requires exactly one fetch and one push URL",
            "Das Teilen von Review-Artefakten erfordert genau eine Fetch- und eine Push-URL",
        ),
        (
            "Sharing destination changed. A completed transfer may already have affected the captured destination; refresh before retrying",
            "Sharing destination changed. A completed transfer may already have affected the captured destination; refresh before retrying",
            "Das Ziel zum Teilen wurde geändert. Eine abgeschlossene Übertragung kann das erfasste Ziel bereits verändert haben; aktualisiere vor einem erneuten Versuch",
        ),
        (
            "Sharing remote changed; refresh repository remotes and try again",
            "Sharing remote changed; refresh repository remotes and try again",
            "Das Remote zum Teilen wurde geändert; aktualisiere die Repository-Remotes und versuche es erneut",
        ),
        (
            "Artifact cache update crossed its captured namespace",
            "Artifact cache update crossed its captured namespace",
            "Die Aktualisierung des Artefaktcaches hat den erfassten Namensraum überschritten",
        ),
        (
            "Give the card its explanation",
            "Give the card its explanation",
            "Gib der Karte eine Erklärung",
        ),
        (
            "No test-lane result for this tree; run Test commits first",
            "No test-lane result for this tree; run Test commits first",
            "Kein Testspur-Ergebnis für diesen Baum; führe zuerst „Commits testen“ aus",
        ),
        (
            "No test command recorded",
            "No test command recorded",
            "Kein Testbefehl aufgezeichnet",
        ),
        (
            "Choose a repository-relative path",
            "Choose a repository-relative path",
            "Wähle einen Pfad relativ zum Repository",
        ),
        (
            "Line must be a positive number",
            "Line must be a positive number",
            "Die Zeile muss eine positive Zahl sein",
        ),
        (
            "Line is outside this revision's file",
            "Line is outside this revision's file",
            "Die Zeile liegt außerhalb der Datei dieser Revision",
        ),
        (
            "Current tree differs from the pinned test result",
            "Current tree differs from the pinned test result",
            "Der aktuelle Baum weicht vom fixierten Testergebnis ab",
        ),
        (
            "The pinned commit has no replacement in this series",
            "The pinned commit has no replacement in this series",
            "Der fixierte Commit hat keinen Ersatz in dieser Serie",
        ),
        (
            "The card's file changed; open the pinned original to review it",
            "The card's file changed; open the pinned original to review it",
            "Die Datei der Karte wurde geändert; öffne zum Prüfen das fixierte Original",
        ),
        (
            "The card's file was removed or is unavailable",
            "The card's file was removed or is unavailable",
            "Die Datei der Karte wurde entfernt oder ist nicht verfügbar",
        ),
        (
            "A shared storyboard supports at most 256 cards",
            "A shared storyboard supports at most 256 cards",
            "Ein geteiltes Storyboard unterstützt höchstens 256 Karten",
        ),
        (
            "Shared storyboard exceeds the 2MiB metadata budget",
            "Shared storyboard exceeds the 2MiB metadata budget",
            "Das geteilte Storyboard überschreitet das Metadatenlimit von 2 MiB",
        ),
        (
            "Shared storyboard has more than 256 cards",
            "Shared storyboard has more than 256 cards",
            "Das geteilte Storyboard enthält mehr als 256 Karten",
        ),
        (
            "Shared storyboard revision is not an immutable commit ID",
            "Shared storyboard revision is not an immutable commit ID",
            "Die Revision des geteilten Storyboards ist keine unveränderliche Commit-ID",
        ),
        (
            "Shared storyboard blob does not match its pinned source",
            "Shared storyboard blob does not match its pinned source",
            "Der Blob des geteilten Storyboards stimmt nicht mit seiner fixierten Quelle überein",
        ),
        (
            "Shared storyboard changed while preparing publication; fetch and retry",
            "Shared storyboard changed while preparing publication; fetch and retry",
            "Das geteilte Storyboard wurde während der Veröffentlichungsvorbereitung geändert; rufe es ab und versuche es erneut",
        ),
        (
            "Shared storyboard was not fetched",
            "Shared storyboard was not fetched",
            "Das geteilte Storyboard wurde nicht abgerufen",
        ),
        (
            "Choose a configured storyboard sharing remote",
            "Choose a configured storyboard sharing remote",
            "Wähle ein konfiguriertes Remote zum Teilen des Storyboards",
        ),
        (
            "Pull request changed; refresh the storyboard",
            "Pull request changed; refresh the storyboard",
            "Der Pull Request wurde geändert; aktualisiere das Storyboard",
        ),
        (
            "Only the pull request author publishes its storyboard",
            "Only the pull request author publishes its storyboard",
            "Nur der Autor des Pull Requests veröffentlicht dessen Storyboard",
        ),
        (
            "Pull request files are refreshing; wait before arranging its storyboard",
            "Pull request files are refreshing; wait before arranging its storyboard",
            "Die Pull-Request-Dateien werden aktualisiert; warte vor dem Anordnen des Storyboards",
        ),
        (
            "Load your local storyboard before editing",
            "Load your local storyboard before editing",
            "Lade vor dem Bearbeiten dein lokales Storyboard",
        ),
        (
            "Only the pull request author arranges this storyboard",
            "Only the pull request author arranges this storyboard",
            "Nur der Autor des Pull Requests ordnet dieses Storyboard an",
        ),
        (
            "The storyboard selection is stale; reload it",
            "The storyboard selection is stale; reload it",
            "Die Storyboard-Auswahl ist veraltet; lade sie neu",
        ),
    ];
    assert_eq!(fixed_expected.len(), FIXED.len());
    let dynamic_expected: &[(&str, &str, &str)] = &[
        (
            "Choose the repository for OPAQUE-component-Stage1/日本語/00001234",
            "Choose the repository for OPAQUE-component-Stage1/日本語/00001234",
            "Wähle das Repository für OPAQUE-component-Stage1/日本語/00001234",
        ),
        (
            "OPAQUE-component-Stage1/日本語/00001234 needs 2–4 ordered stages: deployed, optional bridge, next",
            "OPAQUE-component-Stage1/日本語/00001234 needs 2–4 ordered stages: deployed, optional bridge, next",
            "OPAQUE-component-Stage1/日本語/00001234 benötigt 2–4 geordnete Stufen: ausgeliefert, optionaler Übergang, nächste",
        ),
        (
            "Hunk refused: OPAQUE-detail-Stage1/日本語/00001234",
            "Hunk refused: OPAQUE-detail-Stage1/日本語/00001234",
            "Hunk abgelehnt: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Commit OPAQUE-commit-Stage1/日本語/00001234",
            "Commit OPAQUE-commit-Stage1/日本語/00001234",
            "Commit OPAQUE-commit-Stage1/日本語/00001234",
        ),
        (
            "Comparison OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Comparison OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Vergleich OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
        ),
        (
            "Pull request #OPAQUE-number-Stage1/日本語/00001234 · OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Pull request #OPAQUE-number-Stage1/日本語/00001234 · OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
            "Pull Request #OPAQUE-number-Stage1/日本語/00001234 · OPAQUE-from-Stage1/日本語/00001234 → OPAQUE-to-Stage1/日本語/00001234",
        ),
        (
            "Invalid rulebook from model: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid rulebook from model: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiges Regelwerk vom Modell: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Reviewer echo cannot infer push option OPAQUE-option-Stage1/日本語/00001234; review this operation explicitly",
            "Reviewer echo cannot infer push option OPAQUE-option-Stage1/日本語/00001234; review this operation explicitly",
            "Das Reviewer-Echo kann die Push-Option OPAQUE-option-Stage1/日本語/00001234 nicht auswerten; prüfe diese Operation ausdrücklich",
        ),
        (
            "Push mode OPAQUE-mode-Stage1/日本語/00001234 has no inferable source; review it explicitly",
            "Push mode OPAQUE-mode-Stage1/日本語/00001234 has no inferable source; review it explicitly",
            "Der Push-Modus OPAQUE-mode-Stage1/日本語/00001234 hat keine ableitbare Quelle; prüfe ihn ausdrücklich",
        ),
        (
            "Push source OPAQUE-source-Stage1/日本語/00001234 has no inspectable commit tree",
            "Push source OPAQUE-source-Stage1/日本語/00001234 has no inspectable commit tree",
            "Die Push-Quelle OPAQUE-source-Stage1/日本語/00001234 hat keinen untersuchbaren Commit-Baum",
        ),
        (
            "Review receipt: OPAQUE-detail-Stage1/日本語/00001234",
            "Review receipt: OPAQUE-detail-Stage1/日本語/00001234",
            "Review-Nachweis: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Invalid intent JSON: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid intent JSON: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiges Absichts-JSON: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Intent blob exceeds the 1234-byte budget",
            "Intent blob exceeds the 1,234-byte budget",
            "Der Absichts-Blob überschreitet das Limit von 1.234 Byte",
        ),
        (
            "Latest independent cleanup OPAQUE-name-Stage1/日本語/00001234 (OPAQUE-id-Stage1/日本語/00001234) · OPAQUE-detail-Stage1/日本語/00001234",
            "Latest independent cleanup OPAQUE-name-Stage1/日本語/00001234 (OPAQUE-id-Stage1/日本語/00001234) · OPAQUE-detail-Stage1/日本語/00001234",
            "Letzte unabhängige Bereinigung OPAQUE-name-Stage1/日本語/00001234 (OPAQUE-id-Stage1/日本語/00001234) · OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Container ownership inspection failed: OPAQUE-detail-Stage1/日本語/00001234",
            "Container ownership inspection failed: OPAQUE-detail-Stage1/日本語/00001234",
            "Prüfung des Container-Eigentümers fehlgeschlagen: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Cleanup needs attention: OPAQUE-detail-Stage1/日本語/00001234",
            "Cleanup needs attention: OPAQUE-detail-Stage1/日本語/00001234",
            "Bereinigung erfordert Aufmerksamkeit: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Could not preserve reproducer cleanup evidence for OPAQUE-name-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234; OPAQUE-result-Stage1/日本語/00001234",
            "Could not preserve reproducer cleanup evidence for OPAQUE-name-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234; OPAQUE-result-Stage1/日本語/00001234",
            "Bereinigungsnachweis für Reproduktionsfall OPAQUE-name-Stage1/日本語/00001234 konnte nicht gesichert werden: OPAQUE-detail-Stage1/日本語/00001234; OPAQUE-result-Stage1/日本語/00001234",
        ),
        (
            "Sandbox did not complete: OPAQUE-detail-Stage1/日本語/00001234",
            "Sandbox did not complete: OPAQUE-detail-Stage1/日本語/00001234",
            "Sandbox-Lauf nicht abgeschlossen: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Invalid preserved reproducer evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid preserved reproducer evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiger gesicherter Reproduktionsnachweis: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Submodule OPAQUE-path-Stage1/日本語/00001234 at OPAQUE-oid-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234. Its pinned objects must already be present locally; gitcraft never clones or checks it out.",
            "Submodule OPAQUE-path-Stage1/日本語/00001234 at OPAQUE-oid-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234. Its pinned objects must already be present locally; gitcraft never clones or checks it out.",
            "Submodul OPAQUE-path-Stage1/日本語/00001234 bei OPAQUE-oid-Stage1/日本語/00001234: OPAQUE-detail-Stage1/日本語/00001234. Seine fixierten Objekte müssen bereits lokal vorhanden sein; gitcraft klont es niemals und führt keinen Checkout aus.",
        ),
        (
            "Unsafe or unsupported sandbox source path: OPAQUE-path-Stage1/日本語/00001234",
            "Unsafe or unsupported sandbox source path: OPAQUE-path-Stage1/日本語/00001234",
            "Unsicherer oder nicht unterstützter Sandbox-Quellpfad: OPAQUE-path-Stage1/日本語/00001234",
        ),
        (
            "Unsafe symlink OPAQUE-path-Stage1/日本語/00001234",
            "Unsafe symlink OPAQUE-path-Stage1/日本語/00001234",
            "Unsicherer Symlink OPAQUE-path-Stage1/日本語/00001234",
        ),
        (
            "Symlink OPAQUE-path-Stage1/日本語/00001234 escapes the source root",
            "Symlink OPAQUE-path-Stage1/日本語/00001234 escapes the source root",
            "Symlink OPAQUE-path-Stage1/日本語/00001234 verlässt das Quellverzeichnis",
        ),
        (
            "Archive member OPAQUE-path-Stage1/日本語/00001234 traverses a symlink",
            "Archive member OPAQUE-path-Stage1/日本語/00001234 traverses a symlink",
            "Archiveintrag OPAQUE-path-Stage1/日本語/00001234 durchläuft einen Symlink",
        ),
        (
            "Invalid review evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid review evidence: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiger Review-Nachweis: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "Invalid shared storyboard: OPAQUE-detail-Stage1/日本語/00001234",
            "Invalid shared storyboard: OPAQUE-detail-Stage1/日本語/00001234",
            "Ungültiges geteiltes Storyboard: OPAQUE-detail-Stage1/日本語/00001234",
        ),
        (
            "1234 additional invalid records omitted",
            "1,234 additional invalid records omitted",
            "1.234 weitere ungültige Einträge ausgelassen",
        ),
    ];
    assert_eq!(dynamic_expected.len(), TEMPLATES.len());
    for language in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(language);
        for &(source, en, de) in fixed_expected.iter().chain(dynamic_expected) {
            let stored = source.to_owned();
            let expected = if language == "de" { de } else { en };
            assert_eq!(display_operation(&stored), expected, "operation: {source}");
            assert_eq!(display_status(&stored), expected, "status: {source}");
            assert_eq!(stored, source);
        }
        assert_eq!(
            super::super::review_owned("1 additional invalid records omitted"),
            if language == "de" {
                "Ein weiterer ungültiger Eintrag ausgelassen"
            } else {
                "One additional invalid record omitted"
            }
        );
    }
}

#[test]
fn review_source_owned_diagnostics_keep_refs_commands_and_unknown_lookalikes_opaque() {
    let witnesses = [
        (
            "path/00001234.rs: Rust syntax not parsed (provider: Stage 1)",
            "path/00001234.rs: Rust-Syntax nicht geparst (provider: Stage 1)",
        ),
        (
            "a → b: static call targets changed",
            "a → b: Ziele statischer Aufrufe geändert",
        ),
        (
            "a → b → module/01234 changed",
            "a → b → module/01234 geändert",
        ),
    ];
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for (raw, de) in witnesses {
            let stored = raw.to_owned();
            assert_eq!(dependency(&stored), if language == "de" { de } else { raw });
            assert_eq!(stored, raw);
        }
        let raw = "refs/gitcraft/intents/日本語/00001234: Invalid intent JSON: provider: Stage";
        assert_eq!(
            intent_warning(raw),
            if language == "de" {
                "refs/gitcraft/intents/日本語/00001234: Ungültiges Intent-JSON: provider: Stage"
            } else {
                raw
            }
        );
        for detail in [
            "Stage",
            "Stage1",
            "Stage 1",
            "Abort custom-deploy",
            "Continue custom-script",
            "Undo Stage",
            "Preparing Stage",
            "provider: fatal: external diagnostic",
            "Docker sha256:0001234 · cargo test --offline --test gitcraft_reproducer",
        ] {
            let stored = format!("refs/gitcraft/intents/00001234: {detail}");
            assert_eq!(intent_warning(&stored), stored);
            assert_eq!(dependency(detail), detail);
            assert_eq!(super::super::review_owned(detail), detail);
        }
        for raw in [
            "Hunk refusedx: external",
            "invalid intent JSON: literal",
            "Stage1",
            "Stage 1",
            "Stage 0001",
            "Abort custom-deploy",
            "Continue custom-script",
            "provider: external prose",
            "No head run after cancellation",
            "Stopped after base; no head comparison was made",
            "Head source preparation did not complete",
            "unknown ID",
            "Original fix",
            "Adapted fix",
            "Configured when pinned (execution command not recorded by test lane): Stage",
            "refs/00001234: Stage",
        ] {
            // These strings are not eligible for the canonical review map. Dedicated
            // provenance adapters (or the pending compare seam) own their interpretation.
            assert_eq!(super::super::review_owned(raw), raw);
        }
        for raw in [
            "Abort custom-deploy",
            "Continue custom-script",
            "Stage1",
            "provider: external prose",
            "Original fix",
            "Adapted fix",
        ] {
            assert_eq!(display_operation(raw), raw);
            assert_eq!(display_status(raw), raw);
        }
    }
    crate::i18n::set_language("en-US");
}
