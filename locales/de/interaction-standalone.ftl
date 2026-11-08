# Standalone file-tool presentation. Preserve literal paths/argv/markers and opaque diagnostics.

interaction-standalone-base = Basis

interaction-standalone-binary-preview = { $count ->
        [one] Binär/nicht UTF-8 · ein Byte
       *[other] Binär/nicht UTF-8 · { $count } Bytes
    }
    { $preview }

interaction-standalone-cancel-tool = Dateiwerkzeug abbrechen

interaction-standalone-choose-whole-file = Wähle eine vollständige Datei, um ein bytegenaues Ergebnis zu erhalten.

interaction-standalone-close-error = Mit Fehler schließen

interaction-standalone-compare-title = Dateien vergleichen

interaction-standalone-computed-explanation = Ersetzt ungespeicherte Bearbeitungen in diesem Fenster. MERGED auf dem Datenträger bleibt bis „Speichern und abschließen“ unverändert.

interaction-standalone-computed-heading = Arbeitsergebnis durch berechneten Merge ersetzen

interaction-standalone-done = Fertig

interaction-standalone-error-access-limit = Die Zugriffsmetadaten der Ausgabe überschreiten 1 MiB

interaction-standalone-error-access-limit-unchanged = Die Zugriffsmetadaten der Ausgabe überschreiten 1 MiB; das Original wurde nicht geändert

interaction-standalone-error-access-read-changed = Die Zugriffsmetadaten der Ausgabe haben sich beim Lesen geändert; vor dem Speichern erneut öffnen

interaction-standalone-error-access-snapshot = Die Zugriffsmetadaten der Ausgabe können nicht erfasst werden: { $detail }

interaction-standalone-error-acl-identity-changed = Die Identität der Ausgabe hat sich beim Lesen der nativen ACL geändert

interaction-standalone-error-acl-limit = Die native ACL überschreitet die Grenzen der begrenzten Momentaufnahme

interaction-standalone-error-atomic-install = Dieses Dateisystem kann ein Ergebnis nicht atomar und ohne Überschreiben einsetzen: { $detail }

interaction-standalone-error-before-install = Ausgabe, Eingaben oder Zugriffsmetadaten haben sich vor dem Einsetzen geändert

interaction-standalone-error-binary-inputs = Binäre oder nicht UTF-8-kodierte Eingaben: Wähle eine vollständige Seite, um die Bytes exakt zu erhalten

interaction-standalone-error-choose-input = Wähle eine vollständige Eingabedatei oder bereite ein Textergebnis vor

interaction-standalone-error-clone-metadata = Dieses Dateisystem kann ACL und Metadaten der Ausgabe nicht sicher klonen: { $detail }

interaction-standalone-error-close-writers = Schließe vor dem Speichern andere schreibende Programme: { $detail }

interaction-standalone-error-comparison-cancelled = Dateivergleich abgebrochen

interaction-standalone-error-comparison-limit = Der Dateivergleich ist auf 2 MiB pro Datei begrenzt

interaction-standalone-error-computed-encoding = Der berechnete Merge ist nicht UTF-8-kodiert

interaction-standalone-error-computed-limit = Das berechnete Ergebnis überschreitet 2 MiB; wähle eine vollständige Eingabedatei

interaction-standalone-error-create-reservation = Die Speicherreservierung konnte nicht erstellt werden: { $detail }

interaction-standalone-error-descriptor-response = Ungültige Antwort für den Zugriffsdeskriptor

interaction-standalone-error-during-save = Dateien haben sich beim Speichern geändert. Das aktuelle Ergebnis und die verdrängten Bytes blieben erhalten; prüfe sie vor dem Fortfahren

interaction-standalone-error-executable = Diese ausführbare Datei konnte nicht gefunden werden: { $detail }

interaction-standalone-error-expected-invocation = Ein Aufruf eines eigenständigen Dateiwerkzeugs wird erwartet

interaction-standalone-error-expected-tool = Ein eigenständiges Dateiwerkzeug wird erwartet

interaction-standalone-error-file-arity = { $count ->
        [one] Genau ein Dateipfad wird erwartet
       *[other] Genau { $count } Dateipfade werden erwartet
    }
    { $usage }

interaction-standalone-error-file-read-changed = { $path } hat sich beim Lesen geändert; das Dateiwerkzeug erneut öffnen

interaction-standalone-error-filesystem-metadata = Dieses Dateisystem kann die Zugriffsmetadaten der Ausgabe nicht sicher bereitstellen; das Original wurde nicht geändert

interaction-standalone-error-foreign-lock = Beim Freigeben wurde eine fremde Speichersperre erfasst; sie bleibt im Wiederherstellungsordner und wird nur bei freiem Ziel wiederhergestellt

interaction-standalone-error-index-metadata-preserve = Die Zugriffsmetadaten des Index konnten nicht erhalten werden

interaction-standalone-error-index-policy-preserve = Die vollständige Zugriffsrichtlinie des Index konnte nicht erhalten werden

interaction-standalone-error-index-policy-unsupported = Die Zugriffsrichtlinie des Index wird auf dieser Plattform nicht unterstützt

interaction-standalone-error-input-changed = Die Eingabe { $path } hat sich geändert; vor dem Speichern erneut öffnen

interaction-standalone-error-line-limit = Eine Eingabedatei hat mehr als 20.000 Zeilen; wähle eine vollständige Seite

interaction-standalone-error-lock-changed = Die Speichersperre wurde geändert oder ist verschwunden; fremde Einträge blieben erhalten

interaction-standalone-error-lock-parent = Der übergeordnete Ordner der Speichersperre hat sich beim Freigeben geändert; der erfasste Eintrag blieb erhalten

interaction-standalone-error-lock-release = Die Freigabe der Speichersperre ist ungewiss: { $detail }

interaction-standalone-error-merge-cancelled = Datei-Merge abgebrochen

interaction-standalone-error-merged-before-metadata = MERGED hat sich vor der Vorbereitung der Metadaten geändert

interaction-standalone-error-merged-changed = MERGED hat sich seit dem Öffnen geändert; vor dem Speichern erneut öffnen. Die aktuelle Datei blieb erhalten.

interaction-standalone-error-metadata-list-changed = Die Ausgabemetadaten haben sich beim Auflisten geändert

interaction-standalone-error-metadata-overflow = Überlauf bei den Ausgabemetadaten

interaction-standalone-error-metadata-read-changed = Die Ausgabemetadaten haben sich beim Lesen geändert

interaction-standalone-error-native-attributes = Nicht unterstützte native Ausgabeattribute; das Original wurde nicht geändert

interaction-standalone-error-native-basename = Es wird genau ein nativer Dateiname ohne Verzeichnispfad erwartet

interaction-standalone-error-no-computed-merge = Es ist kein berechneter Text-Merge verfügbar

interaction-standalone-error-no-output-identity = Keine Identität der Ausgabedatei

interaction-standalone-error-no-output-parent = Kein übergeordnetes Ausgabeverzeichnis

interaction-standalone-error-no-overwrite = Einsetzen ohne Überschreiben war nicht möglich; ein gleichzeitig angelegtes Ziel blieb erhalten: { $detail }

interaction-standalone-error-no-pinned-source = Keine fixierte Quelldatei

interaction-standalone-error-no-result-parent = Kein übergeordnetes Verzeichnis für das Merge-Ergebnis

interaction-standalone-error-no-source-identity = Keine Identität der Quelldatei

interaction-standalone-error-no-source-policy = Keine erfasste Zugriffsrichtlinie der Quelle

interaction-standalone-error-open-file = { $path } konnte nicht geöffnet werden: { $detail }

interaction-standalone-error-open-parent = Der übergeordnete Ausgabeordner konnte nicht geöffnet werden: { $detail }

interaction-standalone-error-output-acl = Die ACL der Ausgabe konnte nicht erhalten werden: { $detail }

interaction-standalone-error-output-alias = MERGED verweist auf eine Eingabedatei; wähle eine separate Ausgabe, damit die Eingaben schreibgeschützt bleiben

interaction-standalone-error-output-empty-device = Ein Gerät für leere Eingaben kann nicht als Merge-Ausgabe dienen

interaction-standalone-error-output-filename = Das Merge-Ergebnis benötigt einen Dateinamen

interaction-standalone-error-output-limit = Die Merge-Ausgabe überschreitet 2 MiB

interaction-standalone-error-output-link = Die Merge-Ausgabe ist ein Link oder Reparse-Punkt

interaction-standalone-error-output-ownership = Die Eigentümer der Ausgabe konnten nicht erhalten werden: { $detail }

interaction-standalone-error-output-plain-file = MERGED muss eine reguläre Datei sein, kein Link oder Gerät: { $detail }

interaction-standalone-error-output-readonly = Die Merge-Ausgabe ist schreibgeschützt

interaction-standalone-error-output-type = Der Typ der Ausgabe hat sich geändert

interaction-standalone-error-parent-identity = Die Identität des übergeordneten Ausgabeordners hat sich geändert; das Dateiwerkzeug erneut öffnen

interaction-standalone-error-parent-type = Der übergeordnete Ausgabeordner enthält einen Link oder ist kein Verzeichnis

interaction-standalone-error-platform-metadata = Die Zugriffsmetadaten dieser Plattform können nicht sicher erhalten werden; die ursprüngliche Ausgabe wurde nicht geändert

interaction-standalone-error-prepare-merge = Git konnte keinen Dreiwege-Merge vorbereiten; wähle eine vollständige Eingabedatei oder bearbeite die vorhandene MERGED-Datei

interaction-standalone-error-prepared-policy = Die vorbereitete Ausgabe hat die erfasste Zugriffsrichtlinie nicht beibehalten; das Original ist unverändert

interaction-standalone-error-preserve-metadata = Die Zugriffsmetadaten der Ausgabe konnten nicht erhalten werden: { $detail }

interaction-standalone-error-previous-result = Das vorherige Ergebnis konnte nicht erhalten werden: { $detail }

interaction-standalone-error-recovery-closed = Der Wiederherstellungsordner ist geschlossen

interaction-standalone-error-recovery-location = { $detail }. Wiederherstellungsordner: { $path }

interaction-standalone-error-recovery-reserve = Ein Wiederherstellungsordner konnte nicht reserviert werden

interaction-standalone-error-registration-partial = { $installed } von { $total } Einstellungen wurden eingerichtet; { $key } konnte nicht geschrieben werden: { $detail }. Versuche es erneut oder führe die in der Vorschau gezeigten Befehle manuell aus.

interaction-standalone-error-regular-file = { $path } muss eine reguläre Datei mit höchstens 2 MiB sein

interaction-standalone-error-replacement-acl = Die ACL der Ersatzdatei weicht von der erfassten Zugriffsrichtlinie der Ausgabe ab

interaction-standalone-error-replacement-identity = Die Identität der Ersatzdatei ist nicht sicher

interaction-standalone-error-replacement-reparse = Die Ersatzdatei ist ein Reparse-Punkt

interaction-standalone-error-repository-arity = Genau ein Repository-Pfad wird erwartet
    { $usage }

interaction-standalone-error-reserve-result = Das Ergebnis konnte nicht reserviert werden; möglicherweise gehört die Sperre einem anderen Speichervorgang: { $detail }

interaction-standalone-error-save-and-release = { $error }. { $release }

interaction-standalone-error-save-identity = MERGED oder sein übergeordneter Ordner hat sich beim Speichern geändert; der Speichervorgang wurde nicht als erfolgreich gemeldet

interaction-standalone-error-save-maybe-installed = Der Speichervorgang hat das Ergebnis möglicherweise bereits eingesetzt; { $detail }

interaction-standalone-error-system-root-absolute = SystemRoot muss ein absoluter nativer Pfad sein

interaction-standalone-error-system-root-unavailable = SystemRoot ist nicht verfügbar; die Zugriffsregeln der Ausgabe können nicht sicher erhalten werden

interaction-standalone-error-unknown-option = Unbekannte Option: { $option }
    { $usage }

interaction-standalone-error-windows-copy-response = Unerwartete Antwort beim Kopieren der Windows-Metadaten

interaction-standalone-error-windows-metadata = Die Windows-Zugriffsmetadaten konnten nicht erhalten werden; das Original ist unverändert: { $detail }

interaction-standalone-error-worker-stopped = Der Hintergrundauftrag des Dateiwerkzeugs wurde beendet

interaction-standalone-inputs-heading = Base- / lokale / Remote-Eingaben anzeigen

interaction-standalone-keep-merged = Aktuelle MERGED-Datei behalten

interaction-standalone-local = Lokal

interaction-standalone-merge-title = Dateien zusammenführen

interaction-standalone-merged-explanation = Die vorhandene MERGED-Datei bleibt anfangs unverändert. Beim Speichern bleibt die vorherige Datei in einem benachbarten Wiederherstellungsordner erhalten.

interaction-standalone-merged-path = MERGED: { $path }

interaction-standalone-previous-location = Die vorherige MERGED-Datei blieb unter { $path } erhalten

interaction-standalone-reading = Dateikopien werden gelesen…

interaction-standalone-registration-complete = { $count ->
        [one] Eine Einstellung für benannte Werkzeuge wurde registriert. Verwende git difftool --tool=gitcraft oder git mergetool --tool=gitcraft.
       *[other] { $count } Einstellungen für benannte Werkzeuge wurden registriert. Verwende git difftool --tool=gitcraft oder git mergetool --tool=gitcraft.
    }

interaction-standalone-registration-copy = Git-Werkzeugbefehle kopieren

interaction-standalone-registration-defaults = gitcraft auch als Standardwerkzeug für Diff und Merge festlegen

interaction-standalone-registration-explanation = Schreibt nur die folgenden Einstellungen in die globale Git-Konfiguration dieses Rechners. Es wird weder ein Repository oder Profil geladen noch ein Merge oder Dateivergleich ausgeführt.

interaction-standalone-registration-pending = Die aufgeführten Git-Einstellungen werden geschrieben…

interaction-standalone-registration-register = Benannte Git-Werkzeuge registrieren

interaction-standalone-remote = Remote

interaction-standalone-save-finish = Speichern und abschließen

interaction-standalone-saving = Der abgesicherte Speichervorgang wird abgeschlossen. Das Schließen wartet auf sein Ergebnis.

interaction-standalone-snapshot-summary = Schreibgeschützte Eingabekopien · 2 MiB pro Datei · keine Repository-Aktionen

interaction-standalone-truncated-preview = { $preview }
    … Eingabevorschau gekürzt

interaction-standalone-usage = Verwendung:
      gitcraft [REPOSITORY]
      gitcraft --diff [--] BEFORE AFTER
      gitcraft --merge [--] BASE LOCAL REMOTE MERGED
      gitcraft --mergetool [--] BASE LOCAL REMOTE MERGED

    Dateiwerkzeuge öffnen ein eigenes Fenster und suchen niemals nach einem Repository.
    Diff ist schreibgeschützt. Merge schreibt MERGED erst nach „Speichern und abschließen“.
    Exitcode: 0 abgeschlossen, 1 abgebrochen, 2 Fehler bei Aufruf, Lesen oder Speichern.
    Verwende -- vor einem Repository-Namen, der mit - beginnt. { "\u000A" }

interaction-standalone-use-base = Base-Datei verwenden

interaction-standalone-use-computed = Berechneten Merge verwenden

interaction-standalone-use-local = Lokale Datei verwenden

interaction-standalone-use-remote = Remote-Datei verwenden

interaction-standalone-window-title = GitCraft-Dateiwerkzeuge
