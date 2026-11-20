# Datei-Metadaten ohne Terminal ändern: das Ausführbar-Bit, Umbenennen nur der Groß- und
# Kleinschreibung, und Modus- und Namensänderungen getrennt von den Zeilen eines Diffs (#173, Punkt 13).

file-metadata-executable-menu = Ausführbar-Bit im Index…
file-metadata-executable-menu-tip = Das Ausführbar-Bit des Index-Eintrags der Datei setzen oder entfernen. Nur dieser Modus ändert sich: gestagte und ungestagte Inhalte bleiben, wie sie sind. Funktioniert auch unter Windows, wo Dateien auf der Festplatte kein Ausführbar-Bit haben.
file-metadata-case-menu = Groß-/Kleinschreibung des Namens ändern…
file-metadata-case-menu-tip = Die Datei in denselben Namen mit anderer Groß-/Kleinschreibung umbenennen, sicher auf Dateisystemen ohne Unterscheidung (Windows, macOS). Änderungen wandern mit.
file-metadata-executable-title = Ausführbar-Bit im Index
file-metadata-case-title = Groß-/Kleinschreibung des Namens ändern
file-metadata-apply = Anwenden
file-metadata-rename = Umbenennen
file-metadata-executable-change = { $executable ->
    [true] Index-Modus { $from } → { $to }: ausführbar
   *[other] Index-Modus { $from } → { $to }: nicht ausführbar
}
file-metadata-executable-kept = Nur der Modus des Index-Eintrags ändert sich. Der Inhalt der Datei, gestagt oder nicht, und die Datei im Arbeitsverzeichnis bleiben, wie sie sind; committen Sie, um den neuen Modus festzuhalten.
file-metadata-case-kept = Die Datei wird über einen temporären Namen umbenannt, das klappt auch auf Dateisystemen ohne Unterscheidung der Groß-/Kleinschreibung. Gestagte und ungestagte Änderungen wandern unverändert mit.
file-metadata-refused-conflicted = Die Datei hat einen Konflikt. Lösen Sie ihn zuerst.
file-metadata-refused-untracked = Die Datei ist nicht im Index.
file-metadata-refused-not-regular = Nur normale Dateien haben ein Ausführbar-Bit (der Modus dieser ist { $mode }).
file-metadata-refused-bad-name = Geben Sie einen Dateinamen ohne Ordner ein.
file-metadata-refused-same-name = Geben Sie den Namen in anderer Groß-/Kleinschreibung ein.
file-metadata-refused-not-case-only = Hier kann sich nur die Groß-/Kleinschreibung ändern. Für andere Umbenennungen nehmen Sie ein Terminal oder Ihren Editor.
file-metadata-refused-missing = Die Datei ist im Arbeitsverzeichnis gelöscht.
file-metadata-refused-collision = { $name } gibt es schon in anderer Groß-/Kleinschreibung.
file-metadata-refused-no-temp = Im Ordner wurde kein freier temporärer Name gefunden.
file-metadata-mode-regular = Datei
file-metadata-mode-executable = ausführbar
file-metadata-mode-link = symbolischer Link
file-metadata-mode-submodule = Submodul
file-metadata-diff-mode = Modus { $old } ({ $old-kind }) → { $new } ({ $new-kind })
file-metadata-diff-renamed = { $similarity ->
    [none] Umbenannt von { $from }
   *[other] Umbenannt von { $from } · { $similarity } % ähnlich
}
file-metadata-diff-copied = { $similarity ->
    [none] Kopiert von { $from }
   *[other] Kopiert von { $from } · { $similarity } % ähnlich
}
file-metadata-changed = Die Datei hat sich geändert, seit der Dialog geöffnet wurde. Prüfen Sie, was jetzt angezeigt wird, und bestätigen Sie erneut.
file-metadata-file-mode-warning = Git verfolgt hier Dateimodi (core.fileMode), und die Datei auf der Festplatte behält ihre alte Berechtigung: Git zeigt die umgekehrte Änderung als ungestagt, und alle Dateien zu stagen oder commit -a würde dies rückgängig machen. Ändern Sie auch die Berechtigung der Datei (chmod).
file-metadata-read-failed = Die Datei konnte nicht gelesen werden. Schließen Sie den Dialog und öffnen Sie ihn erneut.
file-metadata-patch-placeholder = <Modus-Patch, wird beim Bestätigen geschrieben>
file-metadata-rename-rolled-back = { $via } konnte nicht in { $to } umbenannt werden; zurück nach { $from } verschoben
