# Repository workflow presentation. Machine identifiers and commands remain literal.

repo-workflows-activity-activity-panel-0a4cf721 = Noch keine Einträge: Hier erscheinen die Vorgänge dieses Tabs (Abrufen, Committen, Auschecken …) mit ihren Git-Befehlen und deren gesamten Ausgaben.

repo-workflows-activity-activity-panel-3756b124 = Die hier aufgeführten Vorgänge vergessen

repo-workflows-activity-activity-panel-38da1505 = Aktivität

repo-workflows-activity-activity-panel-6e279c58 = Aktivitätsbereich schließen

repo-workflows-activity-activity-panel-83b12c22 = Leeren

repo-workflows-activity-ago-4d631fe4 = { $s-3600-1 } h

repo-workflows-activity-ago-d2916a22 = { $s } s

repo-workflows-activity-ago-e823f861 = { $s-60-1 } min

repo-workflows-activity-bounded-62a56981 = { $output-head-end-1 }
    … { $left-out } Bytes ausgelassen …
    { $output-tail-start-2 }

repo-workflows-activity-entry-ui-332e9c9d = Befehle und deren gesamte Ausgaben kopieren

repo-workflows-activity-entry-ui-599c9e61 = Es wurde kein Git-Befehl ausgeführt.

repo-workflows-activity-exit-text-64df366e = nicht beendet

repo-workflows-activity-exit-text-bd2f2ddf = Exit { $c }

repo-workflows-activity-exit-text-c22995ad = Exit 0

repo-workflows-activity-outcome-text-5d28a90f = fehlgeschlagen

repo-workflows-activity-outcome-text-8b47045e = abgebrochen

repo-workflows-activity-outcome-text-a4c3ed04 = abgeschlossen

repo-workflows-activity-seconds-6f29d45b = { NUMBER($d-as-secs-f32-1, minimumFractionDigits: 1, maximumFractionDigits: 1) } s

repo-workflows-activity-seconds-af37e50f = { $d-as-secs-1 } s

repo-workflows-activity-seconds-fba4d59d = { $d-as-secs-60-1 } min { $d-as-secs-60-2 } s

repo-workflows-activity-short-command-line-c85cf117 = { $command-line-args-shown-args-1 } … { $args-len-shown-args-2 } weitere

repo-workflows-agent--14b0fc42 = Dies lässt sich in gitcraft nicht rückgängig machen.

repo-workflows-agent-agent-decide-7695bed1 = Versuchen Sie es nur erneut, wenn der Benutzer Sie darum bittet.

repo-workflows-agent-agent-needs-sign-in-dc823c53 = Für { $label } muss sich der Benutzer am Remote anmelden. gitcraft hat ihn dazu aufgefordert. Versuchen Sie es nach der Anmeldung erneut.

repo-workflows-agent-agent-op-done-16d1e9b2 = { $running-agent-1 }: { $label }: abgeschlossen

repo-workflows-agent-agent-op-done-79a8519c = { $label }: abgeschlossen. { $how }

repo-workflows-agent-agent-op-done-8336fbfe = { $label } fehlgeschlagen: { $e }

repo-workflows-agent-agent-op-done-cf3e44cf = Dies lässt sich in gitcraft nicht rückgängig machen.

repo-workflows-agent-agent-op-done-d5e3a3af = Dies ist der letzte Schritt in der Rückgängig-Liste von gitcraft. Der Benutzer kann ihn mit Ctrl+Z rückgängig machen.

repo-workflows-agent-agent-planned-14b0fc42 = Dies lässt sich in gitcraft nicht rückgängig machen.

repo-workflows-agent-agent-start-917ed8ba = gitcraft konnte den Vorgang nicht starten. Versuchen Sie es erneut.

repo-workflows-agent-agent-step-97d87ec1 = Innerhalb von { $agents-approval-wait-as-secs-1 } Sekunden hat niemand auf die Freigabekarte in gitcraft reagiert. Es wurde nichts ausgeführt. Bitten Sie den Benutzer, vor einem erneuten Versuch auf die Karte in gitcraft zu achten.

repo-workflows-agent-agent-step-c03ca088 = gitcraft war { $queue-wait-as-secs-60-1 } Minuten lang beschäftigt (anderer Vorgang oder Freigabekarte). Es wurde nichts ausgeführt. Versuchen Sie es erneut.

repo-workflows-agent-branch-7776bacf = HEAD ist losgelöst. Checken Sie zuerst einen Branch aus.

repo-workflows-agent-check-branch-name-89d905bf = `{ $name }` ist kein gültiger Branchname

repo-workflows-agent-checked-paths-0e0e013b = Geben Sie mindestens einen Pfad an

repo-workflows-agent-checkout-31e4be52 = { $branch } ist bereits ausgecheckt

repo-workflows-agent-checkout-734452f6 = { $branch } auschecken

repo-workflows-agent-checkout-7a3686d6 = Auschecken

repo-workflows-agent-checkout-994822a7 = Es gibt keinen lokalen Branch namens { $branch } (erstellen Sie ihn mit `create_branch`)

repo-workflows-agent-commit-4475f515 = Nachbessern

repo-workflows-agent-commit-82a9c46f = Committen

repo-workflows-agent-commit-abf5ac58 = Es gibt noch keinen Commit zum Nachbessern

repo-workflows-agent-commit-d5a3b056 = Nichts vorgemerkt. Merken Sie zuerst die Änderungen für den Commit vor (Tool `stage`).

repo-workflows-agent-commit-of-8c23b76d = `{ $rev }` ist kein Commit in diesem Repository

repo-workflows-agent-create-branch-8464d926 = Ein Branch namens { $name } existiert bereits

repo-workflows-agent-create-branch-d2a49cac = Branch { $name } erstellen

repo-workflows-agent-delete-branch-55a975e8 = Es gibt keinen lokalen Branch namens { $name }

repo-workflows-agent-delete-branch-7ef8245e = Ctrl+Z erstellt ihn erneut an diesem Commit.

repo-workflows-agent-delete-branch-94d21a83 = { $name } ist ausgecheckt. Checken Sie zuerst einen anderen Branch aus.

repo-workflows-agent-delete-branch-d32d800a = Branch { $name } löschen

repo-workflows-agent-hard-ask-19b9c2f0 = Die uncommitteten Änderungen an einer versionierten Datei gehen unwiderruflich verloren.

repo-workflows-agent-hard-ask-8de2bf44 = Ctrl+Z setzt { $target } anschließend zurück. Die verworfenen Änderungen bleiben verloren.

repo-workflows-agent-hard-ask-996b546d = Die uncommitteten Änderungen an { $n } versionierten Dateien gehen unwiderruflich verloren.

repo-workflows-agent-hard-ask-c05b9dac = Derzeit gibt es keine uncommitteten Änderungen an versionierten Dateien.

repo-workflows-agent-hard-ask-e720ce31 = { $target } hart auf { $short-hash-1 } zurücksetzen

repo-workflows-agent-mixed-ask-2c1fd48b = { $target } auf { $short-hash-1 } zurücksetzen (mixed)

repo-workflows-agent-mixed-ask-2ddd09f5 = Ctrl+Z setzt { $target } anschließend zurück, merkt die Änderungen jedoch nicht erneut vor.

repo-workflows-agent-not-in-progress-1049b227 = Ein Vorgang ({ $state-label-to-lowercase-1 }) läuft. { $what } würde ihn unwiderruflich beenden. Schließen Sie ihn zuerst ab oder brechen Sie ihn ab (siehe `conflicts`).

repo-workflows-agent-paths-label-26d36d06 = { $many-len-1 } Pfade

repo-workflows-agent-plan-0026c505 = Zurücknehmen

repo-workflows-agent-plan-4c49fdb6 = Cherry-Pick

repo-workflows-agent-push-1d502fc3 = Force-Push

repo-workflows-agent-push-2d7f2003 = Nichts zu pushen: { $branch } hat keine Commits, die { $remote }/{ $there } fehlen

repo-workflows-agent-push-56d04849 = { $to } mit { $branch } überschreiben. { $gone } Git verweigert dies, wenn sich { $to } seit dem letzten Abruf geändert hat.

repo-workflows-agent-push-731ce7ed = Push

repo-workflows-agent-push-747bd6a7 = { $branch } als neuen Branch { $there } an { $remote } senden und ihn verfolgen.

repo-workflows-agent-push-7d6400fd = { $branch } auf { $remote } veröffentlichen

repo-workflows-agent-push-a74bf451 = Nach dem letzten Abruf hat { $to } keine Commits, die { $branch } fehlen.

repo-workflows-agent-push-cb055d06 = { $branch } nach { $to } pushen

repo-workflows-agent-push-ccafdcaa = Force-Push von { $branch } nach { $to }

repo-workflows-agent-push-e5edacbd = { $branch } hat noch keine Commits zum Pushen

repo-workflows-agent-push-f86f9688 = Dieses Repository hat kein Remote zum Pushen

repo-workflows-agent-read-23b028a2 = Losgelöster HEAD

repo-workflows-agent-read-e72061c5 = Ghost-Vorschau nicht verfügbar: { $e }

repo-workflows-agent-rename-branch-00b004f0 = Es gibt keinen lokalen Branch namens { $from }

repo-workflows-agent-rename-branch-4fe447cd = Ein Branch namens { $to } existiert bereits

repo-workflows-agent-rename-branch-5de8c7a7 = { $from } in { $to } umbenennen

repo-workflows-agent-reset-5247bb50 = { $target } steht bereits auf { $short-hash-1 }

repo-workflows-agent-reset-7c10af6a = Es gibt noch keine Commits, auf die zurückgesetzt werden könnte

repo-workflows-agent-reset-c6ebf3c0 = Zurücksetzen

repo-workflows-agent-rewrite-177f568d = Ctrl+Z stellt den aktuellen Stand von { $branch } wieder her.

repo-workflows-agent-rewrite-2630fe5c = Commit nach unten verschieben

repo-workflows-agent-rewrite-3d1ccb79 = Commit nach oben verschieben

repo-workflows-agent-rewrite-601e2a07 = Commit verwerfen

repo-workflows-agent-rewrite-7d959a5b = Ein Vorgang ({ $state-label-to-lowercase-1 }) läuft. Schließen Sie ihn zuerst ab oder brechen Sie ihn ab.

repo-workflows-agent-rewrite-fc4198eb = Commit { $short-hash-1 } verwerfen

repo-workflows-agent-rewrite-ff973443 = Die Nachricht wird mit `commit` und `amend` geändert

repo-workflows-agent-stage-17e7b517 = Konflikte als gelöst markieren

repo-workflows-agent-stage-3b4ddea7 = { $paths-label-paths-1 } vormerken

repo-workflows-agent-stash-38bdfeaa = Stash

repo-workflows-agent-stash-4fce16d0 = Stashen

repo-workflows-agent-stash-a7f01d16 = Es gibt keine Änderungen zum Stashen

repo-workflows-agent-stash-ee431f75 = Die Stash-Nachricht darf nur eine Zeile mit höchstens 1000 Bytes enthalten

repo-workflows-agent-unstage-9d879ab6 = { $conflicted-join-1 } enthält Konflikte. Lösen Sie diese zuerst (das Entfernen aus der Staging-Area würde sie beenden).

repo-workflows-agent-unstage-a79ec226 = In { $paths-join-1 } ist nichts vorgemerkt

repo-workflows-agent-unstage-eb003390 = { $paths-label-paths-1 } aus der Staging-Area entfernen

repo-workflows-agents-after-worktree-add-8fb6d5f3 = Einrichtung

repo-workflows-agents-after-worktree-add-9fd23075 = Die Checkpoint-Hooks des Agenten wurden nicht installiert: { $e }

repo-workflows-agents-after-worktree-add-a2ffe669 = Das Einrichtungsrezept für den Worktree wurde nicht ausgeführt: { $e }

repo-workflows-agents-agent-act-8fb6d5f3 = Einrichtung

repo-workflows-agents-agent-choice-11b39c93 = Agent

repo-workflows-agents-agent-choice-2b8f8124 = { $cli-label-1 } unterstützt keine erste Nachricht. Geben Sie sie dort ein.

repo-workflows-agents-agent-choice-36a10f87 = Installiert die Agent-Hooks im Worktree: Nach jedem Durchgang führen sie gitcraft checkpoint aus. Unter „Uncommittete Änderungen“ lassen sich die Durchgänge dann einzeln vergleichen.

repo-workflows-agents-agent-choice-57b9f7ac = Eigener Befehl

repo-workflows-agents-agent-choice-92a24920 = Wird mit sh im Worktree ausgeführt, z. B. claude --model opus

repo-workflows-agents-agent-choice-c8daf5c0 = Aufgabe als erste Nachricht übergeben

repo-workflows-agents-agent-choice-e383f131 = Jeden Durchgang einzeln zur Prüfung speichern

repo-workflows-agents-agent-section-370f346f = AGENTEN

repo-workflows-agents-agent-section-736dbccf = Keine Agent-Aufgaben. + startet eine Aufgabe in einem eigenen Worktree.

repo-workflows-agents-agent-section-a6153a60 = Stellt die Worktrees der Aufgaben nebeneinander, um die besten Änderungen zusammenzustellen

repo-workflows-agents-agent-section-c37ddff4 = Aufgaben vergleichen (Bake-off) …

repo-workflows-agents-agent-section-d3a1dde8 = Agent-Aufgabe starten …

repo-workflows-agents-agent-turns-94377c68 = AGENT-DURCHGÄNGE

repo-workflows-agents-agent-turns-9a299007 = Nach jedem Durchgang von den Agent-Hooks gespeichert. Klicken Sie auf einen Durchgang, um seine Änderungen zu sehen. Sein Menü vergleicht ihn mit dem aktuellen Dateistand.

repo-workflows-agents-cancel-only-19766ed6 = Abbrechen

repo-workflows-agents-forget-turns-1d1d391e = Die Durchgänge wurden nicht gelöscht: { $e }

repo-workflows-agents-hover-0b4191eb = Durchgang { $turn-number-1 } · { $when }
    
    { $prompt }

repo-workflows-agents-hover-eae937e9 = (kein Prompt aufgezeichnet)

repo-workflows-agents-install-hooks-2f5d6dc4 = Agent-Hooks werden installiert …

repo-workflows-agents-install-hooks-a8f2778c = Die Agent-Hooks wurden nicht installiert: { $e }

repo-workflows-agents-menu-04eb42a7 = Dort ein Terminal öffnen

repo-workflows-agents-menu-11b6670c = Dieser Agent-Worktree fehlt

repo-workflows-agents-menu-13b97894 = Änderungen öffnen und prüfen

repo-workflows-agents-menu-24b65605 = Worktree und Branch bleiben erhalten. Entfernen Sie sie unter WORKTREES.

repo-workflows-agents-menu-7b4c6a5c = Aus der Liste entfernen

repo-workflows-agents-menu-b1e3c6b9 = Dieser Agent-Worktree fehlt oder wird noch vorbereitet

repo-workflows-agents-menu-cbc89e47 = Agent erneut starten

repo-workflows-agents-menu-d4604b9b = In einem neuen Terminal des Worktrees, ohne Einrichtungsrezept und ohne die Aufgabe als erste Nachricht

repo-workflows-agents-note-panel-4254e046 = Schreiben Sie zuerst die Notiz

repo-workflows-agents-note-panel-63565c04 = Notiz hinzufügen

repo-workflows-agents-note-panel-961b2a04 = Schließen (Esc)

repo-workflows-agents-note-panel-bb064d10 = Was der Agent hier ändern soll

repo-workflows-agents-note-panel-d065164e = Bleibt in diesem Worktree, bis Sie die Notizen an den Agenten senden

repo-workflows-agents-problem-5fbb1fb2 = Benennen Sie den Branch, auf dem der Agent arbeiten soll

repo-workflows-agents-problem-6eabbce8 = Geben Sie den vollständigen Pfad auf diesem Rechner an (/home/…). Git liest ~ nicht.

repo-workflows-agents-problem-80f33652 = Ein Branch namens { $branch } existiert bereits. Wählen Sie einen anderen Namen.

repo-workflows-agents-problem-8bffbdf3 = Ein Branchname darf nicht mit - beginnen

repo-workflows-agents-problem-97c58cf1 = Geben Sie den Befehl zum Starten des Agenten ein

repo-workflows-agents-problem-bbc12752 = Wählen Sie den Ordner für den Worktree

repo-workflows-agents-problem-c3177bf6 = Geben Sie den Ausgangspunkt des Branches an: Branch, Tag oder Commit

repo-workflows-agents-save-recipe-97381b52 = Einrichtungsrezept für den Worktree gespeichert

repo-workflows-agents-send-to-agent-4e9f0022 = { $what } konnte nicht an das Terminal { $label } gesendet werden: { $e }

repo-workflows-agents-send-to-agent-a6f80242 = In diesem Worktree gibt es kein Agent-Terminal für { $what }. Starten Sie den Agenten (Agent-Aufgabe starten …) oder wählen Sie sein Terminal neben „An Agent senden“.

repo-workflows-agents-session-header-16959302 = Seit der letzten Prüfung vergleichen …

repo-workflows-agents-session-menu-816f49fb = Alles seit der letzten Prüfung vergleichen …

repo-workflows-agents-session-menu-835a56f7 = Die gespeicherten Durchgänge werden gelöscht. Die Dateien bleiben unverändert.

repo-workflows-agents-session-menu-c3cada37 = Durchgänge dieser Sitzung löschen

repo-workflows-agents-start-agent-task-d4ec2011 = Agent-Aufgabe { $new-branch-1 } starten

repo-workflows-agents-start-agent-task-f80416ff = Warten Sie, bis der laufende Vorgang beendet ist

repo-workflows-agents-start-launch-tracked-d71ae829 = { $launch-label-1 } konnte nicht gestartet werden: { $e }

repo-workflows-agents-sub-25ba7976 = Worktree entfernt

repo-workflows-agents-sub-308560dc = Änderungen werden gezählt …

repo-workflows-agents-sub-32dfcdbc = Eine Datei geändert

repo-workflows-agents-sub-96b72d81 = Ordner fehlt: Bereinigen Sie ihn unter WORKTREES

repo-workflows-agents-sub-a7109ae6 = { $n } Dateien geändert

repo-workflows-agents-sub-b2c35ddf = keine uncommitteten Änderungen

repo-workflows-agents-terminal-choice-c0d8980e = an { $shown }

repo-workflows-agents-terminal-choice-c22c5ccf = Das Terminal, in das die Notizen eingefügt werden

repo-workflows-agents-terminal-choice-f1f46570 = kein Agent-Terminal

repo-workflows-agents-tip-1c0ba476 = { "\u000A" }In diesem Tab geöffnet: Die Änderungen stehen unter „Uncommittete Änderungen“

repo-workflows-agents-tip-5c1b8dd5 = { "\u000A" }Klicken Sie hier, um die Änderungen zu öffnen und zu prüfen

repo-workflows-agents-tip-7d8e5553 = { "\u000A" }Aufgabe: { $row-task-prompt-1 }

repo-workflows-agents-tip-95d5fbf7 = { "\u000A" }Agent: { $row-task-agent-1 }

repo-workflows-agents-tray-act-78e896bb = die Notizen

repo-workflows-agents-tray-panel-0451c67e = Diese Notiz entfernen

repo-workflows-agents-tray-panel-1b67e032 = NOTIZEN FÜR DEN AGENTEN · { $tray-notes-len-1 }

repo-workflows-agents-tray-panel-366ec196 = Kein Agent-Terminal vorhanden. Starten Sie den Agenten (Agent-Aufgabe starten …) oder wählen Sie sein Terminal.

repo-workflows-agents-tray-panel-77a860cb = An Agent senden

repo-workflows-agents-tray-panel-83b12c22 = Leeren

repo-workflows-agents-tray-panel-86de1f26 = Nur hier: Sie gehen ausschließlich an das Terminal des Agenten

repo-workflows-agents-tray-panel-a2c648eb = Fügt sie als eine Nachricht in das Agent-Terminal ein und leert danach die Liste

repo-workflows-agents-tray-panel-ae4eaa14 = { $tray-notes-len-1 } Notizen löschen?

repo-workflows-agents-turn-chip-1d35c7ca = Alles seit diesem Durchgang

repo-workflows-agents-turn-chip-60c8b8ae = nach Durchgang { $turn-number-1 }

repo-workflows-agents-turn-chip-a246e9f1 = Änderungen dieses Durchgangs

repo-workflows-agents-turn-chip-a3b0355d = Hunks prüfen / nur diesen Durchgang zurücknehmen …

repo-workflows-agents-turn-chip-eaf6cf58 = Diesen Durchgang mit dem aktuellen Dateistand vergleichen

repo-workflows-agents-turn-chip-ed5eb9a3 = jetzt

repo-workflows-agents-turn-diff-9021b8b9 = Durchgang { $turn-number-1 }: { $label }

repo-workflows-agents-turn-diff-dc601e7b = vor Durchgang { $turn-number-1 }

repo-workflows-agents-ui-136eaccc = Aus dem Haupt-Worktree kopieren, ein Pfad pro Zeile

repo-workflows-agents-ui-1509f561 = Speichern

repo-workflows-agents-ui-196e15ae = Führt die Eingabe in einem Terminal dieses Worktrees aus (ohne sie zu speichern)

repo-workflows-agents-ui-19766ed6 = Abbrechen

repo-workflows-agents-ui-1fe95550 = In diesem Worktree ausführen

repo-workflows-agents-ui-27cec493 = Was der Agent tun soll (optional)

repo-workflows-agents-ui-3553b76b = Ein eigener Branch und Worktree, dann ein Terminal darin mit dem Agenten.

repo-workflows-agents-ui-44e1cc2b = { $crate-manual-command-1 }+Enter zum Starten

repo-workflows-agents-ui-4bc74b21 = Aufgabe

repo-workflows-agents-ui-52656e81 = Branch

repo-workflows-agents-ui-5c81d3ac = Mit dem Haupt-Worktree verknüpfen, ein Pfad pro Zeile

repo-workflows-agents-ui-6f521989 = Korrigieren Sie zuerst die rot markierten Angaben

repo-workflows-agents-ui-7227fcf5 = Das ausgeführte Skript

repo-workflows-agents-ui-74296b58 = Falls das Repository ein Einrichtungsrezept für Worktrees hat (Worktree einrichten … in der Palette), wird es zuerst im selben Terminal ausgeführt.

repo-workflows-agents-ui-74b240bb = Einrichtungsbefehl (sh -e: stoppt beim ersten fehlgeschlagenen Befehl)

repo-workflows-agents-ui-74ccd433 = Ordner

repo-workflows-agents-ui-97c3e67d = Agent-Aufgabe starten

repo-workflows-agents-ui-beafb9a1 = Das Rezept ist leer: nichts auszuführen

repo-workflows-agents-ui-cbcf7990 = Einrichtungsrezept für Worktrees

repo-workflows-agents-ui-cd1b3ecb = Variablen, NAME=value pro Zeile ($GITCRAFT_WORKTREE_INDEX: Nummer des Worktrees, 0 für den Haupt-Worktree)

repo-workflows-agents-ui-e4bb9f1e = Starten

repo-workflows-agents-ui-e8a96feb = Ausgangspunkt

repo-workflows-agents-ui-f37424d8 = Öffnen Sie einen verknüpften Worktree dieses Repositorys, um das Rezept dort zu testen

repo-workflows-agents-ui-f8894f23 = Wird in jedem Worktree ausgeführt, den gitcraft für dieses Repository erstellt, auch ohne Agenten: in einem Terminal mit sh auf dem Rechner des Repositorys.

repo-workflows-ai-ai-message-button-04254124 = Nachricht wird geschrieben …

repo-workflows-ai-ai-message-button-05060623 = Nachricht mit { $settings-tool-short-1 } aus dem letzten Commit und den vorgemerkten Änderungen schreiben
    Rechtsklick für ein anderes Tool

repo-workflows-ai-ai-message-button-33272cf9 = Nachricht mit { $settings-tool-short-1 } aus den vorgemerkten Änderungen schreiben
    Rechtsklick für ein anderes Tool

repo-workflows-ai-ai-message-button-9553c66f = Nicht während eines Rebase oder Patches. Ändern Sie die Commit-Nachricht danach.

repo-workflows-ai-ai-message-button-a1323968 = Nachricht von einem installierten KI-Tool schreiben lassen

repo-workflows-ai-ai-message-status-48845bff = Schließen

repo-workflows-ai-ai-review-window-04dc8d04 = Review von { $review-branch-1 }

repo-workflows-ai-elapsed-127483e6 = { $secs } s

repo-workflows-ai-elapsed-fba4d59d = { $secs-60-1 } min { $secs-60-2 } s

repo-workflows-ai-message-answered-00f5dcad = Die Antwort des Tools enthielt keine Zusammenfassungszeile.

repo-workflows-ai-review-ui-19766ed6 = Abbrechen

repo-workflows-ai-review-ui-3ccec26f = { $running-tool-short-1 } prüft … { $elapsed-running-started-2 }

repo-workflows-ai-review-ui-b43c02f3 = Was { $review-branch-1 } seit der Aufspaltung zu { $review-base-2 } hinzufügt

repo-workflows-ai-review-ui-d4ea61c7 = Erneut prüfen

repo-workflows-ai-review-ui-d8b8392e = Erneut versuchen

repo-workflows-ai-review-ui-e21f935f = Kopieren

repo-workflows-ai-review-ui-f8ec77e7 = Angehalten.

repo-workflows-ai-start-review-62f579d4 = Kein KI-Tool ausgewählt. Wählen Sie eines unter Einstellungen → KI oder mit der Funkel-Schaltfläche im Commit-Feld.

repo-workflows-ai-suggestion-card-0335c833 = Meine behalten

repo-workflows-ai-suggestion-card-57a64561 = Übernehmen

repo-workflows-ai-suggestion-card-5a6b5767 = Zusammenfassung und Beschreibung damit ersetzen

repo-workflows-ai-suggestion-card-e3bbc8d9 = Die Nachricht des Tools (Ihre bleibt bis zur Auswahl erhalten):

repo-workflows-ai-tool-menu-06f0e210 = Ausschalten

repo-workflows-ai-tool-menu-0c0aa296 = { $tool-label-1 } — wird gesucht …

repo-workflows-ai-tool-menu-1440bda3 = Nach der Installation erneut nach Tools suchen

repo-workflows-ai-tool-menu-5e2d9c98 = Prompts, eigener Befehl, übertragene Datenmenge: Einstellungen → KI

repo-workflows-ai-tool-menu-671a1cd9 = { $tool-label-1 } — unter Einstellungen → KI festlegen

repo-workflows-ai-tool-menu-72f76d6c = KI-Tool für Nachrichten und Reviews (auf { $place })

repo-workflows-ai-tool-menu-77cbb6fb = Legen Sie zuerst unter Einstellungen → KI einen eigenen Befehl fest

repo-workflows-ai-tool-menu-9950df89 = Dieses Tool wurde auf dem Rechner des Repositorys nicht gefunden

repo-workflows-ai-tool-menu-9b1177a4 = { $tool-label-1 } — nicht gefunden

repo-workflows-ai-tool-menu-d36feef0 = Erneut suchen

repo-workflows-ai-tool-menu-e223809d = dieser Rechner

repo-workflows-ai-writing-row-19766ed6 = Abbrechen

repo-workflows-ai-writing-row-ea07a4e5 = { $running-tool-short-1 } schreibt die Nachricht … { $elapsed-running-started-2 }

repo-workflows-atlas-atlas-window-1316052d = Schwerpunktseite { $view-page-1-1 } / { $pages }

repo-workflows-atlas-atlas-window-1556feb2 = Autorendatum

repo-workflows-atlas-atlas-window-1af85190 = Größe

repo-workflows-atlas-atlas-window-1bfb5492 = Jüngste Änderungen

repo-workflows-atlas-atlas-window-2657941d = { $label } ({ $if-n-directory-folder-else-file-1 }), { $n-bytes-2 } Bytes, { $n-changes-3 } Änderungen, häufigster Mitwirkender: { $n-contributor-4 }. { $if-n-directory-zoom-into-folder--5 }

repo-workflows-atlas-atlas-window-34dd32fc = Atlas anwenden / aktualisieren

repo-workflows-atlas-atlas-window-39c66d83 = Dateiverlauf am fixierten Stand öffnen

repo-workflows-atlas-atlas-window-3a9dfa58 = Farbe

repo-workflows-atlas-atlas-window-41bb5a28 = In Ordner hineinzoomen

repo-workflows-atlas-atlas-window-4562ddd7 = Committerdatum

repo-workflows-atlas-atlas-window-48c740b7 = letzter gescannter Stand

repo-workflows-atlas-atlas-window-4b6c521c = { $label }
    { $n-bytes-1 } Bytes · { $n-changes-2 } Änderungen
    Häufigster Mitwirkender: { $if-n-contributor-is-empty-no-cha-3 }

repo-workflows-atlas-atlas-window-4d800030 = Bytes

repo-workflows-atlas-atlas-window-5769ad13 = Keine Dateien in diesem Stand.

repo-workflows-atlas-atlas-window-611ebb33 = Atlas der Änderungen und Mitwirkenden

repo-workflows-atlas-atlas-window-63bfe85d = Von YYYY-MM-DD

repo-workflows-atlas-atlas-window-67be181b = Gescannten Dateiverlauf über analysierte Referenzen anzeigen

repo-workflows-atlas-atlas-window-6aba5092 = Vorherige Schwerpunkte

repo-workflows-atlas-atlas-window-6e795d4d = Zeitraum

repo-workflows-atlas-atlas-window-6fd659e0 = Bis YYYY-MM-DD

repo-workflows-atlas-atlas-window-86061296 = Nächste Schwerpunkte

repo-workflows-atlas-atlas-window-862161d0 = Keine Änderungen im gewählten Zeitraum

repo-workflows-atlas-atlas-window-8d9ca065 = Gescannten Verlauf für diesen exakten Bytepfad öffnen

repo-workflows-atlas-atlas-window-a5661a75 = Häufigster Mitwirkender: { $node-contributor-1 }

repo-workflows-atlas-atlas-window-a9656539 = Mehr Atlasverlauf scannen

repo-workflows-atlas-atlas-window-aef428d1 = { $node-bytes-1 } Bytes · { $node-changes-2 } Dateiänderungen

repo-workflows-atlas-atlas-window-b4ab9b3d = ↑ Repository-Wurzel

repo-workflows-atlas-atlas-window-ba512e1a = Häufigster Mitwirkender

repo-workflows-atlas-atlas-window-bc087fce = Änderungshäufigkeit

repo-workflows-atlas-atlas-window-c2e87ba8 = frühester gescannter Stand

repo-workflows-atlas-atlas-window-d16ccfdf = Atlasanalyse abbrechen

repo-workflows-atlas-atlas-window-ef7caa7d = Fixierter Verlauf und Dateigrößen werden analysiert …

repo-workflows-atlas-atlas-window-f69e93c6 = Dateien und historische Ordner am selben Pfad sind getrennte Schwerpunkte. Der Dateiverlauf verwendet HEAD für aktuelle Dateien oder einen beobachteten Änderungsstand für historische Dateien. Rechtsklick zeigt den Verlauf über die analysierten Referenzen; Nicht-UTF-8-Pfade öffnen ihn direkt. Lange Namen werden gekürzt; nummerierte Identitäten bleiben getrennt.

repo-workflows-atlas-new-70ba4563 = Dateiverlauf abgebrochen

repo-workflows-atlas-read-atlas-02c42736 = Verwenden Sie Datumsangaben im Format YYYY-MM-DD

repo-workflows-atlas-read-atlas-67b18cbf = Verwenden Sie YYYY-MM-DD; der Beginn darf nicht nach dem Ende liegen

repo-workflows-atlas-show-01efcd54 = Dateiverlauf wird vorbereitet …

repo-workflows-atlas-show-076017ad = Diese Liste verfolgt den exakten Pfad über die analysierten Referenzen einschließlich Löschungen. Sie verwendet den gesamten gescannten Stand unabhängig vom Atlaszeitraum und folgt keinen Umbenennungen. Öffnen Sie einen Commit, um die Änderungen zu prüfen. Für ältere Änderungen wählen Sie „Mehr Atlasverlauf scannen“ im Atlas.

repo-workflows-atlas-show-1abc4096 = Nächste Datei-Commits

repo-workflows-atlas-show-56fe4d78 = Vorherige Datei-Commits

repo-workflows-atlas-show-7269bb4e = Gescannten Dateiverlauf über Referenzen anzeigen

repo-workflows-atlas-show-8647a64f = Exakte Pfadbytes: { $clipped-self-label-256-1 }

repo-workflows-atlas-show-c256cd01 = Kein gescannter Commit berührt diesen exakten Pfad. Sein Verlauf liegt möglicherweise vor der Scangrenze.

repo-workflows-bakeoff-assemble-bar-3766ae43 = Öffnen Sie unten eine Datei und übernehmen Sie diese ganz oder einzelne Hunks aus einem Versuch

repo-workflows-bakeoff-assemble-bar-5a22f453 = Wird zusammengestellt …

repo-workflows-bakeoff-assemble-bar-8462ef57 = Zusammenstellen in

repo-workflows-bakeoff-assemble-bar-86fd9bbb = Zusammenstellen ({ $picks } Dateien)

repo-workflows-bakeoff-assemble-bar-916e34d4 = Im Graphen anzeigen

repo-workflows-bakeoff-assemble-bar-aee10357 = Ein neuer Branch ab der Basis mit der Auswahl. Alles andere bleibt wie in der Basis. Kein ausgecheckter Stand wird geändert.

repo-workflows-bakeoff-assemble-bar-ccbdbfe8 = Zusammenstellen (eine Datei)

repo-workflows-bakeoff-assemble-bar-f19adc05 = Benennen Sie zuerst den neuen Branch

repo-workflows-bakeoff-assemble-c7f55e42 = { $branch } bei { $crate-repo-short-commit-1 } erstellt

repo-workflows-bakeoff-bakeoff-view-9754333a = Versuche werden verglichen …

repo-workflows-bakeoff-body-bfd30d1e = Klicken Sie auf eine Datei, um die Versuche zu vergleichen und daraus auszuwählen

repo-workflows-bakeoff-build-06d160c3 = Die Statusdatei für den Build konnte nicht vorbereitet werden: { $e }

repo-workflows-bakeoff-build-c18d3706 = Geben Sie zuerst den Build-Befehl für das Projekt ein

repo-workflows-bakeoff-build-c9238812 = Build · { $name }

repo-workflows-bakeoff-build-cell-17824e4b = Der alte Build läuft möglicherweise noch. Startet einen getrennten gespeicherten Stand und beendet oder bereinigt den alten Start nicht.

repo-workflows-bakeoff-build-cell-26c16489 = nicht gebaut

repo-workflows-bakeoff-build-cell-301c453c = ✘ fehlgeschlagen ({ $code })

repo-workflows-bakeoff-build-cell-3149d8b3 = Führt Ihren Befehl auf dem exakten Vergleichsstand in einem neuen losgelösten Worktree aus

repo-workflows-bakeoff-build-cell-5dac180d = Ergebnis nicht verfügbar

repo-workflows-bakeoff-build-cell-63f96d79 = Start fehlgeschlagen, Terminal oder Ausgabe beendet oder Beleg ungültig. Dies beweist nicht, dass alle Kindprozesse beendet sind.

repo-workflows-bakeoff-build-cell-6fe4abd4 = In neuem Arbeitsbereich wiederholen

repo-workflows-bakeoff-build-cell-84b604d3 = Das vorherige Ergebnis ist unbekannt; Prozesse laufen möglicherweise noch. Startet einen weiteren gespeicherten Arbeitsbereich ohne automatisches Beenden oder Bereinigen.

repo-workflows-bakeoff-build-cell-98cd8a0b = Stand wird vorbereitet …

repo-workflows-bakeoff-build-cell-adccacb7 = Führt den Befehl in einem weiteren neuen gespeicherten Stand aus

repo-workflows-bakeoff-build-cell-af81e9e4 = Erneut bauen

repo-workflows-bakeoff-build-cell-bdd254b6 = Bauen

repo-workflows-bakeoff-build-cell-d07e6c18 = ✔ Build erfolgreich

repo-workflows-bakeoff-build-cell-ef24cfb5 = Die Ausgabe steht im Terminal-Tab

repo-workflows-bakeoff-column-2ed1ade8 = Keine Hunks auswählbar (Binärdatei): Übernehmen Sie die ganze Datei

repo-workflows-bakeoff-column-6e86718f = Ersetzt die ausgewählten Hunks. Erneutes Klicken nimmt die Auswahl zurück.

repo-workflows-bakeoff-column-772841f1 = Löschung übernehmen

repo-workflows-bakeoff-column-a533487d = Ganze Datei übernehmen

repo-workflows-bakeoff-column-badecf50 = Neue Datei übernehmen

repo-workflows-bakeoff-files-51aec897 = { $a-name-1 } hat diese Datei unverändert gelassen

repo-workflows-bakeoff-files-7e17d722 = Keiner der Versuche hat etwas geändert

repo-workflows-bakeoff-files-83d88f06 = { $a-name-1 } hat diese Datei geändert

repo-workflows-bakeoff-files-fe616a60 = { $path }
    Im direkten Vergleich über alle Versuche öffnen

repo-workflows-bakeoff-grid-1af85190 = Größe

repo-workflows-bakeoff-grid-1f19288a = ja, wird mitverglichen

repo-workflows-bakeoff-grid-6263bbf3 = Die vollständige gespeicherte Ausgabe dieses Laufs im eingestellten Editor öffnen

repo-workflows-bakeoff-grid-6a3fe887 = Builds

repo-workflows-bakeoff-grid-7a44bde0 = Uncommittete Arbeit

repo-workflows-bakeoff-grid-7cbda0dd = Build-Ausgabe …

repo-workflows-bakeoff-grid-9c37d9c9 = Neue Abhängigkeiten

repo-workflows-bakeoff-grid-ce199c9e = Berührte Dateien

repo-workflows-bakeoff-header-0e2a45b6 = Andere Versuche …

repo-workflows-bakeoff-header-175f9ff6 = Alle bauen

repo-workflows-bakeoff-header-43c40423 = Bake-off

repo-workflows-bakeoff-header-46334791 = Bauen mit

repo-workflows-bakeoff-header-502f6dbd = Erneut vergleichen

repo-workflows-bakeoff-header-50cd264e = ausgehend von { $crate-repo-short-cmp-base-1 }

repo-workflows-bakeoff-header-7d9eb7ac = Schließen

repo-workflows-bakeoff-header-862a0723 = Führt diesen Befehl ausdrücklich auf jedem verglichenen Stand in einem neuen losgelösten Worktree aus. Kein Einrichtungsrezept wird ausgeführt. Die Arbeitsbereiche bleiben unter Worktrees zur Prüfung und Entfernung erhalten.

repo-workflows-bakeoff-header-9cec669f = Vergleichen Sie zuerst die Versuche

repo-workflows-bakeoff-header-a195ce68 = Geben Sie den Build-Befehl für das Projekt ein

repo-workflows-bakeoff-header-acb0a622 = ⚠ { $elsewhere-join-1 } begann anderswo

repo-workflows-bakeoff-header-b1ce0225 = Liest die Versuche samt uncommitteter Arbeit erneut ein

repo-workflows-bakeoff-header-bb683fa4 = Der gemeinsame Ausgangscommit (ihre Merge-Basis)

repo-workflows-bakeoff-header-c38e956d = Für dieses Repository gespeichert

repo-workflows-bakeoff-header-dd4159a5 = Dieser Versuch begann an einem anderen Commit. Seine Spalte zeigt auch die Änderungen vor seinem Beginn.

repo-workflows-bakeoff-hunk-label-0cf292e8 = Am Anfang

repo-workflows-bakeoff-hunk-label-4d9d8a81 = Zeile { $hunk-old-start-1 }

repo-workflows-bakeoff-hunk-label-7647677f = Nach Zeile { $hunk-old-start-1 }

repo-workflows-bakeoff-hunk-label-a3052fdf = Zeilen { $hunk-old-start-1 }–{ $hunk-old-start-n-1-2 }

repo-workflows-bakeoff-load-bakeoff-c6f043db = Auswahl verworfen: Die Versuche haben sich seit der Auswahl geändert

repo-workflows-bakeoff-pick-label-77dbc108 = ← ganze Datei aus { $name-n-1 }

repo-workflows-bakeoff-pick-label-c708de81 = ← { $list-len-1 } Hunks

repo-workflows-bakeoff-pick-label-d3d98b5d = ← ein Hunk

repo-workflows-bakeoff-side-by-side-47c58b04 = Unverändert in { $unchanged-join-1 }

repo-workflows-bakeoff-side-by-side-6bdfa343 = Auswahl aufheben

repo-workflows-bakeoff-side-by-side-76215eb4 = Der zusammengestellte Branch behält die Basisversion dieser Datei

repo-workflows-bakeoff-side-by-side-f280944f = Ausgewählte Hunks zweier Versuche ändern dieselben Zeilen. Wählen Sie einen davon ab.

repo-workflows-bakeoff-ui-09eb5aca = Wählen Sie { $MIN } bis { $MAX } Versuche derselben Aufgabe zum Vergleich: Worktrees (einschließlich uncommitteter Änderungen) oder Branches.

repo-workflows-bakeoff-ui-19766ed6 = Abbrechen

repo-workflows-bakeoff-ui-43c40423 = Bake-off

repo-workflows-bakeoff-ui-6217d161 = Worktree · { $crate-remote-display-w-1 }

repo-workflows-bakeoff-ui-638e249f = Filter

repo-workflows-bakeoff-ui-ae96d29a = Wählen Sie { $MIN } bis { $MAX } Versuche

repo-workflows-bakeoff-ui-c51e8eaa = Keine Worktrees oder anderen Branches zum Vergleichen

repo-workflows-bakeoff-ui-cd8d844d = Keine Treffer

repo-workflows-bakeoff-ui-e888155d = Vergleichen

repo-workflows-bakeoff-ui-fbbc3b24 = Höchstens { $MAX } Versuche

repo-workflows-bisect--512af075 = Warten Sie auf das Ende des Laufs oder stoppen Sie ihn

repo-workflows-bisect--e5e3ecc5 = Warten Sie, bis der laufende Vorgang abgeschlossen ist

repo-workflows-bisect-bisect-blocker-cff28fea = Warten Sie zuerst auf den Abschluss von { $busy }.

repo-workflows-bisect-bisect-buttons-0599842f = git bisect reset: checkt { $branch } erneut aus

repo-workflows-bisect-bisect-buttons-12c0910a = { $current } kann nicht getestet werden. Versuchen Sie einen anderen Commit.

repo-workflows-bisect-bisect-buttons-7cf0c74e = git bisect run: Ein Befehl testet jeden Commit für Sie

repo-workflows-bisect-bisect-buttons-7f573858 = { $current } enthält die Änderung

repo-workflows-bisect-bisect-buttons-a28c79f7 = git bisect reset: checkt den ursprünglichen Ausgangsstand erneut aus

repo-workflows-bisect-bisect-buttons-a6c7a84b = Abschließen

repo-workflows-bisect-bisect-buttons-a9a77839 = { $current } liegt vor der Änderung

repo-workflows-bisect-bisect-buttons-c3efc1be = Markieren Sie zuerst einen guten und einen schlechten Commit

repo-workflows-bisect-bisect-buttons-cae7d57b = Stoppen

repo-workflows-bisect-bisect-buttons-eded3d42 = Befehl ausführen …

repo-workflows-bisect-bisect-menu-items-43fa49b5 = Bisect hier starten …

repo-workflows-bisect-bisect-menu-items-5cc56687 = Findet den Commit, der eine Änderung eingeführt hat, durch Tests zwischen einem guten und einem schlechten Commit

repo-workflows-bisect-bisect-menu-items-5f1847b9 = { $at } als { $bisect-word-mark-bad-1 } markieren

repo-workflows-bisect-bisect-menu-items-72281c73 = { $at } im Bisect überspringen

repo-workflows-bisect-bisect-palette-items-011b904c = Bisect: { $at } überspringen

repo-workflows-bisect-bisect-palette-items-23b8f037 = Bisect: stoppen

repo-workflows-bisect-bisect-palette-items-34a86ddf = Bisect: abschließen

repo-workflows-bisect-bisect-palette-items-417fbb2c = Bisect: { $at } als { $bisect-word-mark-bad-1 } markieren

repo-workflows-bisect-bisect-palette-items-96774cb9 = Bisect: Befehl ausführen …

repo-workflows-bisect-bisect-palette-items-ba89bd47 = Bisect starten …

repo-workflows-bisect-bisect-status-675fd45e = Bisect läuft

repo-workflows-bisect-bisect-step-done-8336fbfe = { $label } fehlgeschlagen: { $e }

repo-workflows-bisect-commit-link-63633913 = Diesen Commit anzeigen

repo-workflows-bisect-hash-link-63633913 = Diesen Commit anzeigen

repo-workflows-bisect-mark-bisect-15306802 = Diesen Commit überspringen

repo-workflows-bisect-mark-bisect-22f6b91b = { $short-rev-1 } als { $bisect-word-mark-2 } markieren

repo-workflows-bisect-mark-bisect-9b0f3138 = Als { $bisect-word-mark-1 } markieren

repo-workflows-bisect-mark-bisect-a1c6c771 = Es läuft kein Bisect

repo-workflows-bisect-reset-bisect-0ddac35c = Bisect stoppen

repo-workflows-bisect-reset-bisect-1bdcc155 = Bisect abschließen

repo-workflows-bisect-reset-bisect-b63b6486 = wieder am Ausgangscommit

repo-workflows-bisect-reset-bisect-fd58b76b = wieder auf { $branch }

repo-workflows-bisect-run-log-534a3a34 = mit Exitcode { $c } beendet

repo-workflows-bisect-run-log-5fd804ed = fehlgeschlagen: { $e }

repo-workflows-bisect-run-log-64876c47 = Ausgabe ausblenden

repo-workflows-bisect-run-log-7d9eb7ac = Schließen

repo-workflows-bisect-run-log-8d23f4ee = Stoppt den Befehl und seine Kindprozesse. Der Bisect bleibt am erreichten Stand.

repo-workflows-bisect-run-log-9dbbb249 = Ausgabe anzeigen

repo-workflows-bisect-run-log-b7ec68a2 = Lauf stoppen

repo-workflows-bisect-run-log-bb5e4bf5 = durch ein Signal beendet

repo-workflows-bisect-run-message-3a24171a = Bisect-Lauf fehlgeschlagen: { $e }

repo-workflows-bisect-run-message-96334386 = Bisect-Lauf gestoppt

repo-workflows-bisect-run-message-b1d63bd2 = Exitcode { $c }

repo-workflows-bisect-run-message-b4eba258 = Bisect-Lauf mit { $code } beendet: { $last-line }

repo-workflows-bisect-run-message-dc9c7971 = Bisect-Lauf: Nur übersprungene Commits übrig; einer davon ist der erste fehlerhafte Commit

repo-workflows-bisect-run-message-de2346a5 = Bisect-Lauf: Ersten fehlerhaften Commit gefunden

repo-workflows-bisect-run-message-e8e581bc = einem Signal

repo-workflows-bisect-start-bisect-5c82e3e0 = Bisect kann nicht gestartet werden: { $why }

repo-workflows-bisect-start-bisect-db936451 = Bisect starten

repo-workflows-bisect-start-bisect-run-d300d73e = Bisect-Lauf

repo-workflows-bisect-steps-text-14999716 = Noch etwa { $n } Schritte

repo-workflows-bisect-steps-text-263b5d8b = Noch etwa ein Schritt

repo-workflows-bisect-steps-text-2d1513e9 = Letzter Schritt

repo-workflows-bisect-steps-text-5ccbfd65 = { $n } Commits nach diesem

repo-workflows-bisect-steps-text-9bbc8458 = Ein Commit nach diesem

repo-workflows-bisect-steps-text-a4e9b171 = Keine Commits nach diesem

repo-workflows-bisect-stop-bisect-action-0ddac35c = Bisect stoppen

repo-workflows-bisect-stop-bisect-action-c380662a = Bisect stoppen und { $back } erneut auschecken? Die bisherigen Markierungen werden vergessen.

repo-workflows-bisect-stop-bisect-action-fff2c8f7 = der zu Beginn ausgecheckte Stand

repo-workflows-bisect-waiting-2d4ce59a = Markieren Sie einen Commit als { $word } ({ $meaning }): per Rechtsklick im Graphen oder mit { $capitalised-word-1 } für den ausgecheckten Commit.

repo-workflows-bisect-where-it-is-25230d8d = Nur übersprungene Commits übrig. Der erste fehlerhafte Commit ist einer dieser { $possible-len-1 }:

repo-workflows-bisect-where-it-is-4c0b8e59 = Git findet zwischen den Markierungen nichts zum Testen (guter Commit nach dem schlechten?). Markieren Sie andere Commits oder wählen Sie „Stoppen“.

repo-workflows-bisect-where-it-is-683768ab = und { $possible-len-max-named-1 } weitere

repo-workflows-bisect-where-it-is-7d7c30e4 = · Wird getestet

repo-workflows-bisect-where-it-is-8b3812c0 = Erster fehlerhafter Commit:

repo-workflows-bisect-where-it-is-94883ca6 = vor der Änderung

repo-workflows-bisect-where-it-is-d5ebaa0a = mit der Änderung

repo-workflows-ci_view-by-start-ba35f0c4 = Nicht gestartet

repo-workflows-ci_view-ci-act-07fe25a7 = { $what } abgebrochen

repo-workflows-ci_view-ci-act-da60943b = Fehlgeschlagene Jobs von { $what } erneut ausgeführt

repo-workflows-ci_view-ci-view-ui-90db5328 = Auf diesem Commit lief keine CI.

repo-workflows-ci_view-ci-view-ui-b431c0d7 = { $noun } von { $short-view-sha-1 }

repo-workflows-ci_view-ci-view-ui-c454bec0 = Workflow-Läufe

repo-workflows-ci_view-ci-view-ui-d1a8fffe = Pipelines

repo-workflows-ci_view-ci-view-ui-e50c77eb = Bei einigen Projekten konnten keine Builds abgefragt werden.

repo-workflows-ci_view-job-card-0098c77e = Job { $job-name-1 }

repo-workflows-ci_view-job-card-f4ed9e36 = { $job-name-1 }
    Doppelklick öffnet ihn im Browser

repo-workflows-ci_view-load-ci-view-4c1d2d4e = Dieses Repository liegt weder auf GitHub noch auf GitLab.

repo-workflows-ci_view-open-ci-680d5c5f = interaktiver Rebase

repo-workflows-ci_view-open-ci-991ae978 = Schließen oder beenden Sie zuerst den { $editor }

repo-workflows-ci_view-open-ci-d509b2ee = Konflikteditor

repo-workflows-ci_view-page-04ec3970 = PROTOKOLL DER FEHLGESCHLAGENEN JOBS

repo-workflows-ci_view-page-20681c9f = das Protokoll

repo-workflows-ci_view-page-30fc4b32 = SCHRITTE VON { $job-name-to-uppercase-1 }

repo-workflows-ci_view-page-66f52811 = Keine Jobs.

repo-workflows-ci_view-page-7261125c = Das Protokoll ist leer.

repo-workflows-ci_view-page-77a860cb = An Agent senden

repo-workflows-ci_view-page-da532fb6 = Fügt das Ende dieses Protokolls in das Agent-Terminal ein, um den Fehler zu beheben

repo-workflows-ci_view-page-header-8c6c9cf3 = Zurück (Esc)

repo-workflows-ci_view-page-header-a7eac6b5 = Im Browser öffnen

repo-workflows-ci_view-page-header-d62bc4a3 = Erneut laden

repo-workflows-ci_view-run-actions-19766ed6 = Abbrechen

repo-workflows-ci_view-run-actions-1c703119 = Fehlerprotokoll anzeigen

repo-workflows-ci_view-run-actions-46677f1c = Führt fehlgeschlagene oder abgebrochene Jobs erneut aus

repo-workflows-ci_view-run-actions-7b68b715 = Protokoll ausblenden

repo-workflows-ci_view-run-actions-8c40a11a = Fehlgeschlagene Jobs wiederholen

repo-workflows-ci_view-run-header-0b4ad7c1 = Commit im Graphen anzeigen

repo-workflows-ci_view-run-header-396f73af = Versuch { $run-attempt-1 }

repo-workflows-ci_view-run-header-420fc08d = läuft seit { $ci-duration-label-seconds-1 }

repo-workflows-ci_view-run-header-4e8bc57f = · gestartet { $git-relative-time-started-now-1 }

repo-workflows-ci_view-run-header-ac6a17db = Dauer: { $ci-duration-label-seconds-1 }

repo-workflows-ci_view-run-header-b368daf0 = übersprungen oder abgebrochen

repo-workflows-ci_view-timeline-709e9121 = GitLab listet die Schritte eines Jobs nicht auf. Das Protokoll steht auf seiner Seite.

repo-workflows-ci_view-timeline-7a7400f4 = nicht gestartet

repo-workflows-ci_view-timeline-99fa228f = Noch nicht gestartet.

repo-workflows-ci_view-timeline-b59cc5d9 = Dieser Job hat keine Schritte ausgeführt.

repo-workflows-ci_view-timeline-ee459f80 = Dieser Job hat noch keine Schritte zum Anzeigen.

repo-workflows-collision_view-body-282052b1 = Jedes Branchpaar wird im Speicher gemergt (git merge-tree), ebenso jeder Branch mit { $onto }. Kein Checkout und keine Branchänderung. Branches verknüpfter Worktrees sind vorausgewählt.

repo-workflows-collision_view-body-5c35d043 = Zielbranch für die Integration

repo-workflows-collision_view-body-7d9eb7ac = Schließen

repo-workflows-collision_view-body-94846973 = Merge im Speicher … { $done } von { $run-total-1 }

repo-workflows-collision_view-body-cae7d57b = Stoppen

repo-workflows-collision_view-body-cf642735 = Kollisionsmatrix

repo-workflows-collision_view-cannot-check-42af2e23 = Wählen Sie den Zielbranch für die Integration

repo-workflows-collision_view-cannot-check-76fb9e14 = Eine Prüfung läuft

repo-workflows-collision_view-cannot-check-8bcfe01d = Wählen Sie die zu prüfenden Branches aus

repo-workflows-collision_view-cannot-check-c1d9426d = Höchstens { $MOST } Branches gleichzeitig

repo-workflows-collision_view-cell-tip-81bf5184 = { "\u000A  " }und { $n-listed-1 } weitere

repo-workflows-collision_view-cell-tip-9b99b61c = { "\u000A" }Zum Vergleichen klicken

repo-workflows-collision_view-cell-tip-b3f28cca = { $a } und { $b } lassen sich konfliktfrei mergen

repo-workflows-collision_view-check-collisions-96d2bcac = Gestoppt; keine neuen Ergebnisse

repo-workflows-collision_view-matrix-f5346a60 = Nach { $m-target-1 } gemergt

repo-workflows-collision_view-pick-branches-3bc09abe = Zielbranch

repo-workflows-collision_view-pick-branches-6de3bf71 = Es gibt keinen anderen lokalen Branch.

repo-workflows-collision_view-pick-branches-fb40a427 = Auswählen …

repo-workflows-collision_view-results-45512235 = Einige Paare konnten nicht im Speicher gemergt werden: { $e }

repo-workflows-collision_view-results-87f3fc81 = → zeigt jeden Branch beim Mergen nach { $m-target-1 }. Die Zahl gibt die Konfliktdateien an: Darüberfahren zeigt sie, Klicken vergleicht die Branches.

repo-workflows-collision_view-results-9b24a27f = Vorgeschlagene Integrationsreihenfolge

repo-workflows-collision_view-step-note-5896a7b7 = konnte im Speicher nicht mit { $unknown-join-1 } gemergt werden

repo-workflows-collision_view-step-note-5ef5ed3f = Konflikte mit { $parts-join-1 }

repo-workflows-collision_view-step-note-cb1d05dc = lässt sich konfliktfrei mit allen vorherigen Branches mergen

repo-workflows-content_recovery-content-inspect-e3ceaa94 = Ungültige Inhaltsauswahl

repo-workflows-content_recovery-content-perform-9d57ebb2 = Inhaltswiederherstellung: { $e }

repo-workflows-content_recovery-content-perform-fbdf23f8 = Inhalt wiederherstellen

repo-workflows-content_recovery-origin-53bdf21a = Reflog / Verlauf

repo-workflows-content_recovery-origin-856aca6d = Aktuelle Stash-Liste

repo-workflows-content_recovery-origin-d8ee810d = Unerreichbarer Commit

repo-workflows-content_recovery-origin-ebcaaeb6 = Gelöschter Stash

repo-workflows-content_recovery-show-11f20956 = Text, an den Sie sich erinnern, etwa parse_header

repo-workflows-content_recovery-show-1762ea22 = Dieses Repository ist flach geklont. Ältere Inhalte fehlen möglicherweise lokal.

repo-workflows-content_recovery-show-1e53ab9e = Grenzen: 500 Commits, 10000 Objekte, 15000 Dateieinträge, 2 MiB pro Blob, 64 MiB gescannter Inhalt, 300 Treffer, 120 Sekunden. Binärdateien, Nicht-UTF-8-Inhalte und übergroße Inhalte werden übersprungen.

repo-workflows-content_recovery-show-1f3ee4b2 = Branchwiederherstellung ansehen …

repo-workflows-content_recovery-show-30ea379c = Lesen abbrechen

repo-workflows-content_recovery-show-334a8ddd = Die Anzeige ist auf 2000 Zeilen und 400 Zeichen pro Zeile gekürzt. Die Wiederherstellung verwendet die vollständigen gespeicherten Bytes.

repo-workflows-content_recovery-show-34ac4d52 = { $hit-provenance-len-1 } bekannte Orte / Versionen

repo-workflows-content_recovery-show-4c76d94b = Wörtlichen Text suchen

repo-workflows-content_recovery-show-50b4d1c2 = Neuer Branch:

repo-workflows-content_recovery-show-52545e10 = { $report-hits-len-1 } Treffer · { $report-scanned-2 } Blobs / { $report-bytes-3 } Bytes gelesen · { $report-binary-4 } binäre, { $report-large-5 } übergroße, { $report-missing-6 } fehlende übersprungen

repo-workflows-content_recovery-show-55bda87c = Verlorene Inhalte wiederfinden

repo-workflows-content_recovery-show-5ec718b0 = Lokale Objekte werden gelesen · { $self-began-map-or-0-at-at-elapse-1 } s

repo-workflows-content_recovery-show-67802d1b = Sucht wörtlichen Text im Reflog-Verlauf, unerreichbaren Commits, gelöschten Stashes und losgelösten vorgemerkten Blobs. Die Suche liest lokale Objekte, ohne Downloads oder Dateiänderungen.

repo-workflows-content_recovery-show-6f803fc9 = Zeitpunkt unbekannt

repo-workflows-content_recovery-show-7d9eb7ac = Schließen

repo-workflows-content_recovery-show-8cd42b70 = Erstellt eine neue reguläre Datei auf dem Rechner des Repositorys. Vorhandene, versionierte, ignorierte und verknüpfte Ziele werden abgelehnt. Vorgemerkte Inhalte bleiben unverändert.

repo-workflows-content_recovery-show-8d5a103e = Branch { $name } am gespeicherten Commit erstellen. Der vollständige Commit bleibt enthalten; HEAD und Arbeitsdateien bleiben unverändert.

repo-workflows-content_recovery-show-8ef50fb8 = Die Suche hat eine Grenze erreicht oder nicht unterstützte Pfade beziehungsweise Herkunftsangaben übersprungen. Die Ergebnisse sind unvollständig.

repo-workflows-content_recovery-show-93547d7d = Jetzt wiederherstellen

repo-workflows-content_recovery-show-9cba18de = { $prepared-preview-identity-root-p-1 }/{ $path } mit { $prepared-preview-bytes-len-2 } Rohbytes erstellen. Vorhandene Inhalte werden nicht ersetzt.

repo-workflows-content_recovery-show-a6dcec6a = Losgelöster Inhalt: Ursprünglicher Dateiname und Vormerkzeitpunkt sind unbekannt. Er lässt sich als neue Datei wiederherstellen.

repo-workflows-content_recovery-show-a963e860 = { $date } · Losgelöster vorgemerkter Inhalt

repo-workflows-content_recovery-show-abadbc35 = Bei bekanntem Commit-Datum stehen die neuesten Einträge zuerst. Losgelöste Blobs stehen zuletzt.

repo-workflows-content_recovery-show-beb0df0b = Zurück zu den Treffern

repo-workflows-content_recovery-show-c006906c = Gespeicherter Inhalt · { $preview-bytes-len-1 } Bytes · { $preview-blob-12-2 }

repo-workflows-content_recovery-show-c8d6eb2f = Rückkehr in die Stash-Liste ansehen …

repo-workflows-content_recovery-show-cc6d8221 = Neue Datei in diesem Repository:

repo-workflows-content_recovery-show-da1ba0df = Dateiwiederherstellung ansehen …

repo-workflows-content_recovery-show-eff2c65f = Diesen gelöschten Stash in die Stash-Liste zurückholen und den aktuellen Stash behalten. Weder anwenden noch vorgemerkte oder Arbeitsdateien ändern.

repo-workflows-content_recovery-show-fbdaf564 = (Wurzel nicht verfügbar)

repo-workflows-custom_commands-custom-act-5bd548f5 = { $self-name-1 }: { $commands } Repository-Befehle werden jetzt angeboten

repo-workflows-custom_commands-custom-file-read-4533a718 = { $self-name-1 } enthält eigene Befehle. Prüfen Sie diese in der Palette oder einem Menü, um sie zu verwenden.

repo-workflows-custom_commands-custom-file-read-cbe7f164 = { $self-name-1 }: { $cc-repo-file-2 } kann nicht verwendet werden. „Repository-Befehle prüfen …“ zeigt den Grund.

repo-workflows-custom_commands-custom-palette-items-0841b1ff = Repository-Befehl

repo-workflows-custom_commands-custom-palette-items-b8af1810 = Repository-Befehle prüfen …

repo-workflows-custom_commands-custom-palette-items-cd972ec3 = eigener Befehl

repo-workflows-custom_commands-menu-items-7bbc1546 = { $cc-repo-file-1 } enthält noch nicht freigegebene oder unlesbare Befehle. Sehen Sie diese zuerst an.

repo-workflows-custom_commands-menu-items-8f4e08cd = Eigener Befehl (Einstellungen)

repo-workflows-custom_commands-menu-items-b8af1810 = Repository-Befehle prüfen …

repo-workflows-custom_commands-menu-items-cb87a7d9 = Eigener Befehl dieses Repositorys

repo-workflows-custom_commands-read-file-55a45ca4 = { $cc-repo-file-1 } ist kein UTF-8-Text

repo-workflows-custom_commands-read-file-851cebee = { $cc-repo-file-1 } kann nicht gelesen werden: { $e }

repo-workflows-custom_commands-read-here-28b7ef24 = Es ist größer als { $max-1024-1 } KiB

repo-workflows-custom_commands-read-here-50cdcf6c = Es ist keine Datei

repo-workflows-custom_commands-read-here-f4fd09c1 = Es ist ein symbolischer Link

repo-workflows-custom_commands-run-custom-44771367 = ${ $name } darf kein NUL-Zeichen enthalten

repo-workflows-custom_commands-run-custom-captured-471b45de = { $name } konnte nicht ausgeführt werden: { $e }

repo-workflows-custom_commands-run-custom-captured-5215b5ab = { $name } wird ausgeführt …

repo-workflows-custom_commands-run-custom-in-terminal-f33b19b3 = { $name } konnte nicht gestartet werden: { $e }

repo-workflows-dco--5e1a6b4e = Fügt Signed-off-by mit Ihrem Namen und Ihrer E-Mail-Adresse hinzu (DCO)

repo-workflows-dco-commit-note-72e49d72 = Eine DCO-Prüfung lehnt Commits ohne Sign-off des Autors oder Committers ab

repo-workflows-dco-commit-note-c87e4ef3 = Kein Signed-off-by des Autors (DCO)

repo-workflows-dco-commit-note-fccbd96e = ✔ Vom Autor signiert (DCO)

repo-workflows-dco-commits-count-4eb116dc = { $n } Commits

repo-workflows-dco-commits-count-acfd7b9e = Ein Commit

repo-workflows-dco-dco-act-10fe4a7c = Sign-off für dieses Repository konnte nicht gespeichert werden (git config { $signoff-sign-off-key-1 }): { $e }

repo-workflows-dco-dco-op-done-55d2b32b = Signierte Commits von { $plan-branch-1 } pushen

repo-workflows-dco-dco-op-done-5d0c9ff1 = Commits von { $plan-branch-1 } signieren (nur lokal)

repo-workflows-dco-fix-button-04f5829a = Fügt jedem Branch-Commit Ihr Signed-off-by hinzu (rebase --signoff) und pusht mit Lease, nachdem die Auswirkungen angezeigt wurden. Ctrl+Z macht dies rückgängig.

repo-workflows-dco-fix-button-344ac70e = Commits des Branches signieren …

repo-workflows-dco-fix-button-6d181602 = Checken Sie zuerst { $details-head-branch-1 } aus oder öffnen Sie den Branch in einem Worktree. Signiert werden die Commits des ausgecheckten Branches am neuesten Stand des Pull Requests.

repo-workflows-dco-fix-dialog-1a8aeebd = Signieren und pushen

repo-workflows-dco-fix-dialog-6ce6ab1f = Signieren

repo-workflows-dco-fix-label-dfc76b8d = Commits von { $plan-branch-1 } signieren

repo-workflows-dco-fix-preview-1cc2b37b = abgelehnt, falls seit dem letzten Abruf jemand dorthin gepusht hat

repo-workflows-dco-fix-preview-b12258fd = abgelehnt, falls der Branch dort bereits existiert

repo-workflows-dco-plan-0ac1ebd9 = Branch und Basis haben keinen gemeinsamen Commit: { $e }

repo-workflows-dco-plan-4c71d659 = { $branch } hat seit seiner Basis keine eigenen Commits

repo-workflows-dco-plan-589b41e2 = { $branch } steht auf { $crate-repo-short-head-1 }, der Pull Request auf { $crate-repo-short-expected-2 }. Pullen oder pushen Sie zuerst, damit die signierten Commits zum Pull Request gehören.

repo-workflows-dco-plan-927db158 = { $label } enthält Commits, die { $branch } fehlen. Pullen Sie diese zuerst, damit der Push sie nicht verwirft.

repo-workflows-dco-plan-a16c4b9b = Alle Commits von { $branch } sind bereits signiert

repo-workflows-dco-plan-cfb9aeb9 = Committen oder stashen Sie zuerst Ihre Änderungen. Sign-off führt einen Rebase des Branches aus.

repo-workflows-dco-plan-d7cbaa06 = { $branch } hat seit seiner Basis Merge-Commits, die ein Rebase einebnen würde. Signieren Sie ihn in einem Terminal (git rebase --signoff --rebase-merges { $crate-repo-short-merge-base-1 }) oder bitten Sie die Autoren darum.

repo-workflows-dco-plan-d866c6e0 = Checken Sie zuerst den Branch des Pull Requests aus. HEAD ist losgelöst.

repo-workflows-dco-pr-section-15029601 = Keine Commits für die DCO-Prüfung vorhanden.

repo-workflows-dco-pr-section-9ecdacc9 = Commits des Pull Requests werden geladen …

repo-workflows-dco-pr-section-e93d65c8 = Sign-offs der Commits konnten nicht gelesen werden: { $e }

repo-workflows-dco-text-dc4915c9 = Die DCO-Prüfung von #{ $n } schlägt fehl

repo-workflows-dco-text-e5458838 = Dieses Repository enthält die Datei { $file }

repo-workflows-dco-toggle-6ce6ab1f = Sign-off

repo-workflows-dco-toggle-9ba5d60e = Erneut angewendete Commits behalten ihre eigenen Nachrichten

repo-workflows-detached--bff74cc9 = Sie sehen eine alte Version. Neue Commits hier gehören zu keinem Branch.

repo-workflows-detached-detached-act-70dad4a3 = Warten Sie auf den Abschluss von { $busy } und checken Sie dann erneut aus

repo-workflows-detached-detached-banner-15bbfb59 = Branch hier erstellen

repo-workflows-detached-detached-banner-3d7f6bc2 = HEAD ist bei { $short-head-1 } losgelöst und gehört zu keinem Branch. Auch neue Commits gehören zu keinem Branch und bleiben beim Wechsel auf einen anderen Stand zurück.

repo-workflows-detached-detached-banner-f16cf1bf = Ein ausgecheckter Branch an diesem Commit. Neue Commits landen darauf.

repo-workflows-detached-detached-banner-f5cd526a = { $branch } auschecken

repo-workflows-detached-detached-banner-f76b5669 = Zurück zu { $branch }

repo-workflows-detached-keep-steps-d2a49cac = Branch { $name } erstellen

repo-workflows-detached-leave-to-007a4c0e = { $leave-label-1 } wurde nicht ausgecheckt: { $busy } hat inzwischen begonnen

repo-workflows-detached-leave-to-28ae5898 = { $leave-label-1 } auschecken: Warten Sie auf den Abschluss von { $busy }

repo-workflows-detached-leave-to-48b0ce4c = { $leave-label-1 } wurde nicht ausgecheckt: Die Commits, die ohne Branch zurückblieben, konnten nicht aufgelistet werden ({ $e })

repo-workflows-detached-leave-to-c6023785 = Die Auflistung wurde unerwartet beendet

repo-workflows-devcontainer-container-id-a1ced98b = Docker muss eine vollständige Container-ID mit 64 Zeichen zurückgeben

repo-workflows-devcontainer-detect-01821b94 = Die Dev-Container-Konfiguration muss im Worktree lesbar und höchstens 1 MiB groß sein

repo-workflows-devcontainer-detect-764255b7 = Die Dev-Container-Konfiguration ist kein UTF-8-Text

repo-workflows-devcontainer-devcontainer-act-519a489a = Dev-Container-Vorgang

repo-workflows-devcontainer-devcontainer-act-5474eef8 = Wird ausgeführt …

repo-workflows-devcontainer-devcontainer-act-84f8f92f = Vor dem Start abgebrochen

repo-workflows-devcontainer-devcontainer-window-05be654b = Umgebung starten

repo-workflows-devcontainer-devcontainer-window-0c79d376 = Worker beendet oder abgebrochen. Ein Container kann zurückbleiben. Prüfen Sie vor einem neuen Versuch die Labels dieses Worktrees.

repo-workflows-devcontainer-devcontainer-window-20441b4a = Angezeigte Umgebung neu bauen

repo-workflows-devcontainer-devcontainer-window-22f9e7ca = Keine .devcontainer/devcontainer.json gefunden. Entfernte Worktrees benötigen die CLI auf ihrem Rechner. Diese Integration unterstützt lokale Worktrees.

repo-workflows-devcontainer-devcontainer-window-2d5a4455 = Terminal im Container

repo-workflows-devcontainer-devcontainer-window-3675738c = Worktree: { $config-path-display-1 }
    Eigentumszuordnung: { $config-label-2 }
    Konfiguration SHA256: { $model-hash-config-source-as-byte-3 }

repo-workflows-devcontainer-devcontainer-window-3a6ea0e2 = Der Start führt Docker-, Build-, Feature- und Lebenszyklusbefehle dieses Repositorys aus. Prüfen Sie zuerst das Repository und die referenzierten Dateien. Ein Neubau löscht nur den angezeigten zugeordneten Container. Ungespeicherte Dateien, die nur darin existieren, gehen verloren.

repo-workflows-devcontainer-devcontainer-window-3c898437 = Verwendet das installierte Docker und die Dev Container CLI auf diesem Rechner. Unterstützt Linux-Container mit exakter Worktree-Einbindung. Verknüpfte Git-Worktrees benötigen standardmäßige relative POSIX-Metadaten und die native Einbindung des gemeinsamen Git-Verzeichnisses. Compose oder kombinierte eigene workspaceFolder/workspaceMount-Layouts werden abgelehnt. Metadaten werden nie umgeschrieben. Container bleiben nach dem Schließen dieses Fensters erhalten.

repo-workflows-devcontainer-devcontainer-window-4227dbe9 = Container in VS Code öffnen

repo-workflows-devcontainer-devcontainer-window-437e57b5 = Worktree-Konfiguration erneut lesen

repo-workflows-devcontainer-devcontainer-window-4564ef3d = Terminalübergabe abgelaufen oder abgebrochen. Öffnen Sie sie erneut.

repo-workflows-devcontainer-devcontainer-window-473f0412 = Containerkonfiguration SHA256: { $model-hash-binding-config-source-1 }

repo-workflows-devcontainer-devcontainer-window-5d2a90a9 = Container: { $binding-id-1 }
    Docker-Kontext: { $binding-engine-context-2 }
    Daemon: { $binding-engine-id-3 }
    Arbeitsbereich: { $binding-folder-4 }
    Benutzer: { $binding-user-5 }

repo-workflows-devcontainer-devcontainer-window-6e69284d = Konfiguration geändert: Bauen Sie den angezeigten Container vor der Nutzung von Terminal oder Editor neu. Container älterer App-Sitzungen oder Konfigurationen können in Docker verbleiben.

repo-workflows-devcontainer-devcontainer-window-7cb7e0b2 = Profil geändert oder Vorgang abgebrochen. Ein erstellter Container kann zurückbleiben. Prüfen Sie vor einem neuen Versuch die angezeigten Worktree-Labels.

repo-workflows-devcontainer-devcontainer-window-8f14be4f = Diesen Container löschen und neu bauen (nur darin vorhandene Dateien gehen verloren)

repo-workflows-devcontainer-devcontainer-window-a79d7c99 = Konfiguration erkannt. Prüfen Sie diese vor dem Start.

repo-workflows-devcontainer-devcontainer-window-b64a0d4f = Vorgang abbrechen

repo-workflows-devcontainer-devcontainer-window-b64ae277 = Worktree-Umgebung · Dev Containers

repo-workflows-devcontainer-devcontainer-window-bbd55e3e = /bin/sh im angezeigten Container geöffnet

repo-workflows-devcontainer-devcontainer-window-c1f95bd8 = Ich vertraue der Containerkonfiguration und den Lebenszyklusbefehlen dieses Worktrees

repo-workflows-devcontainer-devcontainer-window-d66d64d1 = Umgebung gestartet; Worktree-Einbindung und Eigentumszuordnung geprüft

repo-workflows-devcontainer-devcontainer-window-d8db68fe = { $error }
    Start oder Neubau wurden möglicherweise teilweise ausgeführt. Prüfen Sie Docker, die installierte Dev Container CLI, Build-Dateien und die oben angezeigten Fehler der Lebenszyklusbefehle. Es erfolgt keine globale Bereinigung. Prüfen Sie vor einem neuen Versuch die Labels dieses Worktrees.

repo-workflows-devcontainer-devcontainer-window-df4a39c8 = devcontainer.json (JSON-Kommentare werden von der nativen CLI unterstützt)

repo-workflows-devcontainer-devcontainer-window-dfcb3d90 = Das Terminal verwendet Docker exec mit diesem nativen Remote-Benutzer und /bin/sh. Der Editor benötigt VS Code mit der Dev Containers-Erweiterung.

repo-workflows-devcontainer-editor-47c00405 = Container gestoppt. Starten Sie die Umgebung vor der Editorübergabe.

repo-workflows-devcontainer-editor-806df56f = Zugeordneten Container-Arbeitsbereich an VS Code übergeben. Die Dev Containers-Erweiterung muss installiert sein.

repo-workflows-devcontainer-engine-0af39fb5 = Ungültige Docker-Kontext- oder Daemon-Identität

repo-workflows-devcontainer-engine-161bfc12 = Docker-Endpunkt fehlt

repo-workflows-devcontainer-engine-1cfeec69 = Wählen Sie einen lokalen Docker-Kontext und entfernen Sie DOCKER_HOST, bevor Sie diese Integration verwenden

repo-workflows-devcontainer-engine-a5b6c95b = Diese Integration unterstützt lokale Docker-Sockets. SSH-/TCP-Docker-Hosts benötigen eine eigene native Einrichtung. Es erfolgt keine automatische Übergabe.

repo-workflows-devcontainer-engine-f6690c9e = Docker-Daemon-Identität fehlt

repo-workflows-devcontainer-gitdir-pointer-a66f1615 = Nicht unterstützter .git-Verweis

repo-workflows-devcontainer-guard-7103bfb5 = Dev-Container-Konfiguration verschwunden

repo-workflows-devcontainer-guard-d353a99e = Abgebrochen

repo-workflows-devcontainer-inspect-2227e9c4 = Ungültiges Eigentumslabel

repo-workflows-devcontainer-inspect-2e29e769 = Container gehört zu einem anderen Worktree, Docker-Daemon oder ausgewählten Profil

repo-workflows-devcontainer-inspect-3c611856 = Docker inspect muss ein Array zurückgeben

repo-workflows-devcontainer-inspect-85ded6e6 = Container-ID oder Worktree-/Konfigurationslabels stimmen nicht mehr überein. Dieses Ziel wird abgelehnt.

repo-workflows-devcontainer-inspect-85fd3a35 = Dieser Container bindet den exakten Worktree nicht in seinen angezeigten Arbeitsbereich ein. Reine Volume- oder übergeordnete Verzeichniseinbindungen werden derzeit nicht unterstützt.

repo-workflows-devcontainer-inspect-d934c6ee = Genau ein zugeordneter Container erwartet

repo-workflows-devcontainer-json-0d639c4b = CLI-Ergebnis des Containers überschreitet 8 MiB

repo-workflows-devcontainer-json-edd11b13 = Ungültiges JSON-Ergebnis von { $tool }: { $e }

repo-workflows-devcontainer-linked-222d5e0e = Die native Einbindung des gemeinsamen Git-Verzeichnisses unterstützt nur das standardmäßige relative Layout common/worktrees/name

repo-workflows-devcontainer-linked-7c9c7625 = Gemeinsames Git-Verzeichnis fehlt

repo-workflows-devcontainer-linked-9990fafd = Der Rückverweis der verknüpften Git-Metadaten zeigt nicht auf diesen physischen Worktree

repo-workflows-devcontainer-linked-cdb7b1a0 = Übergeordnetes Git-Metadatenverzeichnis fehlt

repo-workflows-devcontainer-metadata-0a13f60e = Metadatenname des Worktrees fehlt

repo-workflows-devcontainer-metadata-735e6123 = Dieses verknüpfte oder symbolische Git-Layout wird von der nativen Container-Metadateneinbindung nicht unterstützt

repo-workflows-devcontainer-metadata-b2bab91f = Symbolisches oder nicht standardmäßiges gemeinsames Git-Metadatenlayout wird nicht unterstützt

repo-workflows-devcontainer-metadata-b9388ff2 = Container benötigt eine physische Worktree-Wurzel: { $e }

repo-workflows-devcontainer-metadata-file-a1a99ba5 = Git-Metadatenverweis ist kein UTF-8-Text

repo-workflows-devcontainer-metadata-file-bd4adcf1 = Verknüpfte Metadatenverweise müssen reguläre Dateien sein

repo-workflows-devcontainer-metadata-file-ddb7553c = Git-Metadatenverweis überschreitet 8 KiB

repo-workflows-devcontainer-metadata-ready-4d28a9de = Physische Worktree-Einbindung fehlt

repo-workflows-devcontainer-metadata-ready-7575b58d = Worktree-Ziel fehlt

repo-workflows-devcontainer-metadata-ready-8dad9e0c = Die Git-Metadaten des verknüpften Worktrees sind nicht am erforderlichen relativen Pfad les- und schreibbar eingebunden. Die Umgebung ist nicht bereit.

repo-workflows-devcontainer-metadata-ready-fea2d3e9 = Container-Einbindungen fehlen

repo-workflows-devcontainer-native-configuration-5aec3885 = Die native CLI hat die erforderliche gemeinsame Git-Verzeichniseinbindung nicht geplant. Die Umgebung kann nicht starten oder neu gebaut werden.

repo-workflows-devcontainer-native-configuration-90cc8e0e = Die Metadateneinbindung verknüpfter Worktrees wird für Compose oder kombinierte workspaceFolder/workspaceMount-Konfigurationen nicht unterstützt. Die native CLI lässt die erforderliche Einbindung des gemeinsamen Git-Verzeichnisses aus.

repo-workflows-devcontainer-native-configuration-969b53cd = Die native CLI hat keine geparste Konfiguration für den verknüpften Worktree zurückgegeben

repo-workflows-devcontainer-native-configuration-b328a4b3 = Ungültiger nativer workspaceFolder

repo-workflows-devcontainer-normalize-3fe3c368 = Git-Metadatenpfad verlässt die Dateisystemwurzel

repo-workflows-devcontainer-relative-fe173813 = Verknüpfte Container benötigen relative POSIX-Pfade für Git-Metadaten. Dieses Layout wird nicht unterstützt. Git-Metadaten wurden nicht geändert.

repo-workflows-devcontainer-same-97743985 = Worktree, Konfiguration oder ausgewähltes Profil geändert. Lesen Sie die Konfiguration erneut.

repo-workflows-devcontainer-start-6a0249a9 = Docker-Kontext oder Daemon seit der Anzeige geändert. Neubau wird abgelehnt.

repo-workflows-devcontainer-start-87b3fed2 = { $pause-why-1 }; beim Start einer Umgebung können Images und Features heruntergeladen werden

repo-workflows-devcontainer-started-046dacc8 = kein erfolgreiches Ergebnis

repo-workflows-devcontainer-started-19964d1c = containerId fehlt

repo-workflows-devcontainer-started-43acb4ec = remoteUser fehlt

repo-workflows-devcontainer-started-8c6f6a65 = Die CLI hat einen nicht unterstützten Container-Arbeitsbereich oder Benutzer zurückgegeben

repo-workflows-devcontainer-started-d1a64c4b = remoteWorkspaceFolder fehlt

repo-workflows-devcontainer-started-f9fa046f = Dev Container CLI hat die Umgebung nicht gestartet: { $value-description-as-str-unwrap--1 }

repo-workflows-devcontainer-terminal-77e417ed = Container { $binding-id-12-1 } · { $binding-folder-2 }

repo-workflows-devcontainer-terminal-b74a6469 = Container gestoppt. Starten Sie die Umgebung vor dem Öffnen eines Terminals erneut.

repo-workflows-devcontainer-terminal-f171832a = Vor dem Terminalstart abgebrochen

repo-workflows-doctor-ago-4603eba2 = vor { $secs-3600-1 } h

repo-workflows-doctor-ago-7ddb44d8 = gerade eben

repo-workflows-doctor-ago-a0c430f4 = vor { $secs-60-1 } min

repo-workflows-doctor-attributes-2176c800 = vergleicht die Datei

repo-workflows-doctor-attributes-4af8906f = filter={ $name } bezeichnet einen hier nicht definierten Filter. Git speichert die Datei unverändert und checkt sie ebenso aus.

repo-workflows-doctor-attributes-68cb5f93 = Zeilenenden · im Repository: { $eol-word-eol-index-1 } · im Arbeitsverzeichnis: { $eol-word-eol-worktree-2 } · Attribute: { $if-eol-attr-is-empty-else-eol-at-3 }

repo-workflows-doctor-attributes-82806016 = filter=lfs, aber Git LFS ist hier nicht eingerichtet (git lfs install). Checkouts hinterlassen Zeigerdateien; hinzugefügte Dateien landen direkt in Git statt in LFS.

repo-workflows-doctor-attributes-c6b25148 = { $attr }={ $name } bezeichnet einen hier nicht definierten Treiber ({ $attr }.{ $name }.*). Git { $if-attr-merge-merges-it-else-dif-1 } wie üblich.

repo-workflows-doctor-attributes-cb2060cc = Keine Attribute. Git schätzt Text oder Binärdaten für Diffs und Merges selbst ein. Nur core.autocrlf bestimmt die Zeilenenden.

repo-workflows-doctor-attributes-ec97a4d2 = filter={ $name } ist erforderlich, hat hier aber keinen clean- oder smudge-Befehl. Git verweigert das Hinzufügen oder Auschecken dieser Datei.

repo-workflows-doctor-attributes-f31e8d7f = mergt die Datei

repo-workflows-doctor-clear-7f76d2cf = Flag von { $one-path-1 } entfernen

repo-workflows-doctor-clear-83b12c22 = Entfernen

repo-workflows-doctor-clear-909d7fba = Flags entfernen?

repo-workflows-doctor-clear-ac70d25c = { $files-len-1 } Dateien

repo-workflows-doctor-clear-ae587a37 = Git prüft { $what } wieder. Lokale Änderungen erscheinen unter „Änderungen“, wo „Verwerfen“ sie löschen würde. Die zuvor gesetzten Flags werden nicht gespeichert. Dies lässt sich hier nicht rückgängig machen.

repo-workflows-doctor-clear-e318945b = Flags von { $files-len-1 } Dateien entfernen

repo-workflows-doctor-confirm-modal-19766ed6 = Abbrechen

repo-workflows-doctor-confirm-modal-3b13192b = Änderungen verwerfen?

repo-workflows-doctor-confirm-modal-c324130d = Ihre Änderungen an { $file } sind nicht gespeichert. Beim Schließen des Doktors gehen sie verloren.

repo-workflows-doctor-confirm-modal-e76fd2ad = Weiter bearbeiten

repo-workflows-doctor-confirm-modal-eb1a70e3 = Verwerfen

repo-workflows-doctor-details-ba8d6068 = Keine Datei setzt den Wert. Git verwendet seinen eingebauten Standard.

repo-workflows-doctor-doctor-act-9bab8d95 = Warten Sie auf das Ende des laufenden Vorgangs

repo-workflows-doctor-doctor-ui-3095e533 = Repository-Doktor

repo-workflows-doctor-doctor-ui-8c6c9cf3 = Zurück (Esc)

repo-workflows-doctor-doctor-ui-d8cb92fa = Erneut lesen

repo-workflows-doctor-editor-0ba2a573 = Keine Änderungen

repo-workflows-doctor-editor-1509f561 = Speichern

repo-workflows-doctor-editor-1ee81c8e = { $e-file-label-1 } gehört nicht zu diesem Repository. Die Datei gilt für jedes Repository, das sie liest. Klicken Sie erneut zum Speichern.

repo-workflows-doctor-editor-23e39291 = Wird gespeichert …

repo-workflows-doctor-editor-324b134f = Vorschau

repo-workflows-doctor-editor-52448408 = { $e-file-label-1 } wird bearbeitet

repo-workflows-doctor-editor-5902680c = Außerhalb des Repositorys speichern

repo-workflows-doctor-editor-6c1adba2 = Erneut lesen

repo-workflows-doctor-editor-7d9eb7ac = Schließen

repo-workflows-doctor-editor-a875327e = Ändern Sie zuerst die Regeln

repo-workflows-doctor-editor-b5bfc677 = { $e-file-label-1 } wird bearbeitet · nicht gespeichert

repo-workflows-doctor-editor-b5c82673 = Warten Sie auf den laufenden Vorgang

repo-workflows-doctor-editor-f9bfa3dc = Änderungen verwerfen

repo-workflows-doctor-editor-fca046cc = Gespeichert.

repo-workflows-doctor-eol-notes-384973dd = Kein text- oder eol-Attribut. core.autocrlf jedes Rechners bestimmt die Zeilenenden und verursacht dadurch wechselnde Dateiendeformate. `* text=auto` in .gitattributes vereinheitlicht dies.

repo-workflows-doctor-eol-notes-868d3e72 = Mit CRLF oder gemischten Zeilenenden gespeichert. Auf Rechnern mit core.autocrlf erscheint die gesamte Datei als geändert.

repo-workflows-doctor-eol-word-ec3e00be = keine Zeilenumbrüche

repo-workflows-doctor-explain-card-3b61e4e9 = Erklären

repo-workflows-doctor-explain-card-7486f5b7 = Datei erklären

repo-workflows-doctor-explained-55b528ba = Speichern oder verwerfen Sie zuerst die bearbeiteten Änderungen

repo-workflows-doctor-explained-98353ed7 = Diese Datei bearbeiten

repo-workflows-doctor-folder-card-05f75db2 = Durch core.hooksPath ({ $path }) festgelegt statt .git/hooks.

repo-workflows-doctor-folder-card-34845b15 = Der eigene Ordner des Repositorys (.git/hooks); core.hooksPath ist nicht gesetzt.

repo-workflows-doctor-folder-card-548df737 = Hooks-Ordner

repo-workflows-doctor-folder-card-5cc7fab9 = verwaltet hier die Hooks: { $m-advice-1 }

repo-workflows-doctor-folder-card-605924cc = { $n } .sample-Dateien: von Git gelieferte Beispiele, die nie ausgeführt werden.

repo-workflows-doctor-folder-card-63c1e34d = Eine .sample-Datei: ein von Git geliefertes Beispiel, das nie ausgeführt wird.

repo-workflows-doctor-folder-card-8fa06071 = core.hooksPath ist leer. Git führt keinen Hook aus. Entfernen Sie die Einstellung unter „Welche Konfiguration gewinnt“, um Hooks wieder zu aktivieren.

repo-workflows-doctor-history-card-06c896b6 = Verlauf im Graphen

repo-workflows-doctor-history-card-5a389005 = Jede Aktualisierung liest den Verlauf und erstellt den Graphen erneut. Mit einem Commit-Graph liest Git ihn um ein Vielfaches schneller.

repo-workflows-doctor-history-card-cd12e41d = Dieses Repository hat { $group-h-commits-unwrap-or-defaul-1 } Commits und damit mehr als der Graph zeigt. Die ältesten sind nur über das Terminal (git log) oder den Dateiverlauf erreichbar. Das Ausblenden oder Isolieren von Branches begrenzt den Graphen auf diese.

repo-workflows-doctor-hook-row-432f4eba = In LF umwandeln

repo-workflows-doctor-hook-row-433c4667 = läuft bei { $on }

repo-workflows-doctor-hook-row-96c0aa61 = Ausführbar machen

repo-workflows-doctor-hooks-act-63222fea = ausführbar gemacht

repo-workflows-doctor-hooks-act-fdf85121 = mit LF-Zeilenenden gespeichert

repo-workflows-doctor-hooks-card-0dfb736a = Andere Dateien (werden nie ausgeführt)

repo-workflows-doctor-hooks-card-52f0510d = Ein Hook

repo-workflows-doctor-hooks-card-69d27675 = Git führt keine Hooks mit diesen Namen aus. Möglicherweise Tippfehler oder beiseitegelegte Kopien.

repo-workflows-doctor-hooks-card-6c856ad1 = Kein Hook wird ausgeführt. Jeder Vorgang läuft ohne Hook weiter.

repo-workflows-doctor-hooks-card-84fbcccd = Keine aktiven Hooks

repo-workflows-doctor-hooks-card-d028a324 = { $n } Hooks

repo-workflows-doctor-ignored-card-0384b20e = Ignorierte Dateien

repo-workflows-doctor-ignored-card-1303e9b5 = Nichts wird ignoriert.

repo-workflows-doctor-ignored-card-37abda7f = Ignorierte Dateien ({ $total })

repo-workflows-doctor-ignored-card-683768ab = und { $total-list-len-1 } weitere

repo-workflows-doctor-ignored-card-cdd2ef27 = Erklären

repo-workflows-doctor-ignored-card-fb0ae18f = Unversionierte, von Git ignorierte Dateien. Ein Ordner steht für seinen gesamten Inhalt. Klicken Sie einen Eintrag an, um ihn zu erklären.

repo-workflows-doctor-manual-card-14faa67e = Flags der { $ticked-len-1 } ausgewählten Dateien entfernen

repo-workflows-doctor-manual-card-261a3c3d = Keine markierten Dateien

repo-workflows-doctor-manual-card-29a390f9 = Alle Flags entfernen

repo-workflows-doctor-manual-card-6c8cdf68 = Flags der Auswahl entfernen

repo-workflows-doctor-manual-card-70727cd6 = { $n } markierte Dateien

repo-workflows-doctor-manual-card-98cd282b = Wählen Sie zuerst die Dateien aus

repo-workflows-doctor-manual-card-b5c82673 = Warten Sie auf den laufenden Vorgang

repo-workflows-doctor-manual-card-bf185132 = Der Name ist kein gültiges UTF-8. Entfernen Sie das Flag mit git update-index in einem Terminal.

repo-workflows-doctor-manual-card-eb376b6a = Keine versionierte Datei trägt assume-unchanged oder skip-worktree.

repo-workflows-doctor-manual-card-f01d6b0e = Eine markierte Datei

repo-workflows-doctor-manual-card-febfc014 = Git prüft wieder jede dieser Dateien

repo-workflows-doctor-more-683768ab = und { $total-shown-1 } weitere

repo-workflows-doctor-open-doctor-745454bd = Schließen oder beenden Sie zuerst den Konflikteditor oder den interaktiven Rebase

repo-workflows-doctor-origin-label-4cf3860a = gitcraft-Profil

repo-workflows-doctor-other-worktrees-1aef9399 = nicht gesetzt

repo-workflows-doctor-other-worktrees-965a2b09 = Mit anderen Worktrees vergleichen

repo-workflows-doctor-output-card-01623799 = Ausgabe der letzten Vorgänge

repo-workflows-doctor-output-card-109b47bc = … { $lines-len-shown-len-1 } Zeilen darüber

repo-workflows-doctor-output-card-50f9b973 = Noch keine Ausgabe von durch gitcraft ausgeführten Hook-Vorgängen. Terminalausgaben werden nicht gespeichert.

repo-workflows-doctor-output-card-ae5bfc74 = Ausgaben von Git und seinen Hooks bei den letzten Vorgängen in diesem Repository (Commit, Merge, Push, Checkout …), bis zu zwölf Einträge. Nur im Speicher, mit verborgenen Passwörtern in URLs.

repo-workflows-doctor-preview-0f241c6b = Nicht mehr ignoriert

repo-workflows-doctor-preview-63666b35 = Neu ignoriert

repo-workflows-doctor-preview-683768ab = und { $paths-len-preview-shown-1 } weitere

repo-workflows-doctor-preview-8c8b9b2a = Versionierte Dateien mit passenden Regeln (bleiben versioniert)

repo-workflows-doctor-preview-bbb0c0cc = Diese Änderung würde für keine unversionierte Datei die Ignorierung ändern.

repo-workflows-doctor-preview-cd2605b4 = Nur die Regeln dieser Datei. Regeln anderer Dateien sind ausgelassen. Daher werden !-Regeln, die Ignorierungen anderer Dateien aufheben, nicht angezeigt. Andere Dateien können hier aufgeführte Pfade weiterhin ignorieren.

repo-workflows-doctor-remove-confirm-9861b6eb = { $origin-key-1 } entfernen?

repo-workflows-doctor-remove-confirm-b0b26eef = { $origin-key-1 } ({ $shortened-origin-shown-value-2 }) aus { $file } entfernen

repo-workflows-doctor-remove-confirm-c3812fc4 = Entfernen

repo-workflows-doctor-rules-card-379ee96f = Bearbeiten Sie eine Datei, prüfen Sie die Vorschau der Auswirkungen und speichern Sie sie dann.

repo-workflows-doctor-rules-card-464c4ffd = Bearbeiten

repo-workflows-doctor-rules-card-55b528ba = Speichern oder verwerfen Sie zuerst die bearbeiteten Änderungen

repo-workflows-doctor-rules-card-91b1b197 = Dateien mit Ignorierregeln

repo-workflows-doctor-scope-choice-0eb26325 = Dieser Worktree

repo-workflows-doctor-scope-choice-38429e22 = Dieses Repository

repo-workflows-doctor-scope-choice-59b8031f = Alle meine Repositorys

repo-workflows-doctor-scope-choice-ad944dcb = Alle Benutzer dieses Rechners

repo-workflows-doctor-scope-choice-c925a118 = Befehlszeile

repo-workflows-doctor-set-row-2b7c9578 = Ihre globale Git-Konfiguration auf diesem Rechner (meist ~/.gitconfig), für alle Ihre Repositorys dort.

repo-workflows-doctor-set-row-30a28bf1 = Wählen Sie zuerst einen Wert aus

repo-workflows-doctor-set-row-3da50e6f = Gits Systemdatei für alle Benutzer dieses Rechners. Benötigt meist Administratorrechte.

repo-workflows-doctor-set-row-46f01635 = Geben Sie zuerst einen Wert ein

repo-workflows-doctor-set-row-6ec46474 = Benötigt extensions.worktreeConfig = true in diesem Repository. Ohne diese Einstellung verweigert Git den Vorgang bei mehreren Worktrees; bei nur einem schreibt es in .git/config.

repo-workflows-doctor-set-row-85b9cd3d = Geltungsbereich

repo-workflows-doctor-set-row-921f564b = { $setting-key-1 } auf { $shortened-value-2 } für { $scope-choice-scope-to-lowercase-3 } setzen

repo-workflows-doctor-set-row-a215cc6f = { $setting-key-1 } für { $scope-choice-scope-to-lowercase-2 } setzen?

repo-workflows-doctor-set-row-ad30af45 = Ihre globale Git-Konfiguration (meist ~/.gitconfig), für alle Ihre Repositorys. Ein gitcraft-Profil mit demselben Wert hat weiterhin Vorrang.

repo-workflows-doctor-set-row-b5c82673 = Warten Sie auf den laufenden Vorgang

repo-workflows-doctor-set-row-b6f6f3ad = Setzen

repo-workflows-doctor-set-row-cadb5751 = Nur dieser Worktree (seine Datei config.worktree).

repo-workflows-doctor-set-row-cc3edd84 = Gits Systemdatei für alle Benutzer dieses Rechners

repo-workflows-doctor-set-row-ccc71902 = { $setting-key-1 } = { $value } wird in { $if-scope-scope-global-your-globa-2 } geschrieben. Der dort ersetzte Wert wird nicht gespeichert.

repo-workflows-doctor-set-row-e0a790f9 = Ihre globale Git-Konfiguration für alle Ihre Repositorys

repo-workflows-doctor-set-row-e0d5ce0a = Nur dieses Repository (.git/config). Ein späteres include in dieser Datei kann den Wert weiterhin überschreiben.

repo-workflows-doctor-setting-row-2224c772 = nicht gesetzt: Git-Standard

repo-workflows-doctor-setting-row-8f8c7445 = überschreibt { $values-len-1-1 } Werte

repo-workflows-doctor-setting-row-8ffd7cc3 = Git-Standard

repo-workflows-doctor-size-8333f24e = { NUMBER($kib-as-f64-1024-0-1, minimumFractionDigits: 1, maximumFractionDigits: 1) } MiB

repo-workflows-doctor-size-95886483 = { $kib } KiB

repo-workflows-doctor-size-9d2b746f = { NUMBER($kib-as-f64-1-048-576-0-1, minimumFractionDigits: 2, maximumFractionDigits: 2) } GiB

repo-workflows-doctor-size-card-1af85190 = Größe

repo-workflows-doctor-size-card-1f2fe3f1 = { $o-packed-1 } in { $o-packs-2 } Packs, { $size-o-pack-kib-3 }

repo-workflows-doctor-size-card-35195164 = Gepackte Objekte

repo-workflows-doctor-size-card-52c9d559 = { $o-garbage-1 } Dateien im Objektordner sind keine Objekte ({ $size-o-garbage-kib-2 }). Sie stammen von einem unterbrochenen Vorgang; git gc meldet sie.

repo-workflows-doctor-size-card-8757152b = Lose Objekte

repo-workflows-doctor-size-card-b83a7e1d = Lose und zusätzlich gepackt

repo-workflows-doctor-size-card-d09648c6 = Commits

repo-workflows-doctor-sparse-card-04bc71e9 = Von Sparse-Checkout ausgelassen ({ $left-out-len-1 })

repo-workflows-doctor-sparse-card-7bb29a8c = Git markiert auch diese Dateien mit skip-worktree. Sie gehören jedoch zu Sparse-Checkout. Ändern Sie die Auswahl mit git sparse-checkout (set, add, disable), indem Sie die Flags beibehalten.

repo-workflows-doctor-sparse-card-9cff4083 = Sparse-Checkout ist im Kegelmodus aktiv. Nur die Dateien auf oberster Ebene sind ausgecheckt.

repo-workflows-doctor-sparse-card-ce4ee135 = Sparse-Checkout ist im Kegelmodus aktiv. Ausgecheckt: { $dirs-join-1 }.

repo-workflows-doctor-sparse-card-f364e530 = Sparse-Checkout ist mit Mustern aktiv. Alle skip-worktree-Dateien gelten als dadurch ausgelassen. Deshalb werden ihre Flags hier nicht entfernt.

repo-workflows-doctor-speed-card-0ad7db50 = git status: { $took-as-millis-1 } ms (gitcraft führt es bei jeder erkannten Änderung aus).

repo-workflows-doctor-speed-card-2154776b = Lesedauer

repo-workflows-doctor-speed-card-8636e9ce = { $d-as-millis-1 } ms

repo-workflows-doctor-speed-card-aebc38ca = Letzte Tab-Aktualisierungen (Branches, Verlauf und Änderungen gemeinsam): { $times-join-1 }.

repo-workflows-doctor-speed-card-ffe6b51a = Noch keine Tab-Aktualisierung gemessen.

repo-workflows-doctor-suggestion-2b7f0cfe = hier nicht verfügbar

repo-workflows-doctor-suggestion-5a1f096a = Einschalten

repo-workflows-doctor-suggestion-5a853d12 = Objekte packen

repo-workflows-doctor-suggestion-a96e0fc8 = Gits eigener Monitor existiert nur unter Windows und macOS (auf Netzlaufwerken nur mit fsmonitor.allowRemote). Unter Linux kann ein durch core.fsmonitor benannter Watchman-Hook einspringen.

repo-workflows-doctor-suggestion-b5c82673 = Warten Sie auf den laufenden Vorgang

repo-workflows-doctor-suggestion-bd63228b = Dateisystemmonitor

repo-workflows-doctor-suggestions-card-e930084c = Mögliche Beschleunigungen

repo-workflows-doctor-title-1e6bed5e = Verborgene Änderungen

repo-workflows-doctor-title-8b2d57fa = Welche Konfiguration gewinnt

repo-workflows-doctor-title-8d6acfc3 = Zustand und Wartung

repo-workflows-doctor-title-94ec061c = Ignorierregeln und Attribute

repo-workflows-doctor-title-d412a062 = Hooks

repo-workflows-doctor-turn-on-action-3c97132f = Wartung planen?

repo-workflows-doctor-turn-on-action-7cde0a5e = { $commands-join-1 } trägt dieses Repository in maintenance.repo Ihrer globalen Git-Konfiguration ein, setzt maintenance.auto=false und maintenance.strategy=incremental in seiner Konfiguration und plant Git-Hintergrundläufe. Diese rufen stündlich Remotes nach refs/prefetch ab. Die Läufe lesen Ihre globale Konfiguration, kein gitcraft-Profil. git maintenance unregister nimmt dies für dieses Repository zurück (git config --unset maintenance.auto aktiviert automatische gc erneut); git maintenance stop entfernt den Zeitplan für alle Repositorys.

repo-workflows-doctor-turn-on-action-d8e3e02d = Einschalten: { $s-title-to-lowercase-1 }

repo-workflows-doctor-turn-on-action-f4830a1d = Planen

repo-workflows-doctor-turn-on-action-f688c962 = Geplante Wartung

repo-workflows-doctor-ui-089db75c = Nicht gesetzte Einstellungen anzeigen

repo-workflows-doctor-ui-11781882 = Hooks sind Programme, die Git während eines Vorgangs ausführt: vor Commit oder Push und nach Checkout. Ein fehlgeschlagener Hook stoppt den Vorgang. Ein nicht ausführbarer Hook wird übersprungen oder führt zu einer schwer verständlichen Fehlermeldung.

repo-workflows-doctor-ui-195ee0f7 = Dateien, deren Änderungen nie unter „Änderungen“ erscheinen, tragen häufig Index-Flags. Beide Flags verbergen Änderungen, haben aber unterschiedliche Bedeutungen. Git unterstützt keines davon zum Ignorieren von Änderungen an versionierten Dateien.

repo-workflows-doctor-ui-1f9897b7 = Git liest zuerst die Systemdatei, dann Ihre globale Datei, die Dateien dieses Repositorys und Worktrees und zuletzt Befehlszeilenwerte. Bei einwertigen Einstellungen gewinnt der zuletzt gelesene Wert. Bei einem gitcraft-Profil ist die globale Datei die Profildatei: Sie liest zuerst ~/.gitconfig und überschreibt sie dann mit eigenen Werten (Git außerhalb von gitcraft sieht diese nicht). Werte aus include- oder includeIf-Dateien erscheinen unter ihrer eigenen Datei. Angezeigt werden nur diese Einstellungen, keine Zugangsdaten.

repo-workflows-doctor-ui-282e3905 = assume-unchanged: Verspricht Git, dass sich die Datei nicht ändert. Git prüft sie für mehr Geschwindigkeit auf langsamen Datenträgern nicht. Git kann die Datei trotzdem überschreiben (Pull, Checkout) und das Flag entfernen. Es dient nicht dem Erhalt lokaler Änderungen.

repo-workflows-doctor-ui-2bfe98f3 = Keine passende Einstellung.

repo-workflows-doctor-ui-4e47a723 = Einstellungen filtern

repo-workflows-doctor-ui-56146558 = Die beiden Flags

repo-workflows-doctor-ui-678f7b82 = Warum eine Datei verschwindet, Zeilenenden wechseln oder ein Merge-Treiber nicht läuft: Die Ignorierregel und Attribute einer Datei im Arbeitsverzeichnis. Das Dateimenü „Ignorierregeln und Attribute“ führt hierher.

repo-workflows-doctor-ui-99079609 = skip-worktree: Git behält die Arbeitsversion und vergleicht sie nicht mit dem Index. Sparse-Checkout verwendet dies. Auch lokale Änderungen werden damit oft verborgen, obwohl Git dies nicht unterstützt: Pull, Checkout oder Merge können bis zum Entfernen des Flags scheitern. Für lokale Einstellungen ist eine unversionierte Überschreibungsdatei sicherer.

repo-workflows-doctor-ui-9b3b999d = Eine Korrektur läuft

repo-workflows-doctor-ui-b5c82673 = Warten Sie auf den laufenden Vorgang

repo-workflows-doctor-ui-c7deeea1 = Größe des Repositorys, Lesedauer und mögliche Git-Beschleunigungen. Alle Beschleunigungen sind Git-Einstellungen für dieses Repository und helfen Git überall, auch außerhalb von gitcraft.

repo-workflows-doctor-value-row-29f009c0 = Alle { $origin-key-1 }-Werte aus dieser Datei entfernen. Danach gilt der nächste Wert oder der Git-Standard.

repo-workflows-doctor-value-row-4b67153d = Wirksam

repo-workflows-doctor-value-row-4d8ff0fe = Über die Befehlszeile oder GIT_CONFIG_PARAMETERS gesetzt, nicht in einer Datei

repo-workflows-doctor-value-row-62a03f09 = Überschrieben

repo-workflows-doctor-value-row-b5c82673 = Warten Sie auf den laufenden Vorgang

repo-workflows-doctor-value-row-c3812fc4 = Entfernen

repo-workflows-doctor-value-row-fd2cc5af = gitcraft schreibt diese Datei über Einstellungen → Profil. Ändern Sie den Wert dort.

repo-workflows-doctor-verdict-050f8d52 = Versioniert: Ignorierregeln gelten nicht für bereits versionierte Dateien. Diese Regel hat daher keine Wirkung. Beenden Sie die Versionierung mit git rm --cached, um sie zu ignorieren. Die Datei bleibt auf dem Datenträger.

repo-workflows-doctor-verdict-2459673a = Keine passende Ignorierregel.

repo-workflows-doctor-verdict-765462d4 = Der Ordner wird ignoriert. Da Git nicht hineinsieht, kann eine !-Regel für eine enthaltene Datei nicht wirken. Heben Sie die Ordnerregel auf (!folder/) oder ignorieren Sie stattdessen folder/*.

repo-workflows-doctor-verdict-a1c6264a = Versioniert, ohne passende Ignorierregel.

repo-workflows-doctor-verdict-a292bdad = Nicht ignoriert: Diese !-Regel ist die letzte passende Regel.

repo-workflows-doctor-verdict-b78474c9 = Ein Ordner: Die angezeigte Regel gilt für seinen gesamten Inhalt. Git sieht nicht in ignorierte Ordner hinein.

repo-workflows-doctor-verdict-b796f495 = Durch diese Regel ignoriert.

repo-workflows-doctor-verdict-c3b9d549 = Diese Datei existiert hier nicht. Die Regeln werden für einen solchen Dateinamen gelesen. Ordnerregeln mit abschließendem / passen nicht darauf.

repo-workflows-doctor-verdict-f0d742e1 = Versioniert; die letzte passende Regel ist diese !-Regel.

repo-workflows-drift_view-body-31794226 = Erneut messen

repo-workflows-drift_view-body-3e453620 = Coding-Agent befragen (optional)

repo-workflows-drift_view-body-42a9eb31 = Aufgabe speichern

repo-workflows-drift_view-body-4bc74b21 = Aufgabe

repo-workflows-drift_view-body-65fb660d = Die Aufgabe des Agenten (beim Worktree gespeichert, nie committet)

repo-workflows-drift_view-body-7d9eb7ac = Schließen

repo-workflows-drift_view-body-a8164b33 = Abweichung vom Aufgabenbereich: { $view-worktree-name-1 } ({ $branch })

repo-workflows-drift_view-body-cdc38d02 = Verknüpftes Issue #{ $n }: { $text-lines-next-unwrap-or-defaul-1 }

repo-workflows-drift_view-body-e039d62e = Führt claude -p oder codex exec hier mit Aufgabe und Pfaden ohne Dateiinhalte aus und zeigt, welche Einträge laut Agent nicht benötigt werden

repo-workflows-drift_view-body-fa4f662d = Verknüpftes Issue #{ $n }: { $e }

repo-workflows-drift_view-item-518baa09 = In diesem Worktree nicht committet. Öffnet den Worktree mit seinen Änderungen.

repo-workflows-drift_view-item-bccfdf69 = Öffnet den Diff

repo-workflows-drift_view-item-d91938be = Öffnet:
    { $tip-join-n-1 }

repo-workflows-drift_view-measure-drift-27f1506d = Die Forge des Repositorys ist unbekannt

repo-workflows-drift_view-report-3ecc0ca7 = Aufgabe: { $task-lines-next-unwrap-or-defaul-1 }

repo-workflows-drift_view-report-57f4cf0a = Keine Aufgabenbeschreibung vorhanden. Schreiben Sie oben eine oder verknüpfen Sie ein Issue, um die Ordner einzuordnen.

repo-workflows-drift_view-save-task-a40b28e7 = Neben den Git-Daten des Worktrees gespeichert

repo-workflows-drift_view-sections-14063185 = Neue Abhängigkeiten ({ $report-dependencies-len-1 })

repo-workflows-drift_view-sections-1bb434ba = Von der Aufgabe nicht erwähnte Ordner ({ $unrelated-len-1 })

repo-workflows-drift_view-sections-4cf1b0e4 = (oberste Ebene)

repo-workflows-drift_view-sections-619bb740 = Alle geänderten Dateien ({ $report-files-len-1 })

repo-workflows-drift_view-sections-819e9567 = { $dep-name-1 } in { $dep-manifest-2 }

repo-workflows-drift_view-sections-a8299dbd = Geänderte öffentliche Signaturen ({ $report-signatures-len-1 })

repo-workflows-email-apply-61c57973 = Ungültiger Mailboxpfad

repo-workflows-email-compare-6921220f = Committen oder stashen Sie zuerst die Worktree-Änderungen

repo-workflows-email-compare-b5921a15 = Worktree-HEAD, Branch, Änderungen, Konfiguration oder ausgewähltes Profil geändert. Lesen Sie den Stand beziehungsweise die Vorschau erneut.

repo-workflows-email-email-act-3eece223 = Empfänger geändert. Erstellen Sie eine neue Vorschau.

repo-workflows-email-email-act-5474eef8 = Wird ausgeführt …

repo-workflows-email-email-act-84f8f92f = Vor dem Start abgebrochen

repo-workflows-email-email-act-c19c9f05 = E-Mail-Patches · b4

repo-workflows-email-email-act-fa043d04 = Nicht registriert oder b4 nicht verfügbar: { $e }

repo-workflows-email-email-window-035719bf = Vorherige gesendete Message-ID

repo-workflows-email-email-window-03712771 = Empfangen und prüfen

repo-workflows-email-email-window-1219b3d1 = Nummerierte Revision setzen

repo-workflows-email-email-window-15173718 = Serie mit b4 abrufen

repo-workflows-email-email-window-179e0bb7 = Public-Inbox-HTTPS-URL oder Message-ID

repo-workflows-email-email-window-1e3943ad = Vorbereiteten Branch und Anschreiben lesen

repo-workflows-email-email-window-2a42475c = Vorschau ohne E-Mail-Versand erstellt

repo-workflows-email-email-window-2e516d68 = Revision

repo-workflows-email-email-window-381b0db7 = Erstellen und überarbeiten

repo-workflows-email-email-window-3d343b32 = Ausgehende Vorschau erstellen (ohne Versand)

repo-workflows-email-email-window-49077514 = Revision (0 = neueste)

repo-workflows-email-email-window-54194499 = Mir ist bewusst, dass die b4-Vorbereitung diesen Branch und seine Metadaten umschreiben kann

repo-workflows-email-email-window-54cdddfe = Vorschau und Versand

repo-workflows-email-email-window-582e1946 = Anschreiben mit b4 speichern

repo-workflows-email-email-window-624d6ba0 = Serie abgerufen. Prüfen Sie Anschreiben, Patches und Trailer vor dem Anwenden.

repo-workflows-email-email-window-632693ae = { $error }
    Prüfen Sie nach Abbruch oder Teilausführung Git, b4 und E-Mail-Zustand vor einem neuen Versuch. Konflikte beim Anwenden verwenden die vorhandenen Vorgangsaktionen „Fortsetzen“ und „Abbrechen“.

repo-workflows-email-email-window-6ad695fc = Ich habe alle Empfänger und Nachrichten geprüft und möchte echte E-Mails senden

repo-workflows-email-email-window-8c1d9c9f = Profil geändert. Vorgang abgebrochen. Prüfen Sie vor einem neuen Versuch Repository und E-Mail-Zustand.

repo-workflows-email-email-window-8c3ad598 = Benötigt den unterstützten b4-Quellbuild ce4c1158 (0.17-dev), installiert mit seinem Python-Einstiegspunkt in einer virtuellen Umgebung auf diesem Rechner. Der Python-Helfer verwendet den isolierten Modus; reine user-site-/PYTHONPATH-Installationen werden nicht unterstützt. Andere Builds werden bis zur Vertragsprüfung abgelehnt. Verwendet b4s eingerichtete Identität, Public-Inbox und Versandtransport. gitcraft exportiert keine Zugangsdaten.

repo-workflows-email-email-window-99fe2a7b = Der Versand erzeugt Date-, Message-ID- und Signaturheader neu. Empfänger, Betreff und decodierte MIME-Inhalte werden gegen diese Vorschau geprüft. b4s Verhalten für erfolgreiche Versand-Tags und Überarbeitungen gilt. Fehlgeschlagene native Vorprüfungen oder unbekannte Rückfragen brechen ab. Nur die abschließende Versandbestätigung wird durch diese ausdrückliche Freigabe beantwortet.

repo-workflows-email-email-window-9cb42639 = Für HEAD { $received-snapshot-head-1 } abgerufen · { $received-messages-len-2 } Patches

repo-workflows-email-email-window-b0fcf938 = Zusätzliche Empfänger. b4 ermittelt Empfänger auch aus Anschreiben, Konfiguration und Review-Trailern. Prüfen Sie unten jede erzeugte Nachricht.

repo-workflows-email-email-window-b28037e9 = Cc

repo-workflows-email-email-window-b64a0d4f = Vorgang abbrechen

repo-workflows-email-email-window-b8c5cd6e = Vorbereiteten Branch gelesen

repo-workflows-email-email-window-bc346e4f = Aktuellen Branch registrieren

repo-workflows-email-email-window-c19c9f05 = E-Mail-Patches · b4

repo-workflows-email-email-window-c6755006 = Worker beendet oder abgebrochen. Prüfen Sie vor einem neuen Versuch Repository und E-Mail-Zustand. E-Mails könnten bereits zugestellt sein.

repo-workflows-email-email-window-c962be80 = Abzweigpunkt

repo-workflows-email-email-window-d9adba22 = Anschreiben

repo-workflows-email-email-window-e6884e31 = Geprüfte Serie über b4 senden

repo-workflows-email-email-window-e6ec08b2 = Gesendete Serie überarbeiten

repo-workflows-email-email-window-e70c5648 = Geprüfte Serie anwenden (git am --3way)

repo-workflows-email-email-window-f4b06ef6 = An

repo-workflows-email-guard-d353a99e = Abgebrochen

repo-workflows-email-interpreter-409abb66 = Installiertes b4 benötigt ein Python-Skript als Einstiegspunkt

repo-workflows-email-interpreter-d5e7d057 = Nicht unterstützter b4-Einstiegspunkt. Verwenden Sie den Python-Einstiegspunkt des unterstützten Quellbuilds.

repo-workflows-email-interpreter-fdf4c44a = b4-Einstiegspunkt hat keine Python-Shebang

repo-workflows-email-invoke-b4-8728290f = Installieren Sie b4 in einem absoluten ausführbaren PATH-Verzeichnis (Python-Virtualenv wird unterstützt)

repo-workflows-email-invoke-b4-c9d943f4 = Installierter b4-Einstiegspunkt kann nicht gelesen werden: { $e }

repo-workflows-email-mailbox-10ff6da2 = Serie überschreitet 32 MiB

repo-workflows-email-mailbox-52374212 = Git-am-Mailboxumschlag von b4 erwartet

repo-workflows-email-mailbox-ede3c4f9 = Mehr als 1024 Nachrichten

repo-workflows-email-messages-07ed6ccf = E-Mail

repo-workflows-email-messages-1edf47d5 = Betreff:

repo-workflows-email-messages-8e0d8018 = Nachricht { $index-1-1 } · { $message-headers-lines-find-l-l-s-2 }

repo-workflows-email-messages-a6252664 = Rohe RFC822-Nachricht

repo-workflows-email-new-21656993 = Privates E-Mail-Verzeichnis kann nicht angelegt werden

repo-workflows-email-parse-8107bb41 = E-Mail überschreitet 32 MiB

repo-workflows-email-parse-ff511962 = E-Mail hat keinen Subject-Header

repo-workflows-email-parse-ff6e2278 = Unvollständiger mbox-Umschlag

repo-workflows-email-parts-18af1d8d = [{ $mail-ctype-mimetype-1 }-Anhang: { $raw-len-2 } decodierte Bytes]{ "\u000A" }

repo-workflows-email-parts-93b357ba = MIME-Verschachtelung überschreitet 24 Ebenen

repo-workflows-email-prepare-7fa1230f = Positive Revisionsnummer erwartet

repo-workflows-email-prepare-ad82832b = Unbekannte Vorbereitungsaktion

repo-workflows-email-prepare-b8cd601f = Geben Sie eine Abzweigpunkt-Revision ein

repo-workflows-email-prepare-d287cfde = Revision muss positiv sein

repo-workflows-email-preview-2345df14 = Begrenzte ausgehende Nachricht kann nicht gelesen werden

repo-workflows-email-preview-7a897da3 = Ungültiger ausgehender Dateiname

repo-workflows-email-preview-ca11bd96 = Mehr als 1024 ausgehende Nachrichten

repo-workflows-email-preview-d135ecfa = Ausgehende Serie überschreitet 32 MiB

repo-workflows-email-preview-ff2f2c5c = b4 hat keine ausgehenden Nachrichten erzeugt

repo-workflows-email-receive-3b985b29 = Begrenzte Mailbox kann nicht gelesen werden

repo-workflows-email-receive-4350d1be = b4 hat keine Mailbox erzeugt

repo-workflows-email-receive-61e11166 = Ungültiger Mailboxname

repo-workflows-email-receive-7fcd485d = Nicht-UTF-8-Ausgabedateiname von b4

repo-workflows-email-receive-9820d0e4 = Begrenztes Anschreiben kann nicht gelesen werden

repo-workflows-email-receive-b0772e5b = b4 hat mehrere Mailboxen erzeugt. Wählen Sie eine einzelne Serie.

repo-workflows-email-recipients-5fb6b41e = Ungültige Empfängerliste

repo-workflows-email-recipients-c706c00a = Leere Empfängerliste

repo-workflows-email-recipients-e0cee753 = Empfängerliste: { $e }

repo-workflows-email-reroll-id-17d9d618 = Unausgeglichene Message-ID-Klammern

repo-workflows-email-reroll-id-bbeca305 = Verwenden Sie die vorherige gesendete Message-ID, keine URL

repo-workflows-email-send-9753828b = Versand meldete keinen Erfolg: { $e }. Einige E-Mails könnten bereits zugestellt sein. Prüfen Sie vor einem neuen Versuch b4 und den E-Mail-Zustand.

repo-workflows-email-send-a590b131 = b4 meldet erfolgreichen Versand. Prüfen Sie vor einem weiteren Versand die nativen Versand-Tags und die Revision.
    { $output }

repo-workflows-email-send-reviewed-e9663ecb = Ausgehende Empfänger oder Inhalte geändert. Erstellen Sie vor dem Versand eine neue Vorschau.

repo-workflows-email-snapshot-2627ca9e = .b4-config muss im Worktree lesbar und höchstens 1 MiB groß sein

repo-workflows-email-snapshot-a55ce185 = E-Mail-Patches benötigen derzeit einen lokalen Worktree und den dort installierten unterstützten b4-Quellbuild ce4c1158 (0.17-dev)

repo-workflows-email-source-034af213 = Geben Sie eine Public-Inbox-HTTPS-URL oder Message-ID ein

repo-workflows-email-source-51ea03e4 = Inbox-URL darf keine Zugangsdaten enthalten

repo-workflows-email-source-60f601ea = HTTPS-URL oder Message-ID erwartet

repo-workflows-fence_view-body-7d9eb7ac = Schließen

repo-workflows-fence_view-body-803f082f = Agent-Schutzgrenze

repo-workflows-fence_view-body-98912c89 = Zusätzlich einen Abschnitt in { $fence-agents-1 } schreiben (Codex)

repo-workflows-fence_view-body-a8aa1101 = Speichern und an Agenten schreiben

repo-workflows-fence_view-body-b31ba75f = Eigener Abschnitt in einer üblicherweise versionierten Datei, die dadurch als geändert erscheint

repo-workflows-fence_view-body-b42a9a6f = Vorschläge hinzufügen

repo-workflows-fence_view-body-c408ae8c = Zusätzlich { $fence-cursor-1 } schreiben

repo-workflows-fence_view-body-cac5a24a = Nicht in die Konfigurationen aller Worktrees geschrieben:
    { $write-errors-join-n-1 }

repo-workflows-fence_view-body-d10df00e = Eigener Block in einer möglicherweise versionierten Datei, die dadurch als geändert erscheint

repo-workflows-fence_view-body-d1953461 = Für Agenten gesperrte Pfade, einer pro Zeile, in .gitignore-Syntax ohne Ausnahmen (secrets/, .env, *.pem, /Cargo.lock), unabhängig von Großschreibung. In der Git-Konfiguration dieses Repositorys gespeichert und von seinen Worktrees geteilt. Die Regeln werden jetzt und für künftige Worktrees in die Claude-Code-Sperrregeln geschrieben ({ $fence-claude-1 }, von Git ausgeschlossen). Da Regeln umgangen werden können, wird jeder verknüpfte Worktree geprüft. Berühren seine Commits, Änderungen oder unversionierten beziehungsweise ignorierten Dateien die Grenze oder scheitert die Prüfung, wird er rot. Merge, Fast-Forward und Rebase prüfen zuerst, ob Branch-Commits oder Worktree die Grenze berührt haben, und fragen dann nach.

repo-workflows-fence_view-confirm-body-e537b5dc = { $branch } (im Worktree { $wt } ausgecheckt)

repo-workflows-fence_view-fence-gate-214a3091 = Trotzdem mergen

repo-workflows-fence_view-fence-gate-2b8e4f3b = Agent-Schutzgrenze wird geprüft …

repo-workflows-fence_view-fence-gate-6b730ef5 = Gegen Agent-Schutzgrenze prüfen

repo-workflows-fence_view-list-violations-a6d5a69b = { "  " }und { $found-len-listed-1 } weitere

repo-workflows-fence_view-save-7bfa9a3b = Gespeichert, aber nicht überall:
    { $errors-join-n-1 }

repo-workflows-file_comparison-assemble-1641af0b = Mehr als 50.000 Patch-Zeilen. Vergleichen Sie einen kleineren Bereich.

repo-workflows-file_comparison-assemble-56abde67 = Vergleich verschobenen Codes abgebrochen

repo-workflows-file_comparison-boundary-26efe746 = Binär- oder Nicht-UTF-8-Versionen: Vergleichen Sie die Bytevorschau. Zeilenweises Vormerken ist nicht verfügbar.

repo-workflows-file_comparison-boundary-5421cc1b = Der rohe Patch ist kein UTF-8-Text

repo-workflows-file_comparison-boundary-6732715f = Git konnte für diese unterschiedlichen Bytes keinen rohen Textpatch erstellen. Verwenden Sie die normale Patch-Ansicht.

repo-workflows-file_comparison-boundary-b861cbb0 = Git konnte diese Rohversionen nicht ausrichten

repo-workflows-file_comparison-boundary-button-033f86f9 = Aus Staging-Area entfernen

repo-workflows-file_comparison-boundary-button-0cadc525 = { $text } Lupenzeile { $index-1-1 }

repo-workflows-file_comparison-boundary-button-de838855 = Vormerken

repo-workflows-file_comparison-boundary-e44610d7 = Mehr als 20.000 Zeilen in einer Version. Verwenden Sie die normale Patch-Ansicht.

repo-workflows-file_comparison-comparison-pick-28dc27ab = { $pick-verb-1 } { $snapshot-spec-path-2 } aus der Staging-Lupe

repo-workflows-file_comparison-file-comparison-view-04166b68 = Dateien bilden Zeilen, fixierte Branch-Commits die Spalten. V-Markierungen kennzeichnen gleiche Bytes auch über Zeilen hinweg. Fehlende Zellen und Dateimodi bleiben getrennt. Klicken Sie zwei Zellen für einen Inhaltsvergleich an.

repo-workflows-file_comparison-file-comparison-view-103acf25 = Drei Versionen lesen

repo-workflows-file_comparison-file-comparison-view-1e2e1383 = Branches auswählen ({ $state-selected-len-1 } ausgewählt, höchstens 8)

repo-workflows-file_comparison-file-comparison-view-21819769 = Von

repo-workflows-file_comparison-file-comparison-view-22d5cf54 = Rohe Stände · nach Repository-Änderungen aktualisieren · keine automatischen Downloads

repo-workflows-file_comparison-file-comparison-view-2ad51b3c = Dateiübergreifend verschobener Code

repo-workflows-file_comparison-file-comparison-view-2d2cd49c = Vorgemerkte Verschiebungen

repo-workflows-file_comparison-file-comparison-view-327070c2 = HEAD → vorgemerkt → Arbeitskopie. Merken Sie Zeilen an beiden Grenzen vor oder entfernen Sie diese daraus. Aktualisieren Sie bei Dateiänderungen. Binärdateien und Dateien mit Git-Konvertierungen sind hier schreibgeschützt.

repo-workflows-file_comparison-file-comparison-view-3f64b850 = Dateiansicht aktualisieren

repo-workflows-file_comparison-file-comparison-view-40379eb3 = Normalen Patch öffnen

repo-workflows-file_comparison-file-comparison-view-50009ce1 = Datei

repo-workflows-file_comparison-file-comparison-view-51d4ac7c = Commit → Arbeitskopie

repo-workflows-file_comparison-file-comparison-view-5fa4365c = Unveränderliche Versionen werden gelesen …

repo-workflows-file_comparison-file-comparison-view-725513de = Vergleich abgebrochen. Aktualisieren Sie die Ansicht für einen neuen Versuch.

repo-workflows-file_comparison-file-comparison-view-74ccd433 = Ordner

repo-workflows-file_comparison-file-comparison-view-7b8bea18 = Dateiansicht schließen

repo-workflows-file_comparison-file-comparison-view-82a9c46f = Commit

repo-workflows-file_comparison-file-comparison-view-86154135 = Dateimigrationen suchen

repo-workflows-file_comparison-file-comparison-view-89fa7940 = Nach / Commit

repo-workflows-file_comparison-file-comparison-view-90e2e8fa = Dateiversionen über Branches

repo-workflows-file_comparison-file-comparison-view-aa2b8150 = Git erkennt Umbenennungen und Kopien ab 50 % Ähnlichkeit unter höchstens 2.000 Kandidaten. Merken Sie umgeordnete Dateien zuerst vor, um neue Arbeitsdateinamen einzuschließen. Inhaltsänderungen öffnen sich getrennt von der Verschiebung.

repo-workflows-file_comparison-file-comparison-view-ca74fb37 = Vergleich abbrechen

repo-workflows-file_comparison-file-comparison-view-d1d29085 = Arbeitskopie

repo-workflows-file_comparison-file-comparison-view-daec459a = Vorgemerkt

repo-workflows-file_comparison-file-comparison-view-dfc3916d = Verschobenen Code suchen

repo-workflows-file_comparison-file-comparison-view-edc82098 = Dateimigrationskarte

repo-workflows-file_comparison-file-comparison-view-efa19808 = Commit-Verschiebungen

repo-workflows-file_comparison-file-comparison-view-f08307c5 = Vergleicht gelöschte und hinzugefügte Blöcke über versionierte Dateien. Leerraum an Zeilenrändern wird ignoriert. Merken Sie neue Dateien zuerst vor. Begrenzt auf 300 Dateien, 50.000 Zeilen und 40 Sekunden. Verschlüsselte Dateien sind ausgeschlossen.

repo-workflows-file_comparison-file-comparison-view-f2473f24 = Zwischen Commits

repo-workflows-file_comparison-file-comparison-view-f315d745 = Repository-Wurzel

repo-workflows-file_comparison-file-comparison-view-f4b06ef6 = Nach

repo-workflows-file_comparison-file-comparison-view-fb60b8e0 = Staging-Lupe mit drei Zuständen

repo-workflows-file_comparison-file-comparison-view-fdc19d89 = Branchversionen vergleichen

repo-workflows-file_comparison-invalidate-81f92ce0 = Repository geändert. Aktualisieren Sie die Dateiansicht.

repo-workflows-file_comparison-lens-3a1781b9 = Lange Zeilen sind bei 2.000 Zeichen gekürzt. Grenzaktionen verwenden die vollständige Rohzeile. Verwenden Sie den normalen Patch zur Prüfung.

repo-workflows-file_comparison-lens-4902e29c = Die Arbeitskopie entspricht HEAD, aber der nächste Commit ändert diese Datei trotzdem. Arbeitsänderungen nehmen die vorgemerkte Änderung zurück.

repo-workflows-file_comparison-lens-6e60c4a1 = Gesamte Grenze aus der Staging-Area entfernen

repo-workflows-file_comparison-lens-75673075 = ← aus Staging-Area entfernen

repo-workflows-file_comparison-lens-8f600488 = Gesamte Grenze vormerken

repo-workflows-file_comparison-lens-92bc93d0 = ← vormerken

repo-workflows-file_comparison-lens-b72fb6c1 = nimmt vorgemerkte Änderung zurück

repo-workflows-file_comparison-lens-bfae8a90 = HEAD verwendet { $previous }. Das Entfernen von Inhalten aus der Staging-Area behält die vorgemerkte Verschiebung. Ändern Sie die Verschiebung selbst im normalen Patch.

repo-workflows-file_comparison-lens-d1d29085 = Arbeitskopie

repo-workflows-file_comparison-lens-daec459a = Vorgemerkt

repo-workflows-file_comparison-matrix-50009ce1 = Datei

repo-workflows-file_comparison-matrix-6be36ca4 = Fehlt

repo-workflows-file_comparison-matrix-c81d97c3 = { $label }
    Fixiert: { $c-oid-1 }

repo-workflows-file_comparison-matrix-ebfcc1d5 = { $data-paths-len-1 } Dateien · nach der ersten eine weitere Zelle auswählen

repo-workflows-file_comparison-migration-06869a99 = Ähnlichkeit unbekannt

repo-workflows-file_comparison-migration-208e67e7 = { $m-before-name-1 } → { $m-after-name-2 } · { $if-m-copy-copy-else-rename-3 } { $m-similarity-map-s-format-s-unwr-4 }

repo-workflows-file_comparison-migration-3064d79a = Umbenennung

repo-workflows-file_comparison-migration-35255dde = Git-Erkennungswarnung (einige Verschiebungen bleiben eventuell unbekannt): { $warning }

repo-workflows-file_comparison-migration-41aec147 = { $data-moves-len-1 } Umbenennungen/Kopien · { $data-other-changes-2 } andere Änderungen

repo-workflows-file_comparison-migration-a419e1f7 = Keine von Git erkannten Umbenennungen oder Kopien. Hinzugefügte oder gelöschte Pfade gelten nicht automatisch als Verschiebungen.

repo-workflows-file_comparison-migration-e21f935f = Kopie

repo-workflows-file_comparison-open-file-comparison-a6cb5dc1 = Schließen oder beenden Sie zuerst den Editor

repo-workflows-file_comparison-paths-28803ea3 = Lösen Sie Konflikte vor der Analyse verschobenen Codes

repo-workflows-file_comparison-paths-58fb2d6a = Änderungsstatus fehlt

repo-workflows-file_comparison-paths-59a504f9 = { $files-display-path-path-1 }: Nicht-UTF-8-Pfad übersprungen

repo-workflows-file_comparison-paths-ae3de607 = Unerwarteter Dateiänderungsstatus

repo-workflows-file_comparison-paths-d4097e54 = Mehr als 300 geänderte Dateien. Vergleichen Sie einen kleineren Bereich.

repo-workflows-file_comparison-paths-e3561990 = Unvollständige Liste geänderter Dateien

repo-workflows-file_comparison-prepare-da3978aa = Ungültige Vergleichsseiten

repo-workflows-file_comparison-read-0378d2b4 = Vorgemerkte Änderungen · HEAD → Index

repo-workflows-file_comparison-read-259fd376 = HEAD während der Analyse geändert. Aktualisieren Sie die Ansicht.

repo-workflows-file_comparison-read-3fbe0e2d = Unbekannte Quelle für verschobenen Code

repo-workflows-file_comparison-read-431f5828 = Analyse verschobenen Codes abgebrochen oder länger als 40 Sekunden

repo-workflows-file_comparison-read-5e3dd301 = Commit { $oid-8-1 } · erster Elterncommit

repo-workflows-file_comparison-read-6289d4ba = { $files-display-path-path-as-bytes-1 }: verschlüsselter Inhalt ausgeschlossen

repo-workflows-file_comparison-read-6adc8019 = Dateiinhalt während der Analyse geändert. Aktualisieren Sie die Ansicht.

repo-workflows-file_comparison-read-a6767f12 = Geänderte Patches überschreiten 6 MiB. Vergleichen Sie einen kleineren Bereich.

repo-workflows-file_comparison-read-bf031806 = { $oid-8-1 } → Arbeitskopie (versionierte Dateien)

repo-workflows-file_comparison-read-f42f63e5 = { $files-display-path-path-as-bytes-1 }: Nicht-UTF-8-Patch übersprungen

repo-workflows-file_comparison-read-fc054b54 = Betroffene Pfade während der Analyse geändert. Aktualisieren Sie die Ansicht.

repo-workflows-file_comparison-read-fcc615d7 = Arbeitskopie · Index → Arbeitskopie (versionierte Dateien)

repo-workflows-file_comparison-show-70bfd7fb = Zeilenumbruch

repo-workflows-file_comparison-show-pair-06ebe02e = Bytevorschau (erste 512 Bytes pro Seite; vollständige Größen stehen oben)

repo-workflows-file_comparison-show-pair-2f04c026 = Keine Inhaltsänderungen: identische Bytes. Namen, Vorhandensein und Dateimodi bleiben getrennt.

repo-workflows-file_comparison-show-pair-7b68fe55 = Nachher

repo-workflows-file_comparison-show-pair-98154a15 = Nur Inhaltsänderungen

repo-workflows-file_comparison-show-pair-9bb72500 = Vorher

repo-workflows-file_comparison-show-pair-e3ccb1fe = Vorher: { $pair-versions-0-label-1 } · Nachher: { $pair-versions-1-label-2 }

repo-workflows-file_comparison-view-4699e093 = { $if-side-0-source-else-destinatio-1 } { $l-label-2 } öffnen

repo-workflows-file_comparison-view-746eb9db = { $data-locations-len-1 } dateiübergreifend verschobene Blöcke · gemeinsame M-Markierungen verbinden beide Stellen

repo-workflows-file_comparison-view-8bb733fc = M{ $i-1-1 } · Verschoben nach …

repo-workflows-file_comparison-view-a47d33a8 = Keine dateiübergreifenden Verschiebungen gefunden (mindestens 20 Buchstaben oder Ziffern). Verschiebungen innerhalb einer Datei bleiben im normalen Patch.

repo-workflows-file_comparison-view-b8b66b62 = unverändert / neu eingerückt

repo-workflows-file_comparison-view-baa2bc90 = beim Verschieben bearbeitet

repo-workflows-forge_link-deliver-5add4905 = Browser konnte für { $link-url-1 } nicht geöffnet werden: { $e }

repo-workflows-forge_link-deliver-8d525e5f = Kopiert

repo-workflows-forge_link-deliver-aab63f85 = Link öffnen

repo-workflows-forge_link-deliver-b19fb8d1 = Geöffnet

repo-workflows-forge_link-deliver-c5e2849f = Link { $done }, aber { $warning }: { $link-url-1 }

repo-workflows-forge_link-deliver-dbf362d4 = Link kopieren

repo-workflows-forge_link-deliver-f9eecf25 = Keine Webadresse: { $link-url-1 }

repo-workflows-forge_link-items-44c5ece2 = { $what } öffnen auf

repo-workflows-forge_link-items-55932e34 = Auf { $c-forge-1 }. Der Branch verfolgt kein Remote und es gibt kein origin. Wählen Sie eines aus.

repo-workflows-forge_link-items-6bb03074 = Dauerhafter Link über den Commit-Hash auf { $one-remote-1 } ({ $one-place-2 })

repo-workflows-forge_link-items-78bef62f = { $what } auf { $one-forge-1 } öffnen

repo-workflows-forge_link-items-a5232678 = Link zu { $what } kopieren

repo-workflows-forge_link-items-ea6a597e = Die Branchseite auf { $one-remote-1 } ({ $one-place-2 }). Sie folgt dem Branch bei Änderungen.

repo-workflows-forks-allow-edits-6bb2bbb0 = Bearbeitung durch Maintainer erlauben

repo-workflows-forks-allow-edits-9e60d9b6 = Commits von Mitgliedern erlauben, die in den Zielbranch mergen dürfen

repo-workflows-forks-allow-edits-ddc30ba0 = Bearbeitung durch Maintainer erlauben

repo-workflows-forks-approve-runs-282082c7 = Workflow-Läufe von #{ $number } konnten nicht freigegeben werden: { $e }

repo-workflows-forks-approve-runs-2f659b28 = Workflow-Läufe von #{ $number } werden freigegeben …

repo-workflows-forks-approve-runs-a151ad19 = Workflow-Läufe von #{ $number } freigegeben

repo-workflows-forks-approve-runs-a77eca22 = Die Freigabe führt den Code des Mitwirkenden auf Ihrer CI aus. Prüfen Sie zuerst die Änderungen von #{ $number } und wählen Sie dann „Freigeben und erneut ausführen“.

repo-workflows-forks-badges-42d0a9e0 = Bearbeitung erlaubt

repo-workflows-forks-badges-84e3eb92 = wartet auf Freigabe

repo-workflows-forks-badges-aea3a983 = { $info-owner-1 } erlaubt Maintainern den Push auf den Branch { $info-branch-2 }. Wählen Sie im Menü „Auschecken zum Pushen nach { $info-owner-3 }“.

repo-workflows-forks-badges-e3d8fbdd = Erster Beitrag von { $info-owner-1 } zu diesem Repository

repo-workflows-forks-checkout-00c3592c = Der Branchname von #{ $info-number-1 } ({ $info-branch-2 }) kann nicht unverändert verwendet werden

repo-workflows-forks-checkout-13fc08d7 = Der Branch von #{ $info-number-1 } liegt in diesem Repository. Checken Sie ihn wie einen normalen Branch aus.

repo-workflows-forks-checkout-43a1fda6 = Die Forge hat den Ort des Forks von { $info-owner-1 } nicht angegeben

repo-workflows-forks-checkout-625f9e12 = { $info-owner-1 } erlaubt Maintainern keinen Push auf den Branch von #{ $info-number-2 }

repo-workflows-forks-checkout-d005bf20 = { $info-branch-1 }, { $info-owner-2 }/{ $info-branch-3 } und pr-{ $info-number-4 } existieren hier bereits als Branches

repo-workflows-forks-checkout-fork-5ecb3bac = Die Herkunft von #{ $number } ist noch unbekannt

repo-workflows-forks-checkout-fork-abaaf4c3 = #{ $number } auschecken, um nach { $info-owner-1 } zu pushen

repo-workflows-forks-count-082131a5 = Unerwartete Anzahl: { $e }

repo-workflows-forks-fork-items-2b48edeb = Checken Sie zuerst den Branch von { $info-owner-1 } zum Rück-Pushen aus

repo-workflows-forks-fork-items-3450c709 = Rebase auf { $info-base-1 } und Push nach { $info-owner-2 }

repo-workflows-forks-fork-items-6c1751f8 = { $info-owner-1 } erlaubt Maintainern keinen Push nach { $info-branch-2 }. Bitten Sie um die Aktivierung von „{ $allow-edits-self-forge-3 }“.

repo-workflows-forks-fork-items-7e44168f = Bereits eingerichtet: Push geht an den Branch { $info-owner-1 } von { $info-branch-2 }

repo-workflows-forks-fork-items-9862a39c = Der Branch kann nicht zum Pushen ausgecheckt werden. Sein Repository existiert nicht mehr.

repo-workflows-forks-fork-items-99b20173 = Freigeben und ausführen ({ $waiting }) …

repo-workflows-forks-fork-items-9e40421a = { $b-name-1 } auschecken (pusht nach { $info-owner-2 })

repo-workflows-forks-fork-items-a0b3b5ed = Ruft { $info-base-1 } und den Branch von { $info-owner-2 } ab, führt einen Rebase auf { $info-base-3 } aus und pusht dann mit Lease zurück, nachdem die Auswirkungen angezeigt wurden

repo-workflows-forks-fork-items-a33fed38 = Fügt den Fork von { $info-owner-1 } als Remote hinzu, sofern noch nicht vorhanden, checkt den Branch { $info-branch-2 } mit Tracking aus und setzt pushRemote für Rück-Pushes. Ctrl+Z macht dies rückgängig.

repo-workflows-forks-fork-items-afc89aab = Gibt Workflow-Läufe des neuesten Commits frei, die dann fremden Code auf Ihrer CI ausführen. Prüfen Sie zuerst die Änderungen.

repo-workflows-forks-fork-items-bdee4479 = Fork wurde gelöscht

repo-workflows-forks-fork-items-c8fd7141 = Zum Pushen nach { $info-owner-1 } auschecken

repo-workflows-forks-forks-op-done-8287ad6b = { $plan-local-1 } ist hier rebasiert, aber nicht nach { $plan-fork-2 } gepusht. Ctrl+Z stellt den vorherigen Stand wieder her.

repo-workflows-forks-forks-op-done-9c338e90 = Der Rebase hat bei Konflikten angehalten. Lösen Sie diese und setzen Sie ihn fort. gitcraft bietet danach den Push an. Alternativ können Sie abbrechen.

repo-workflows-forks-offer-push-after-conflict-731ce7ed = Pushen

repo-workflows-forks-offer-push-after-conflict-dcc709d6 = Der Rebase ist abgeschlossen. Force-Push von { $plan-local-1 } auf den Branch { $plan-fork-2 } von { $plan-branch-3 } mit Lease. Wird abgelehnt, falls seit dem Abruf dorthin gepusht wurde. Ctrl+Z stellt den fremden Branch wieder her (und das nächste Ctrl+Z Ihren).

repo-workflows-forks-plan-fork-rebase-1abac341 = Dieses Repository hat kein Remote als Rebase-Ziel

repo-workflows-forks-plan-fork-rebase-a3647cfa = Rebase und Push

repo-workflows-forks-plan-fork-rebase-ac71056e = Branch und Basis von #{ $number } werden abgerufen …

repo-workflows-forks-plan-rebase-6ccf1bc5 = { $local } enthält bereits alle Commits von { $upstream }/{ $info-base-1 }

repo-workflows-forks-plan-rebase-a0b078e0 = Checken Sie zuerst den Branch von #{ $info-number-1 } für Pushes nach { $info-owner-2 } aus

repo-workflows-forks-plan-rebase-b597bd2f = Checken Sie zuerst den Branch von #{ $info-number-1 } für Pushes nach { $info-owner-2 } aus. Kein Remote ist dessen Fork.

repo-workflows-forks-plan-rebase-e28bbaf9 = Committen oder stashen Sie zuerst Ihre Änderungen. Dies führt einen Rebase des Branches aus.

repo-workflows-forks-plan-tidy-a7e148ae = Unbenutzte Fork-Remotes entfernen

repo-workflows-forks-plan-tidy-adf73b14 = Remotes konnten nicht gelesen werden: { $e }

repo-workflows-forks-plan-tidy-b732e7ba = Entfernt die von gitcraft für Forks hinzugefügten Remotes, von denen kein Branch mehr pullt oder zu denen er pusht: { $unused-join-1 }, samt Remote-Tracking-Branches. Ctrl+Z fügt sie wieder hinzu (für ihre Branches erneut abrufen).{ $kept }

repo-workflows-forks-plan-tidy-c3812fc4 = Entfernen

repo-workflows-forks-plan-tidy-da56e1d6 = Kein Fork-Remote kann entfernt werden{ $kept }

repo-workflows-ghost-approval-preview-1426bc08 = { $files-len-1 } Arbeitsdateien ändern sich

repo-workflows-ghost-approval-preview-675d236b = Verwirft uncommittete Arbeitsänderungen

repo-workflows-ghost-approval-preview-70be46e5 = Liste zurückbleibender Commits bei 200 gekürzt

repo-workflows-ghost-approval-preview-e74a52f6 = Hält bei Schritt { $conflict-step-1 } an: { $conflict-files-join-2 }

repo-workflows-ghost-current-7776bacf = HEAD ist losgelöst. Checken Sie zuerst einen Branch aus.

repo-workflows-ghost-delete-branch-26d05fbc = { $name } ist nicht in { $what } gemergt. Löschen wird ohne „Erzwingen (-D)“ abgelehnt. Mit Erzwingen bleiben seine eigenen Commits ohne Branch zurück.

repo-workflows-ghost-delete-branch-a9c01edc = Hier gibt es keinen Branch { $name }

repo-workflows-ghost-delete-branch-dde58e5a = seinen Upstream

repo-workflows-ghost-delete-branch-fadd1b38 = den ausgecheckten Branch

repo-workflows-ghost-drop-commit-181a54e0 = Nach dem Commit kamen Merges, die dieser Vorgang einebnen würde

repo-workflows-ghost-drop-commit-3a19c0c2 = Uncommittete Änderungen werden zuerst gestasht und danach wiederhergestellt.

repo-workflows-ghost-drop-commit-da2d4ead = Der erste Commit hat keinen Elterncommit für einen Rebase

repo-workflows-ghost-force-push-2d4460fd = Stand des letzten Abrufs von { $up-remote-1 }. Mit Lease wird der Push abgelehnt, wenn seitdem jemand gepusht hat.

repo-workflows-ghost-force-push-a9c01edc = Hier gibt es keinen Branch { $name }

repo-workflows-ghost-force-push-c637c04f = { $up-tracking-name-1 } verliert nichts. Ein normaler Push würde ausreichen.

repo-workflows-ghost-force-push-f06fee94 = { $name } verfolgt keinen Remote-Branch

repo-workflows-ghost-ghost-strip-a81f72de = Es wurde nichts ausgeführt

repo-workflows-ghost-ghost-strip-b69b3d5d = ↑ { $plural-above-new-commit-is-new-c-1 } oberhalb der sichtbaren Zeilen.

repo-workflows-ghost-ghost-strip-c4a8b8f8 = Vorschau: { $title }

repo-workflows-ghost-ghost-strip-d0c20bfd = Auswirkungen werden ermittelt …

repo-workflows-ghost-ghost-strip-f637d899 = Vorschau nicht möglich. { $sentence-one-line-why-1 }

repo-workflows-ghost-listed-683768ab = und { $items-len-listed-1 } weitere

repo-workflows-ghost-lost-line-02717c8c = Verlässt den Remote-Branch ({ $count }): { $names }

repo-workflows-ghost-lost-line-2cd6d050 = Durch neue Kopien ersetzt ({ $count }; Originale: { $originals } ohne Branch): { $names }

repo-workflows-ghost-lost-line-50788b96 = mehr als { $o-lost-len-1 } Commits

repo-workflows-ghost-lost-line-811bd7ca = Ohne Branch zurückgelassen ({ $count }): { $names }

repo-workflows-ghost-lost-line-8ccbdaa0 = Ohne Branch zurückgelassen ({ $count }, davon { $o-replaced-1 } durch neue Kopien ersetzt): { $names }

repo-workflows-ghost-outcome-lines-02e3b1f7 = Keine Arbeitsdatei ändert sich.

repo-workflows-ghost-outcome-lines-193731dd = Der Merge hält an

repo-workflows-ghost-outcome-lines-4b54308f = Geänderte Arbeitsdateien ({ $files-len-1 }): { $listed-files-f-format-f-status-f-2 }

repo-workflows-ghost-outcome-lines-7728bdcc = Hält bei Schritt { $c-step-1 } von { $c-of-2 } an ({ $describe-commit-3 })

repo-workflows-ghost-outcome-lines-81deb64d = { $name } wird bei { $shown-to-1 } erstellt

repo-workflows-ghost-outcome-lines-820df962 = Ermittelt mit { $engine-describe-1 }

repo-workflows-ghost-outcome-lines-91af17db = { $name } wird gelöscht

repo-workflows-ghost-outcome-lines-b8d8f166 = Verwirft { $describe-dropped-1 } — { $dropped-author-2 }

repo-workflows-ghost-outcome-lines-ce73844c = { $name } wird verschoben: { $short-from-1 } → { $shown-to-2 }

repo-workflows-ghost-outcome-lines-dfa9e9b6 = { $plural-fetched-commit-commits-1 } vom Remote

repo-workflows-ghost-outcome-lines-f0e05d95 = Der Commit wird leer

repo-workflows-ghost-outcome-lines-f93fde8f = Konflikte in { $listed-c-files-f-f-clone-1 }

repo-workflows-ghost-pull-3a19c0c2 = Uncommittete Änderungen werden zuerst gestasht und danach wiederhergestellt.

repo-workflows-ghost-pull-9045dbaa = Stand des letzten Abrufs von { $up-remote-1 }. Spätere Pushes werden nicht angezeigt.

repo-workflows-ghost-pull-c7917d08 = Fast-Forward: Der Branch wird auf den Remote-Commit verschoben.

repo-workflows-ghost-pull-dd489c0a = Bereits aktuell. Der Branch enthält alles vom Remote.

repo-workflows-ghost-pull-f53705f4 = { $branch-name-1 } verfolgt keinen Remote-Branch. Nichts zu pullen.

repo-workflows-ghost-rebase-1f963916 = Bereits aktuell. Nichts erneut anzuwenden, keine Änderungen.

repo-workflows-ghost-rebase-b1f8a147 = Der Rebase startet nicht mit uncommitteten Änderungen. Committen oder stashen Sie diese zuerst.

repo-workflows-ghost-rebase-e550da8a = Uncommittete Änderungen werden zuerst gestasht und danach wiederhergestellt (rebase.autoStash).

repo-workflows-ghost-reset-hard-3126bbde = Uncommittete Änderungen dieser Dateien gehen unwiderruflich verloren.

repo-workflows-ghost-rewrite-3477e4cb = Der Rebase hält dort an und fragt, ob der leer gewordene Commit erhalten bleiben soll. Der Branch bewegt sich noch nicht.

repo-workflows-ghost-rewrite-3d8b966c = rebase.rebaseMerges ist gesetzt und die Commits enthalten Merges. Git behält diese, die Vorschau kann sie noch nicht erneut anwenden.

repo-workflows-ghost-rewrite-af60968a = Geänderte Dateien werden nicht aufgelistet. Git { $self-version-1 } wendet die Commits in einem Scratch-Klon erneut an.

repo-workflows-ghost-rewrite-c84070ef = { $w-picks-len-1 } Commits erneut anzuwenden. Zu viele für die Vorschau (höchstens { $PICKS-LIMIT }).

repo-workflows-ghost-rewrite-e69265a2 = Der Rebase hält dort bis zur Konfliktlösung an. Der Branch bewegt sich noch nicht.

repo-workflows-ghost-rewrite-fb29af2f = HEAD ist währenddessen mitten im Rebase bei { $waits } losgelöst.

repo-workflows-ghost-shown-4ef6715c = ein Merge-Commit

repo-workflows-ghost-timeline-18df3f5f = Dateiänderungsvorschau unvollständig: Erforderliche Blobs fehlen lokal oder das begrenzte Lesen wurde beendet. Keine Objekte wurden heruntergeladen.

repo-workflows-ghost-timeline-48223518 = Ungültige Timeline-Commit-ID

repo-workflows-ghost-timeline-5a1baa67 = Timeline-Vorschau unvollständig: Erforderlicher lokaler Verlauf fehlt oder das begrenzte Lesen wurde beendet. Darüberfahren lädt nie Objekte herunter.

repo-workflows-ghost-timeline-6f3d9015 = Uncommittete Änderungen bleiben mit --keep erhalten. Ein kollidierender Checkout verweigert den Sprung.

repo-workflows-ghost-timeline-c25867a7 = Gespeicherte Referenzen halten frühere Zustände wiederherstellbar. Diese Vorschau bewegt weder Referenzen noch Dateien.

repo-workflows-ghost-title-20596011 = Rückgängig-Baum: { $label }

repo-workflows-ghost-title-2d281f6b = Rebase von { $current } auf { $label }

repo-workflows-ghost-title-62fc0c1a = { $branch } löschen

repo-workflows-ghost-title-ae583650 = { $current } hart auf { $short-to-1 } zurücksetzen

repo-workflows-ghost-title-b193a29c = Force-Push von { $branch }

repo-workflows-ghost-title-baaedb85 = { $mode-label-1 } in { $current }

repo-workflows-ghost-title-c58e3395 = { $branch } hier und auf dem Remote löschen

repo-workflows-ghost-title-fc4198eb = Commit { $short-hash-1 } verwerfen

repo-workflows-guide--21dad442 = Tipps ausgeschaltet. Einstellungen → Allgemein → Tipps schaltet sie wieder ein.

repo-workflows-guide--2ee88cd5 = Erster Commit erforderlich

repo-workflows-guide--b77d9539 = Setzen Sie zuerst Namen und E-Mail-Adresse (Einstellungen → Profil oder git config user.name / user.email)

repo-workflows-guide--b7acfeab = Tipps und erste Hinweise ausschalten (Einstellungen → Allgemein → Tipps schaltet sie wieder ein)

repo-workflows-guide-ask-empty-commit-348d9f75 = HEAD ist losgelöst. Checken Sie einen Branch für den ersten Commit aus.

repo-workflows-guide-confirm-dialog-82a9c46f = Committen

repo-workflows-guide-confirm-dialog-91b6a6f9 = Leeren Commit namens „{ $EMPTY-COMMIT-MESSAGE }“ auf { $name } erstellen? Dadurch erhält das Repository einen Branch zum Veröffentlichen und als Ausgangspunkt weiterer Branches. Bis zur Veröffentlichung können Sie dies rückgängig machen.

repo-workflows-guide-first-commit-step-02da0750 = Noch nichts zu committen. Dateien im Repository-Ordner erscheinen automatisch oben im Graphen. Alternativ können Sie mit einem leeren Commit beginnen.

repo-workflows-guide-first-commit-step-37bb706f = Ihre Dateien sind bereit. Schreiben Sie eine Zusammenfassung und committen Sie sie.

repo-workflows-guide-first-commit-step-6221ab4e = Ein Commit ohne Dateien: „{ $EMPTY-COMMIT-MESSAGE }“

repo-workflows-guide-first-commit-step-9416811c = Leeren Commit erstellen …

repo-workflows-guide-first-commit-step-cfa86831 = Nachricht schreiben

repo-workflows-guide-first-commit-step-d0ee469e = Öffnet die Änderungen rechts an der Commit-Zusammenfassung

repo-workflows-guide-first-commit-step-e2f5a327 = Ersten Commit erstellen

repo-workflows-guide-first-commit-step-ed251864 = Erledigt.

repo-workflows-guide-first-steps-3c4e21be = Fügen Sie zuerst ein Remote hinzu

repo-workflows-guide-first-steps-76a2896d = Checken Sie zuerst einen Branch aus

repo-workflows-guide-first-steps-view-a5b4a2a3 = Bis zum erneuten Öffnen dieses Repositorys ausgeblendet

repo-workflows-guide-first-steps-view-d4b85ef0 = Diese Schritte ausblenden

repo-workflows-guide-header-5cec6b07 = Dieses Repository hat noch keine Commits

repo-workflows-guide-header-a9174a7c = Drei Schritte für den Start:

repo-workflows-guide-header-bb24101f = Nächste Schritte für dieses Repository

repo-workflows-guide-header-cd9905c0 = Bisher nur auf diesem Rechner:

repo-workflows-guide-hint-status-2c0ee188 = Tipp schließen

repo-workflows-guide-publish-step-2ce79937 = Branch zum Remote pushen und verfolgen, damit Push und Pull direkt funktionieren.

repo-workflows-guide-publish-step-859390eb = Veröffentlichen

repo-workflows-guide-publish-step-93d6376b = { $b } zum Remote pushen und verfolgen, damit Push und Pull direkt funktionieren.

repo-workflows-guide-publish-step-ed251864 = Erledigt.

repo-workflows-guide-remote-step-06906287 = Veröffentlichungsziel, etwa ein neues leeres Repository auf einer Forge.

repo-workflows-guide-remote-step-25a81e6a = Erledigt: { $remotes-join-1 }.

repo-workflows-guide-remote-step-d20b3285 = Remote hinzufügen …

repo-workflows-guide-remote-step-ecd12a48 = Remote hinzufügen

repo-workflows-guide-start-first-commit-881f72cb = Noch nichts zu committen. Fügen Sie Dateien im Repository-Ordner hinzu oder ändern Sie welche.

repo-workflows-guide-text-414ed52e = Tipp: { $cmd }+Klick auf einen weiteren Commit vergleicht die beiden

repo-workflows-guide-text-aa7b18c5 = Tipp: „Solo“ im Branchmenü zeigt nur diesen Branch

repo-workflows-guide-text-c09ad9f4 = Tipp: F7 oder Alt+↓ springt zur nächsten Änderung, Shift+F7 oder Alt+↑ zur vorherigen

repo-workflows-guide-text-e94f4321 = Tipp: Doppelklick auf eine Branch-Pille checkt den Branch aus

repo-workflows-guide-tip-bar-3e9b3541 = Schon gewusst?

repo-workflows-guide-tip-bar-c1567088 = Nächster Tipp

repo-workflows-guide-undo-refusal-6cb7f998 = { $name } ist veröffentlicht. Das Rückgängigmachen des leeren ersten Commits würde einen auch auf dem Remote vorhandenen Branch löschen. Erstellen Sie stattdessen darauf weitere Commits.

repo-workflows-history_activity-activity-view-04b6c8fb = Zeitraum anwenden

repo-workflows-history_activity-activity-view-0e407d89 = Mitwirkendenstatistik

repo-workflows-history_activity-activity-view-101c65f2 = { " " }· flacher Klon (früherer Verlauf nicht verfügbar)

repo-workflows-history_activity-activity-view-126236b2 = Alle erfassten Daten

repo-workflows-history_activity-activity-view-1556feb2 = Autordatum

repo-workflows-history_activity-activity-view-21819769 = Von

repo-workflows-history_activity-activity-view-4562ddd7 = Committerdatum

repo-workflows-history_activity-activity-view-4a7fda65 = Aktivität aktualisieren

repo-workflows-history_activity-activity-view-4d167723 = Die Grenze von 100.000 Commits wurde erreicht. Die Zahlen sind unvollständig.

repo-workflows-history_activity-activity-view-60b300fd = Aktivitätskalender

repo-workflows-history_activity-activity-view-6561da04 = Erfasste Daten: { $first } – { $last }

repo-workflows-history_activity-activity-view-7c0879db = Momentaufnahme lokaler und entfernter Branches, Tags und HEAD. Nach Änderungen aktualisieren. Stash und interne App-Referenzen sind ausgeschlossen.

repo-workflows-history_activity-activity-view-7e33d4ef = Loslassen, um den ausgewählten Zeitraum anzuwenden.

repo-workflows-history_activity-activity-view-8efbb1ca = Aktivität schließen

repo-workflows-history_activity-activity-view-956a51f6 = Gruppieren nach

repo-workflows-history_activity-activity-view-a20fa1bb = Weiteren Verlauf erfassen

repo-workflows-history_activity-activity-view-ab181f77 = Verlaufsaktivität

repo-workflows-history_activity-activity-view-ac3896c6 = Tage verwenden die im jeweiligen Commit gespeicherte Zeitzone. Beide Grenztage gehören zum Zeitraum.

repo-workflows-history_activity-activity-view-afc6abac = Bis

repo-workflows-history_activity-activity-view-baa24799 = Dieses Repository enthält keine Commits.

repo-workflows-history_activity-activity-view-cc4e9aaf = Analyse abbrechen

repo-workflows-history_activity-activity-view-d402be35 = Die Analyse wurde an der Grenze von 64 MiB beendet. Die Zahlen gelten nur für die erfassten Commits.

repo-workflows-history_activity-activity-view-e136531e = { $coverage } · { $history-commits-len-1 } eindeutige Commits · { $history-tips-2 } Referenzspitzen{ $if-history-shallow-shallow-clone-3 }

repo-workflows-history_activity-activity-view-e23552d7 = Verlauf wird analysiert … { $view-progress-load-ordering-rela-1 } Commits gelesen

repo-workflows-history_activity-activity-view-e6f89512 = An der Grenze eines flachen Klons zählen Dateien im ersten verfügbaren Dateibaum als hinzugefügt.

repo-workflows-history_activity-activity-view-fd008e90 = Gesamter erreichbarer Verlauf

repo-workflows-history_activity-activity-view-fe26c97a = Teilweiser Verlauf

repo-workflows-history_activity-authors-626a79db = Alle Autoren anzeigen

repo-workflows-history_activity-authors-65845cf6 = Keine Aktivität in diesem Zeitraum.

repo-workflows-history_activity-authors-6cbebfa2 = Aktive Tage

repo-workflows-history_activity-authors-6e25b1da = Unterschiedliche betroffene Pfade, aktive Tage und erste bis letzte Aktivität im ausgewählten Zeitraum.

repo-workflows-history_activity-authors-76cb89fa = Autoren folgen der aktuellen .mailmap von Git. Umbenennungen betreffen den alten und neuen Pfad. Merge-Änderungen werden mit dem ersten Eltern-Commit verglichen.

repo-workflows-history_activity-authors-ce199c9e = Betroffene Dateien

repo-workflows-history_activity-authors-d09648c6 = Commits

repo-workflows-history_activity-authors-d95082a2 = Autor

repo-workflows-history_activity-authors-e707bedf = Aktivitätszeitraum

repo-workflows-history_activity-calendar-370eebba = { $month-year-1 }-{ $m }
    { $n } Commits

repo-workflows-history_activity-calendar-58339f45 = Mi

repo-workflows-history_activity-calendar-66dab40c = Fr

repo-workflows-history_activity-calendar-6a276950 = Vorheriger Monat

repo-workflows-history_activity-calendar-74e53211 = Nächster Monat

repo-workflows-history_activity-calendar-7da11212 = Do

repo-workflows-history_activity-calendar-91e33e3b = Vorheriges Jahr

repo-workflows-history_activity-calendar-a21a5dc6 = Nächstes Jahr

repo-workflows-history_activity-calendar-a54ddac5 = Monate durchsuchen

repo-workflows-history_activity-calendar-c60e09cb = { $day }
    { $n } Commits

repo-workflows-history_activity-calendar-d1eb39b0 = Di

repo-workflows-history_activity-calendar-db18f17f = So

repo-workflows-history_activity-calendar-f40d7f51 = Mo

repo-workflows-history_activity-calendar-fdeb71b5 = Sa

repo-workflows-history_activity-cancel-ddfe1c43 = Analyse abgebrochen. Zum erneuten Versuch aktualisieren.

repo-workflows-history_activity-commits-640e56ec = { $selected-len-1 } Commits im ausgewählten Zeitraum

repo-workflows-history_activity-commits-f0d98513 = Commit-Details öffnen

repo-workflows-history_activity-history-activity-window-0ad38473 = Aktivität im Repository-Verlauf

repo-workflows-history_activity-open-activity-a6cb5dc1 = Zuerst den Editor abschließen oder schließen

repo-workflows-history_activity-page-1208ec01 = Vorherige Seite

repo-workflows-history_activity-page-c08ac736 = Nächste Seite

repo-workflows-history_activity-page-dbaceb71 = Seite { $page-1-1 } von { $len-div-ceil-100-max-1-2 }

repo-workflows-history_activity-requested-range-25f6acd0 = Das Anfangsdatum darf nicht nach dem Enddatum liegen

repo-workflows-history_activity-requested-range-4d18caf5 = Datumsangaben wie 2026-11-07 verwenden

repo-workflows-hotfix-cancel-19766ed6 = Abbrechen

repo-workflows-hotfix-contents-08251562 = Signiert

repo-workflows-hotfix-contents-0e86de5c = Diese Git-Version kann Merge-Konflikte nicht vorab erkennen (ab 2.38 möglich). Bei einem Konflikt hält der Vorgang dort an.

repo-workflows-hotfix-contents-0f2dabe6 = Ein anderer Vorgang läuft. Bitte warten.

repo-workflows-hotfix-contents-1503916a = Tag

repo-workflows-hotfix-contents-296ee3d7 = Hotfix abschließen

repo-workflows-hotfix-contents-3b7c3548 = Anmerkungen zum Tag (Markdown)

repo-workflows-hotfix-contents-3d5a5069 = GEPLANTE SCHRITTE

repo-workflows-hotfix-contents-89b9dd15 = Strg+Z stellt die Branches und das Tag hier wieder her. Bereits gepushte Änderungen bleiben erhalten.

repo-workflows-hotfix-contents-96176159 = { $plan-hotfix-name-1 } hier löschen

repo-workflows-hotfix-contents-9d164a95 = Es gibt hier keinen Entwicklungsbranch (develop oder den von git-flow)

repo-workflows-hotfix-contents-a2e52e9b = Nach { $r } pushen

repo-workflows-hotfix-contents-ba6c71e9 = Und nach { $d-name-1 }

repo-workflows-hotfix-contents-c2cdf43f = git tag -s mit Ihrem Signaturschlüssel, andernfalls annotiert

repo-workflows-hotfix-contents-c5ecd20d = Pushen (dieses Repository hat kein Remote)

repo-workflows-hotfix-contents-dd94532f = { " " }seit { $t }

repo-workflows-hotfix-contents-f3077f1d = { $plan-hotfix-name-1 } · { $plan-changes-len-2 } Änderung{ $if-plan-changes-len-1-else-s-3 }{ $plan-base-tag-as-deref-map-t-for-4 }

repo-workflows-hotfix-contents-f5346a60 = Nach { $plan-main-name-1 } gemergt

repo-workflows-hotfix-hotfix-act-06b55025 = { $tag } ist keine Version, von der aus ein Hotfix erstellt werden kann

repo-workflows-hotfix-hotfix-op-done-5b11af18 = { " " }— Strg+Z macht die ausgeführten Schritte rückgängig

repo-workflows-hotfix-hotfix-op-done-fbdef87e = { $run-label-1 } (teilweise angehalten)

repo-workflows-hotfix-plan-8f734f01 = { $branch } enthält keine eigenen Commits zum Veröffentlichen

repo-workflows-hotfix-plan-97906966 = Es gibt hier keinen Hauptbranch (main, master oder den von git-flow)

repo-workflows-hotfix-plan-a798cdeb = Zuerst den Hotfix-Branch ({ $names-prefix-1 } …) auschecken

repo-workflows-hotfix-plan-e373acd0 = Änderungen zuerst committen oder stashen. Beim Abschließen werden andere Branches ausgecheckt.

repo-workflows-hotfix-plan-eeb1f496 = { $t-name-1 } ist in einem anderen Worktree ausgecheckt. Dort zuerst etwas anderes auschecken.

repo-workflows-hotfix-preview-4aeab9fa = Löscht { $h-name-1 } hier

repo-workflows-hotfix-preview-523cd105 = Erstellt auf diesem Merge nach { $self-main-name-3 } das Tag { $if-c-sign-signed-else-annotated-1 } { $c-tag-trim-2 }

repo-workflows-hotfix-preview-7df6bedd = Pusht danach { $names-join-and-1 } und das Tag gemeinsam oder gar nicht nach { $remote } (git push --atomic)

repo-workflows-hotfix-preview-ab279563 = Mergt { $h-name-1 } ({ $crate-repo-short-h-head-2 }) nach { $t-name-3 } (git merge --no-ff)

repo-workflows-hotfix-preview-e3ee915a = { " " }und{ " " }

repo-workflows-hotfix-problem-0225cdbc = Beim Mergen nach { $into } würden Konflikte in { $files-join-1 } auftreten. Manuell mergen{ $or }

repo-workflows-hotfix-problem-a56c91b5 = { " " }oder die Auswahl aufheben

repo-workflows-issue_work-card-1887e67c = beim ersten Lesen

repo-workflows-issue_work-card-1c245f79 = { $what } ausblenden

repo-workflows-issue_work-card-1e7fbd27 = Issue erneut lesen. Die Karte zeigt den Stand { $when }.

repo-workflows-issue_work-card-53611908 = am { $link-read-1 }

repo-workflows-issue_work-card-57fa7fe2 = Für dieses Issue sind keine Akzeptanzkriterien festgehalten.

repo-workflows-issue_work-card-5fa29511 = { $what } anzeigen

repo-workflows-issue_work-card-6d2189c3 = beim Arbeitsbeginn

repo-workflows-issue_work-card-7fe3e1e0 = Issue im Browser öffnen

repo-workflows-issue_work-card-9b1e287d = mit Stand vom { $link-read-1 }

repo-workflows-issue_work-card-b9210e97 = Das Issue enthält nur seinen Titel.

repo-workflows-issue_work-card-fb6318e9 = Zuordnung dieses Branches zu einem Issue entfernen

repo-workflows-issue_work-choose-104ab921 = Arbeitsort

repo-workflows-issue_work-choose-52656e81 = Branch

repo-workflows-issue_work-choose-561b5212 = Die ersten { $ISSUES-MAX } Issues wurden geladen. Weitere Issues finden Sie auf der Seite der Plattform.

repo-workflows-issue_work-choose-723dfd05 = Kein Issue passt zum Filter.

repo-workflows-issue_work-choose-7dce44e8 = Nach Nummer, Titel oder Label filtern

repo-workflows-issue_work-choose-aa3b1760 = Hier

repo-workflows-issue_work-choose-e8f4aaee = In einem neuen Worktree

repo-workflows-issue_work-issue-plan-0dd9c97f = { $branch } ist der Standardbranch. Die Arbeit an einem Issue benötigt einen eigenen Branch.

repo-workflows-issue_work-issue-plan-1f8c993f = { $remote } existiert. Ein Branch folgt ihm, { $where- }.

repo-workflows-issue_work-issue-plan-2bd468f0 = Das Repository enthält noch keine Commits. Zuerst etwas committen, um die Arbeit darauf aufzubauen.

repo-workflows-issue_work-issue-plan-38012887 = Ein neuer Branch ausgehend von { $said }, { $where- }.

repo-workflows-issue_work-issue-plan-8dbd7c09 = { $branch } ist bereits { $other-key-1 } zugeordnet. Beim Start wird der Branch stattdessen { $key } zugeordnet.

repo-workflows-issue_work-issue-plan-952a7458 = { $branch } ist hier bereits ausgecheckt und wird mit { $key } verknüpft.

repo-workflows-issue_work-issue-plan-ae567fc0 = in einem neuen Worktree, { $folder-open-1 }

repo-workflows-issue_work-issue-plan-b341ef66 = { $branch } ist bereits in { $folder-open-1 } ausgecheckt. Der Branch wird mit { $key } verknüpft und dieser Worktree geöffnet.

repo-workflows-issue_work-issue-plan-c9fafe01 = Das Arbeitsverzeichnis enthält Änderungen. Sie wechseln mit nach { $branch }, sofern sie nicht überschrieben würden. Andernfalls wird nichts ausgeführt.

repo-workflows-issue_work-issue-plan-cf90f159 = { $branch } existiert bereits. Die Arbeit wird dort unverändert fortgesetzt, { $where- }.

repo-workflows-issue_work-issue-plan-dbeefca3 = hier ausgecheckt

repo-workflows-issue_work-issue-row-91765782 = Issue { $issue-number-1 }: { $issue-title-2 }

repo-workflows-issue_work-issue-start-point-212218a3 = { $tracking }, wird zuerst gefetcht

repo-workflows-issue_work-issue-start-point-3c20246c = HEAD ({ $super-short-self-refs-head-1 }): Hier und auf dem Remote gibt es kein { $name } als Ausgangspunkt

repo-workflows-issue_work-issue-start-point-497395b3 = { $name } (kein Remote vorhanden)

repo-workflows-issue_work-issue-start-point-ceee14ee = { $tracking } (der Standardbranch des Remotes), wird zuerst gefetcht

repo-workflows-issue_work-issue-start-point-d844f1e9 = { $name } im lokalen Zustand ({ $remote } enthält kein { $name })

repo-workflows-issue_work-name-problem-4b6fdeb4 = Der Branch braucht einen Namen

repo-workflows-issue_work-name-problem-4be6bde2 = Ein Branchname darf kein { $c } enthalten

repo-workflows-issue_work-name-problem-5e1d1dfc = Ein Branchname darf nicht mit - beginnen

repo-workflows-issue_work-name-problem-64d737b0 = Kein Abschnitt eines Branchnamens (zwischen /) darf mit .lock enden

repo-workflows-issue_work-name-problem-72447597 = Ein Branchname darf kein { $two } enthalten

repo-workflows-issue_work-name-problem-7b27dd13 = { $name } ist ein von Git reservierter Name

repo-workflows-issue_work-name-problem-8e7a7abd = Ein Branchname darf nicht mit . enden

repo-workflows-issue_work-name-problem-b19e092f = Kein Abschnitt eines Branchnamens (zwischen /) darf mit . beginnen

repo-workflows-issue_work-name-problem-b9131e75 = Ein Branchname darf keine Steuerzeichen enthalten

repo-workflows-issue_work-name-problem-e2570ee3 = Ein Branchname darf weder mit / beginnen oder enden noch // enthalten

repo-workflows-issue_work-open-issue-picker-2a41bd2f = Keine Repository-Plattform erkannt. Oben Jira oder Linear auswählen oder das Remote konfigurieren.

repo-workflows-issue_work-open-issue-picker-3a89bbe3 = Repository-Plattform

repo-workflows-issue_work-refresh-issue-0c6fd7a6 = Die Repository-Plattform ist nicht verfügbar

repo-workflows-issue_work-refresh-issue-8336fbfe = { $label } fehlgeschlagen: { $e }

repo-workflows-issue_work-refresh-issue-a0e9ad2a = { $label }: Auf { $busy } warten

repo-workflows-issue_work-show-19766ed6 = Abbrechen

repo-workflows-issue_work-show-3a89bbe3 = Repository-Plattform

repo-workflows-issue_work-show-6a7da0d2 = Arbeit beginnen

repo-workflows-issue_work-show-8bd32aa3 = Die Ihnen zugewiesenen offenen Issues auf { $picker-forge-1 }

repo-workflows-issue_work-show-b32a9827 = Anfrage an { $picker-forge-1 } …

repo-workflows-issue_work-show-b37705f6 = Zuerst warten, bis { $busy } abgeschlossen ist

repo-workflows-issue_work-show-b6db3846 = Profil geändert. Diese Quelle neu laden, um zu beginnen.

repo-workflows-issue_work-show-d3965ab6 = Ihnen ist kein offenes Issue dieses Repositorys zugewiesen.

repo-workflows-issue_work-show-d8b8392e = Erneut versuchen

repo-workflows-issue_work-show-fd3baaea = Arbeit mit einem Issue beginnen

repo-workflows-issue_work-start-issue-563140a4 = Tracker-Konto geändert. Issue-Quelle neu laden.

repo-workflows-issue_work-start-issue-c14dd822 = Arbeit an { $issue-key-1 } beginnen: { $why }

repo-workflows-issue_work-start-issue-e2b80c1b = Arbeit an { $issue-key-1 } beginnen: Auf { $busy } warten

repo-workflows-issue_work-start-ops-4c405286 = { $remote } konnte nicht gefetcht werden. Daher beginnt die Arbeit bei { $remote }/{ $base } vom letzten Fetch.

repo-workflows-issue_work-take-issue-read-bbcbbb59 = Die Issues der Branches konnten nicht gelesen werden: { $e }

repo-workflows-issue_work-unlink-issue-6df1900e = Entfernen der Issue-Zuordnung von { $branch } fehlgeschlagen: { $e }

repo-workflows-issue_work-what-3e69f7c1 = Aufgabenliste

repo-workflows-issue_work-what-68e64f0f = Akzeptanzkriterien

repo-workflows-issue_work-what-734b09bc = Beschreibung

repo-workflows-lfs_locks-caught-ff2cd16f = Interner Fehler: { $said }

repo-workflows-lfs_locks-filters-53518a4c = Nach Pfad oder Inhaber filtern

repo-workflows-lfs_locks-filters-5da4f78e = Nur meine

repo-workflows-lfs_locks-header-2e532790 = Sperren erneut lesen

repo-workflows-lfs_locks-header-961b2a04 = Schließen (Esc)

repo-workflows-lfs_locks-header-b815cb09 = LFS-Sperren

repo-workflows-lfs_locks-list-04f767bd = { $a } und { $b }

repo-workflows-lfs_locks-list-e6b59dc4 = { $a }, { $b } und { $rest-len-1 } weitere

repo-workflows-lfs_locks-lock-banners-03d23d29 = LFS-Sperren …

repo-workflows-lfs_locks-lock-banners-11ae36a8 = { $if-one-owner-is-empty-someone-el-1 } hält die Sperre von { $one-path-2 }. Ihr Push wird verweigert, bis die Sperre freigegeben wurde.

repo-workflows-lfs_locks-lock-banners-2737026b = Von anderen gesperrt: { $list-names-1 }. Ihr Push dieser Dateien wird verweigert, bis die Sperren freigegeben wurden.

repo-workflows-lfs_locks-lock-banners-3233cb04 = Jemand anderes

repo-workflows-lfs_locks-lock-banners-379e383e = Alle entsperren

repo-workflows-lfs_locks-lock-banners-4ac709aa = Entsperren

repo-workflows-lfs_locks-lock-banners-4caac7c4 = Sie haben { $many-len-1 } Dateien gepusht und halten deren Sperren weiterhin. Freigeben, damit andere die Dateien ändern können?

repo-workflows-lfs_locks-lock-banners-4fa40890 = Sie haben { $one-path-1 } gepusht und halten die Sperre weiterhin. Freigeben, damit andere die Datei ändern können?

repo-workflows-lfs_locks-lock-banners-6df5e70f = Gesperrt lassen

repo-workflows-lfs_locks-lock-banners-82b7be09 = { $one } kann gesperrt werden und ist derzeit frei. Sperren Sie die Datei, damit niemand anderes während Ihrer Arbeit Änderungen daran pusht.

repo-workflows-lfs_locks-lock-banners-942087cc = Erneut versuchen

repo-workflows-lfs_locks-lock-banners-96ed4625 = { $many-len-1 } geänderte Dateien können gesperrt werden und sind derzeit frei. Sperren Sie sie, damit niemand anderes während Ihrer Arbeit Änderungen daran pusht.

repo-workflows-lfs_locks-lock-banners-ac5bb7e1 = Alle sperren

repo-workflows-lfs_locks-lock-banners-bb83044b = Gesperrt lassen

repo-workflows-lfs_locks-lock-banners-db44b8db = Sperren

repo-workflows-lfs_locks-lock-banners-f8368582 = Die LFS-Sperren konnten nicht gelesen werden. Daher ist unbekannt, wer Ihre geänderten Dateien gesperrt hat.

repo-workflows-lfs_locks-lock-job-35b33376 = { $label }: abgeschlossen. .gitattributes committen, um die Änderung zu teilen.

repo-workflows-lfs_locks-lock-job-36f5baee = { $label }: abgeschlossen

repo-workflows-lfs_locks-lock-job-8336fbfe = { $label } fehlgeschlagen: { $e }

repo-workflows-lfs_locks-lock-label-398e9b81 = Von { $who } gesperrt { $git-relative-time-at-now-1 }

repo-workflows-lfs_locks-lock-label-6329feb6 = Von { $who } gesperrt

repo-workflows-lfs_locks-lock-list-1a9f2707 = Keine der Sperren gehört Ihnen.

repo-workflows-lfs_locks-lock-list-3ef08b56 = Keine Sperre passt zum Filter.

repo-workflows-lfs_locks-lock-list-5619b99f = Niemand hält eine Sperre.

repo-workflows-lfs_locks-lock-list-e1890cf8 = Sperren werden gelesen …

repo-workflows-lfs_locks-lock-menu-items-03d23d29 = LFS-Sperren …

repo-workflows-lfs_locks-lock-menu-items-052a679c = Die LFS-Sperre freigeben, damit andere Änderungen an dieser Datei pushen können

repo-workflows-lfs_locks-lock-menu-items-0c28d273 = Der Server kann den Sperrinhaber nicht bestimmen. Die Freigabe schlägt fehl, wenn die Sperre nicht Ihnen gehört.

repo-workflows-lfs_locks-lock-menu-items-3f09bd22 = Die LFS-Sperre übernehmen, damit niemand anderes Änderungen an dieser Datei pushen kann

repo-workflows-lfs_locks-lock-menu-items-4ac709aa = Entsperren

repo-workflows-lfs_locks-lock-menu-items-6526301c = git-lfs gibt die Sperre einer geänderten Datei nicht frei. Die Änderungen zuerst committen.

repo-workflows-lfs_locks-lock-menu-items-b8fc75d5 = Ein Vorgang für LFS-Sperren läuft

repo-workflows-lfs_locks-lock-menu-items-bafa9d65 = Jemand anderes hält die Sperre dieser Datei

repo-workflows-lfs_locks-lock-menu-items-bea7de48 = Die Änderungen der Datei vor dem Entsperren committen

repo-workflows-lfs_locks-lock-menu-items-db44b8db = Sperren

repo-workflows-lfs_locks-locks-read-05347473 = Die LFS-Sperren konnten nicht gelesen werden: { $e }

repo-workflows-lfs_locks-patterns-119fe07d = git lfs track --lockable: Speichert die Dateien in LFS und schreibt das Muster in .gitattributes

repo-workflows-lfs_locks-patterns-6105d5e3 = Sperrbar machen

repo-workflows-lfs_locks-patterns-abf43528 = Sperrbare Dateien

repo-workflows-lfs_locks-patterns-ddfb1d6f = Dateien, die diesen Mustern entsprechen, können gesperrt werden. Bis dahin hält git-lfs sie schreibgeschützt. .gitattributes committen, um ein neues Muster zu teilen.

repo-workflows-lfs_locks-patterns-ed7c75cc = Kein Muster in der .gitattributes im Hauptverzeichnis ist sperrbar.

repo-workflows-lfs_locks-unlock-buttons-19766ed6 = Abbrechen

repo-workflows-lfs_locks-unlock-buttons-24110cc4 = { $n } entsperren

repo-workflows-lfs_locks-unlock-buttons-2978e628 = der Inhaber verliert die Sperre

repo-workflows-lfs_locks-unlock-buttons-4ac709aa = Entsperren

repo-workflows-lfs_locks-unlock-buttons-6021bccb = Entsperren von { $n } Dateien erzwingen …

repo-workflows-lfs_locks-unlock-buttons-62509394 = die Inhaber verlieren ihre Sperren

repo-workflows-lfs_locks-unlock-buttons-97edf7d6 = deren Inhaber

repo-workflows-lfs_locks-unlock-buttons-9971cec2 = Den Inhabern { $n } Sperren entziehen

repo-workflows-lfs_locks-unlock-buttons-b57b0be8 = { $owner } die Sperre entziehen

repo-workflows-lfs_locks-unlock-buttons-c214395e = Entzieht dem Inhaber die Sperre. Nur Administratoren dürfen dies. Der Inhaber wird nicht benachrichtigt und seine ungepushten Änderungen können mit den Änderungen anderer kollidieren.

repo-workflows-lfs_locks-unlock-buttons-ccf3d9ea = Alle auswählen

repo-workflows-lfs_locks-unlock-buttons-e213bc66 = Erneut klicken, um die Freigabe zu erzwingen: { $who }.

repo-workflows-lfs_locks-unlock-buttons-e2d5673c = Keine auswählen

repo-workflows-lfs_locks-unlock-buttons-f16646da = Entsperren erzwingen …

repo-workflows-lfs_storage-caught-ff2cd16f = Interner Fehler: { $said }

repo-workflows-lfs_storage-checkout-1716d186 = Nur Zeiger ({ $missing-len-1 })

repo-workflows-lfs_storage-checkout-4cd9a422 = Führt git lfs pull für jede Datei aus, unabhängig von den folgenden Ordnereinstellungen

repo-workflows-lfs_storage-checkout-5f0a6ba0 = Alle herunterladen ({ $paths-len-1 })

repo-workflows-lfs_storage-checkout-683768ab = und { $missing-len-listed-1 } weitere

repo-workflows-lfs_storage-checkout-87bcf309 = git lfs pull, soweit die folgenden Ordnereinstellungen dies zulassen ({ $e })

repo-workflows-lfs_storage-checkout-a5d7a59b = { $n } Dateien in LFS, alle heruntergeladen.

repo-workflows-lfs_storage-checkout-b627e471 = Keine Datei dieses Checkouts wird in LFS gespeichert.

repo-workflows-lfs_storage-checkout-d6eafe82 = Herunterladen

repo-workflows-lfs_storage-checkout-f4b00836 = LFS-Dateien werden aufgelistet …

repo-workflows-lfs_storage-checkout-f5a54014 = Eine Datei in LFS, heruntergeladen.

repo-workflows-lfs_storage-download-action-43432746 = „{ $bad }“ beginnt oder endet mit einem Leerzeichen, das git-lfs entfernt. LFS: Pull verwenden, um die Datei herunterzuladen.

repo-workflows-lfs_storage-download-action-e9f327fc = { $bad } enthält ein Komma, das git-lfs als zwei Muster liest. LFS: Pull verwenden, um die Datei herunterzuladen.

repo-workflows-lfs_storage-fetch-only-3f558a4c = LFS ruft jetzt nur { $names-join-1 } ab

repo-workflows-lfs_storage-fetch-only-e3be5d1f = Die LFS-Abrufauswahl konnte nicht gesetzt werden: { $e }

repo-workflows-lfs_storage-filters-49e74a0b = Abweichend von der .lfsconfig des Repositorys: { $names-join-1 }

repo-workflows-lfs_storage-filters-72d4ac36 = Die LFS-Abrufauswahl konnte nicht gelesen werden: { $e }

repo-workflows-lfs_storage-filters-8758c8bb = Nur diese Ordner

repo-workflows-lfs_storage-filters-9777b8b2 = Bei Fetch, Pull und Checkout lädt git-lfs nur eingeschlossene Ordner (alle, wenn keiner angegeben ist), ohne die ausgeschlossenen Ordner. Die Einstellung gilt nur für dieses Repository.

repo-workflows-lfs_storage-filters-9fd728c6 = Hinzufügen

repo-workflows-lfs_storage-filters-c621d797 = Die .lfsconfig des Repositorys legt fest: { $names-join-1 }

repo-workflows-lfs_storage-filters-cd8bf912 = Nie diese Ordner

repo-workflows-lfs_storage-filters-dc013b99 = { $name } entfernen

repo-workflows-lfs_storage-header-59ef044f = LFS-Speicher

repo-workflows-lfs_storage-header-961b2a04 = Schließen (Esc)

repo-workflows-lfs_storage-header-d8cb92fa = Erneut lesen

repo-workflows-lfs_storage-list-lfs-files-8d1bad49 = Die LFS-Dateien konnten nicht aufgelistet werden: { $e }

repo-workflows-lfs_storage-pointer-mark-efdc14b1 = Hier liegt nur der LFS-Zeiger vor (nicht heruntergeladen). Rechtsklick → LFS: Herunterladen.

repo-workflows-lfs_storage-prune-19766ed6 = Abbrechen

repo-workflows-lfs_storage-prune-30967fda = Ein Objekt

repo-workflows-lfs_storage-prune-407859ca = Erneut klicken, um { $objects } zu löschen

repo-workflows-lfs_storage-prune-5fc3fdf9 = { $objects } löschen ({ $human-size-plan-bytes-1 })

repo-workflows-lfs_storage-prune-659493e2 = Nichts zu bereinigen.

repo-workflows-lfs_storage-prune-8bfaae7c = Die Bereinigung löscht lokale LFS-Objekte, die weder aktuelle Commits noch der Checkout oder ungepushte Arbeit benötigen. Sie können erneut heruntergeladen werden, solange das Remote sie bereithält.

repo-workflows-lfs_storage-prune-8f8978b1 = { $plan-objects-1 } Objekte

repo-workflows-lfs_storage-prune-ebe86463 = Freigebbaren Speicher ermitteln

repo-workflows-lfs_storage-prune-f0bcc399 = Freigebbarer Speicher wird ermittelt …

repo-workflows-lfs_storage-set-filter-b41c110a = Speichern fehlgeschlagen: { $e }

repo-workflows-lfs_storage-storage-menu-items-1b8b17ee = git lfs track: Schreibt das Muster in .gitattributes. Bereits committete Dateien bleiben normale Git-Dateien, bis sie erneut geändert und vorgemerkt werden. .gitattributes committen, um das Muster zu teilen.

repo-workflows-lfs_storage-storage-menu-items-1de3e10c = LFS: Herunterladen

repo-workflows-lfs_storage-storage-menu-items-31a0cb12 = { $pattern } in LFS verfolgen

repo-workflows-lfs_storage-storage-menu-items-4f441605 = { $path } in LFS verfolgen

repo-workflows-lfs_storage-storage-menu-items-5ed965af = LFS: Nur { $dir } abrufen

repo-workflows-lfs_storage-storage-menu-items-e2cb4df2 = Das LFS-Objekt dieser Datei herunterladen und auschecken

repo-workflows-lfs_storage-storage-menu-items-ea226348 = git-lfs lädt bei Fetch, Pull und Checkout künftig nur diesen und weitere unter LFS: Speicher … gewählte Ordner herunter. Andere Dateien bleiben Zeiger. Die Einstellung gilt für dieses Repository.

repo-workflows-lfs_storage-storage-panel-6ce01b6d = Alte Objekte

repo-workflows-lfs_storage-storage-panel-a99e2b3d = Dieser Checkout

repo-workflows-lfs_storage-storage-panel-b05f298a = LFS-Abrufauswahl

repo-workflows-lfs_storage-usage-18b3c0d1 = Noch keine LFS-Objekte auf dem Datenträger

repo-workflows-lfs_storage-usage-9aea26d2 = { $human-size-u-bytes-1 } in einem LFS-Objekt auf dem Datenträger

repo-workflows-lfs_storage-usage-a5b8a0cd = { $human-size-u-bytes-1 } in { $n } LFS-Objekten auf dem Datenträger

repo-workflows-lfs_storage-usage-aca1a392 = LFS-Objekte werden vermessen …

repo-workflows-lfs_storage-usage-bc295532 = Die LFS-Objekte konnten nicht vermessen werden: { $e }

repo-workflows-line_dossier-line-dossier-view-82a9c46f = Committen

repo-workflows-owners-contents-3203b0df = Die Dateien des Repositorys konnten nicht aufgelistet werden: { $e }

repo-workflows-owners-contents-343bb384 = Eine Regel

repo-workflows-owners-contents-4adeeb91 = Abschnitte: { $sections-join-1 }

repo-workflows-owners-contents-4b99390e = In Ihrem Editor

repo-workflows-owners-contents-4d726f4a = { $n } Regeln

repo-workflows-owners-contents-64feb239 = VON DER PLATTFORM ANDERS GELESENE ZEILEN ({ $file-problems-len-1 })

repo-workflows-owners-contents-7d9eb7ac = Schließen

repo-workflows-owners-contents-8f8353f4 = Zeile { $p-line-1 }: { $p-text-2 }

repo-workflows-owners-contents-96978c8c = { $file-path-1 } bei HEAD, gelesen wie von { $file-flavor-name-2 } · { $rules }

repo-workflows-owners-contents-bd5f6b1d = Datei öffnen

repo-workflows-owners-contents-bd73a6b1 = Keine. Die Plattform liest jede Zeile wie geschrieben.

repo-workflows-owners-contents-c72a165d = Zuletzt nicht erneut lesbar ({ $e }). Angezeigt wird der zuvor gelesene Stand.

repo-workflows-owners-contents-e1e62224 = CODEOWNERS prüfen

repo-workflows-owners-owners-act-9cd4150e = Dieses Repository hat keine CODEOWNERS-Datei

repo-workflows-owners-owners-act-b11dc59d = Prüfung fehlgeschlagen

repo-workflows-owners-owners-summary-db29b096 = Aus { $path } bei HEAD ({ $rules-count-1 })

repo-workflows-owners-read-owners-cf60f83c = Lesen fehlgeschlagen

repo-workflows-owners-results-26e406d1 = Keine. Jede Datei hat einen Verantwortlichen.

repo-workflows-owners-results-683768ab = und { $lint-unowned-len-listed-1 } weitere

repo-workflows-owners-results-85d177f5 = MUSTER OHNE PASSENDE DATEI ({ $lint-unmatched-len-1 })

repo-workflows-owners-results-9c0433a9 = Keine. Jedes Muster passt zu einer Datei.

repo-workflows-owners-results-a6551c12 = Zeile { $n }: { $pattern }

repo-workflows-owners-results-a92df9d7 = DATEIEN OHNE VERANTWORTLICHE ({ $lint-unowned-len-1 } von { $lint-files-2 })

repo-workflows-owners-reviewers-for-d68c1cfb = { $base } ist hier kein Branch

repo-workflows-owners-reviewers-ui-796e0df8 = Reviews anhand von CODEOWNERS anfordern, sobald es kein Entwurf mehr ist

repo-workflows-owners-reviewers-ui-7a8a3ca3 = Ein Team. Die Plattform fragt seine Mitglieder an.

repo-workflows-owners-reviewers-ui-d8e1731e = Ein Konto

repo-workflows-owners-reviewers-ui-da35c68e = Reviews anhand von CODEOWNERS anfordern

repo-workflows-owners-rules-343bb384 = Eine Regel

repo-workflows-owners-rules-4d726f4a = { $n } Regeln

repo-workflows-owners-summary-text-3ff108bf = { $unowned } Dateien ohne Verantwortlichen

repo-workflows-owners-summary-text-4339bb43 = Kein Verantwortlicher

repo-workflows-owners-summary-text-6a5cbe9f = Eine Datei ohne Verantwortlichen

repo-workflows-owners-summary-text-9655d0de = Hier hat keine Datei einen Verantwortlichen

repo-workflows-owners-summary-text-a198627d = Benötigt { $codeowners-and-list-short-names--1 }

repo-workflows-package-buttons-0e916101 = Aktualisieren

repo-workflows-package-buttons-7d9eb7ac = Schließen

repo-workflows-package-buttons-8a24ec7c = Listet die Pakete und ihre Dateien nach einer Änderung erneut auf

repo-workflows-package-contents-59de121d = Paket

repo-workflows-package-contents-89bee303 = Die Dateien des Pakets konnten nicht aufgelistet werden: { $e }

repo-workflows-package-contents-8ed27415 = Hier wird nichts veröffentlicht: package.json ist privat oder alle Crates haben publish = false.

repo-workflows-package-contents-a2e9d64e = Aus { $pkg-ecosystem-tool-1 }, ausgeführt in { $if-pkg-dir-is-empty-the-reposito-2 }

repo-workflows-package-contents-b32a9827 = Anfrage an { $pkg-ecosystem-tool-1 } …

repo-workflows-package-contents-b60559a9 = Was ein Paket veröffentlicht

repo-workflows-package-contents-d5728cc3 = dem Hauptverzeichnis des Repositorys

repo-workflows-package-contents-e237505e = Die Pakete konnten nicht gefunden werden: { $e }

repo-workflows-package-more-683768ab = und { $len-shown-1 } weitere

repo-workflows-package-notes-adea78e6 = Nichts gefunden.

repo-workflows-package-report-ui-47ec594b = In { $p-version-1 }, fehlt hier

repo-workflows-package-report-ui-6c125757 = Neu seit { $p-version-1 }

repo-workflows-package-report-ui-74f3a93b = Vergleich fehlgeschlagen: { $e }

repo-workflows-package-report-ui-b2667be4 = VERGLICHEN MIT { $registry-to-uppercase-1 }

repo-workflows-package-report-ui-b413d4db = Anfrage an { $registry } …

repo-workflows-package-report-ui-b47c6e02 = Noch nie auf { $registry } veröffentlicht.

repo-workflows-package-report-ui-d4826304 = DATEIEN ({ $report-files-len-1 })

repo-workflows-package-report-ui-ef333981 = Dieselben Dateien wie { $p-version-1 } auf { $registry }.

repo-workflows-package-report-ui-fa5b38db = ÜBLICHERWEISE ENTHALTEN, FEHLT HIER

repo-workflows-package-report-ui-fefc6039 = MÖGLICHERWEISE VERSEHENTLICH ENTHALTEN

repo-workflows-parallel_bisect-hash-link-63633913 = Diesen Commit anzeigen

repo-workflows-parallel_bisect-parallel-buttons-19766ed6 = Abbrechen

repo-workflows-parallel_bisect-parallel-buttons-7d9eb7ac = Schließen

repo-workflows-parallel_bisect-parallel-buttons-e5e3ecc5 = Warten Sie, bis der laufende Vorgang abgeschlossen ist

repo-workflows-parallel_bisect-parallel-link-63633913 = Diesen Commit anzeigen

repo-workflows-parallel_bisect-parallel-outcome-683768ab = und { $hashes-len-max-named-1 } weitere

repo-workflows-parallel_bisect-parallel-outcome-8b3812c0 = Erster fehlerhafter Commit:

repo-workflows-pr_review-verdict-buttons-eb1a70e3 = Verwerfen

repo-workflows-pr_view-dismiss-box-19766ed6 = Abbrechen

repo-workflows-pr_view-entry-header-48845bff = Schließen

repo-workflows-push-dismiss-48845bff = Schließen

repo-workflows-push-rejected-683768ab = und { $r-behind-named-1 } weitere

repo-workflows-reflog-recover-items-4c49fdb6 = Cherry-Pick

repo-workflows-reflog-reflog-menu-items-916e34d4 = Im Graphen anzeigen

repo-workflows-reflog-scanning-19766ed6 = Abbrechen

repo-workflows-rehearse-rehearse-window-cae7d57b = Stoppen

repo-workflows-release-close-button-7d9eb7ac = Schließen

repo-workflows-repository_tools-act-08cd49af = Isolierten Vorschlag schließen

repo-workflows-repository_tools-act-27cc6eb8 = Die Subtree-Vorbereitung verwendete ein vorheriges Profil oder eine andere Anmeldung. Erneut vorbereiten.

repo-workflows-repository_tools-act-346c061a = Akzeptierte isolierte Wiederanwendung rückgängig machen

repo-workflows-repository_tools-act-494336ed = Exakten Subtree-Split veröffentlichen

repo-workflows-repository_tools-act-519202af = Die Vorbereitung verwendete ein vorheriges Profil oder eine andere Anmeldung. Bereiten Sie sie erneut vor.

repo-workflows-repository_tools-act-551afaa9 = Eine Änderungsliste zurückholen

repo-workflows-repository_tools-act-5bd57c48 = In isolierten Worktrees erneut anwenden

repo-workflows-repository_tools-act-63403bc6 = Virtuellen Branch-Vorgang rückgängig machen

repo-workflows-repository_tools-act-65d162a8 = Isolierte Wiederanwendung fortsetzen

repo-workflows-repository_tools-act-6dc9a943 = Wählen Sie einen gespeicherten Durchgang für die inverse Vorschau

repo-workflows-repository_tools-act-7f694b62 = Ausgangseintrag entfernen

repo-workflows-repository_tools-act-88c16034 = Subtree-Vorgang

repo-workflows-repository_tools-act-901dccbe = Eigentumszuordnung virtueller Branches speichern

repo-workflows-repository_tools-act-a3cbb206 = Änderungsliste speichern

repo-workflows-repository_tools-act-b4e12052 = Durchgang zurückgenommen. Vorgemerkte Inhalte bleiben erhalten. Rückgängig über „Frühere Versionen“: { $id-draft-reference-1 }. { $id-evidence-2 }

repo-workflows-repository_tools-act-b64c091a = Isolierte Wiederanwendung akzeptieren

repo-workflows-repository_tools-act-b967f21f = Exakten Push einreihen

repo-workflows-repository_tools-act-bea96408 = Eine Änderungsliste ablegen

repo-workflows-repository_tools-act-c08c65bd = Exakte Wiederanwendungsbelege abgleichen

repo-workflows-repository_tools-act-c1eec3a9 = Eine Änderungsliste committen

repo-workflows-repository_tools-act-ccd2fdda = Verbindung geändert. Öffnen Sie den Ausgang erneut.

repo-workflows-repository_tools-act-ce061d46 = Materialisierung virtueller Branches ändern

repo-workflows-repository_tools-act-d021ec81 = Profil oder Anmeldung des vorbereiteten Subtrees geändert. Erneut vorbereiten und freigeben.

repo-workflows-repository_tools-act-d64d1e25 = Gehen Sie online und warten Sie auf den aktuellen Vorgang, bevor Sie einen Ausgangs-Push vorbereiten

repo-workflows-repository_tools-act-de986fa8 = Hunks eines virtuellen Branches committen

repo-workflows-repository_tools-act-e3e83f28 = Quellbranch ist aus dem geprüften Graphen verschwunden

repo-workflows-repository_tools-act-fc328635 = Einen gespeicherten Durchgang zurücknehmen

repo-workflows-repository_tools-deferred-badge-8daa08a5 = Konflikt wartet: { $p }

repo-workflows-repository_tools-deferred-badge-a5eafc03 = Isolierte Umschreibung; ursprünglicher Branch bleibt bis zur geschützten Annahme erhalten

repo-workflows-repository_tools-dependency-job-57895189 = Veröffentlichungsabhängigkeiten der Submodule prüfen

repo-workflows-repository_tools-dependency-job-80d8056a = Abhängigkeitsvorgang abgeschlossen

repo-workflows-repository_tools-dependency-job-96731680 = Übergeordnete Veröffentlichung gestoppt. Prüfen Sie die Ergebnisse pro Repository.

repo-workflows-repository_tools-dependency-job-a7b1dc6d = Abhängigkeiten abgeschlossen, aber das übergeordnete Repository, Profil oder die Anmeldung hat sich geändert. Bereiten Sie den Push erneut vor.

repo-workflows-repository_tools-dependency-job-f48f9c31 = Abhängigkeiten vor der Prüfung des übergeordneten Pushes veröffentlichen

repo-workflows-repository_tools-dependency-job-f7c1dc97 = Abhängigkeitsprüfung hat Hindernisse gefunden. Prüfen Sie jedes Submodul.

repo-workflows-repository_tools-force-01549c1f = Force-Push über die Push-Route

repo-workflows-repository_tools-force-15d2a4c4 = Das Ziel steht derzeit auf { $p-remote-commit-1 }. Dies kann Commits anderer Mitwirkender entfernen. Exakte URL und erwarteter Zielcommit sind fixiert. Eine Lease verhindert konkurrierende Pushes.

repo-workflows-repository_tools-force-1d53000c = Ich habe das Ziel geprüft und akzeptiere das Ersetzen seines Verlaufs

repo-workflows-repository_tools-force-884b8ad2 = { $p-destination-1 } auf { $p-remote-2 } ({ $crate-git-redact-p-url-none-3 }) durch { $p-branch-4 } bei { $p-commit-5 } ersetzen?

repo-workflows-repository_tools-force-b7f756d6 = Force-Push mit ausdrücklicher Lease

repo-workflows-repository_tools-label-137ebba9 = { $direction } { $route-remote-1 }/{ $route-branch-2 } · { $counts }

repo-workflows-repository_tools-label-2d5fbffb = Anzahl unbekannt (zuerst abrufen)

repo-workflows-repository_tools-label-87dc28eb = { $direction }: keine Route eingerichtet

repo-workflows-repository_tools-label-f25a8952 = { $a } voraus · { $b } zurück

repo-workflows-repository_tools-object-view-14a9c8ce = Loses Objekt: { $n } komprimierte Bytes

repo-workflows-repository_tools-object-view-3e15593a = Objekt-ID oder Dateiversion, etwa HEAD:src/main.rs. Die Prüfung lädt nie fehlende Objekte herunter. Diese Garantie erfordert Git 2.45 oder neuer.

repo-workflows-repository_tools-object-view-44de072f = { $o-kind-1 } · { $o-size-2 } Bytes entpackt · { $o-disk-3 } Bytes auf Datenträger

repo-workflows-repository_tools-object-view-56daa2be = Baumeinträge / verknüpfte Objekte

repo-workflows-repository_tools-object-view-62a6da87 = Objekt

repo-workflows-repository_tools-object-view-7b47361a = Basis

repo-workflows-repository_tools-object-view-8efd32c4 = Wie ein Objekt gespeichert ist

repo-workflows-repository_tools-object-view-9147a920 = { $data-format-1 } · { $data-objects-display-2 } · aufgelöst aus { $data-query-3 }

repo-workflows-repository_tools-object-view-9d4281bc = Bei Deltas beschreibt die Darstellungsgröße die Delta-Anweisungen. Die entpackte Objektgröße steht oben.

repo-workflows-repository_tools-object-view-b39d687c = Delta-Basis: { $base }

repo-workflows-repository_tools-object-view-ddc47a7a = Git-Verzeichnis: { $data-git-dir-1 }
    Gemeinsames Verzeichnis: { $data-common-dir-2 }

repo-workflows-repository_tools-object-view-e0723a86 = Prüfen

repo-workflows-repository_tools-object-view-f4de0102 = { $if-i-0-object-else-base-1 }: { $row-oid-2 } · { $row-kind-3 } · gepackt { $row-disk-4 } B · Darstellung { $row-size-5 } B · Offset { $row-offset-6 } · Tiefe { $row-depth-7 }

repo-workflows-repository_tools-patch-eef91a52 = Keine Unterschiede

repo-workflows-repository_tools-picker-f523255b = Bekannte Branches

repo-workflows-repository_tools-prepare-branch-review-2685a3d8 = Push-Vorbereitung abgebrochen

repo-workflows-repository_tools-prepare-branch-review-77a313b9 = Ungültige ausgewählte Push-Fortsetzung

repo-workflows-repository_tools-prepare-branch-review-a89fdaff = Repository, Profil oder Anmeldung geändert. Wählen Sie die Push-Aktion erneut.

repo-workflows-repository_tools-publish-group-1f0d64fa = Veröffentlichung unvollständig: { $failed } Ziele fehlgeschlagen. Prüfen Sie die Ergebnisse jedes Ziels.

repo-workflows-repository_tools-publish-group-3cc9a2c6 = Gruppe veröffentlichen

repo-workflows-repository_tools-publish-group-449d962a = Alle Ziele haben den vorbereiteten Commit veröffentlicht

repo-workflows-repository_tools-refs-list-c2a1a3d6 = Objekt kopieren

repo-workflows-repository_tools-refs-list-edec79bb = Objekt prüfen

repo-workflows-repository_tools-reftable-view-4101df87 = { $storage-git-1 } · aktuelle Speicherung: { $storage-format-2 }

repo-workflows-repository_tools-reftable-view-87ed192f = Alle anderen Schreibzugriffe sind beendet und ich habe eine Sicherung

repo-workflows-repository_tools-reftable-view-891eab64 = Die Migration behält Referenzen und Reflogs bei, verweigert verknüpfte Worktrees und prüft beides anschließend. Stoppen Sie alle anderen Programme, die ins Repository schreiben. Sichern Sie das Repository vor dem Fortfahren.

repo-workflows-repository_tools-reftable-view-c2782e54 = Referenzspeicherung und geführte Migration

repo-workflows-repository_tools-reftable-view-d3ba0aeb = Nach { $target } migrieren und prüfen

repo-workflows-repository_tools-reftable-view-ea4f0afa = Ich habe die Kompatibilität meiner installierten Werkzeuge mit dem Zielformat geprüft

repo-workflows-repository_tools-reftable-view-faca3ce1 = Reftable benötigt Git 2.45 oder neuer und kompatible Drittwerkzeuge. Manche IDEs, eingebettete Git-Bibliotheken, Sicherungsprogramme und ältere Clients unterstützen nur Dateien. Prüfen Sie vor der Migration jedes Werkzeug, das auf dieses Repository zugreift. Dateien bieten die breiteste Kompatibilität.

repo-workflows-repository_tools-remote-config-preview-bba0304c = Remote-Vorschau: { $e }

repo-workflows-repository_tools-replacements-view-0a69dfc6 = Wirksamer Verlauf

repo-workflows-repository_tools-replacements-view-1714de4d = Alte Grafts in Replace-Referenzen umwandeln

repo-workflows-repository_tools-replacements-view-33ea34c6 = Betroffener Commit { $r-original-1 } → { $r-effective-2 }

repo-workflows-repository_tools-replacements-view-4953f897 = Verbindung entfernen

repo-workflows-repository_tools-replacements-view-4cff5861 = Ursprünglicher Verlauf (--no-replace-objects)

repo-workflows-repository_tools-replacements-view-5fcfdd61 = Wirksame Eltern: { $r-parents-1 }

repo-workflows-repository_tools-replacements-view-7916bff6 = Temporären Graft erstellen

repo-workflows-repository_tools-replacements-view-7b8c9687 = Alte Datei info/grafts (umgewandelte Verbindungen bleiben hier sichtbar und entfernbar):

repo-workflows-repository_tools-replacements-view-84da95ea = Commit:

repo-workflows-repository_tools-replacements-view-9c43f2c2 = Ich habe diese lokalen Verlaufsänderungen geprüft

repo-workflows-repository_tools-replacements-view-b022884c = Neuer Elterncommit:

repo-workflows-repository_tools-replacements-view-e51254f0 = Importierte Verläufe verbinden: Ersetzt die Eltern dieses Commits durch einen Commit aus dem anderen Verlauf. Fügen Sie vollständige Commit-IDs ein. Zyklische Verbindungen und vorhandene Ersetzungen werden abgelehnt.

repo-workflows-repository_tools-replacements-view-f5b874e5 = Replace-Referenzen und temporäre Verlaufsverbindungen

repo-workflows-repository_tools-replacements-view-fcc3aff3 = Replace-Referenzen betreffen diese Commits und ihre Nachfahren. Die Verbindungen unten sind lokal und temporär. Das Entfernen der Replace-Referenz stellt den ursprünglichen Verlauf wieder her.

repo-workflows-repository_tools-repository-tool-mutation-988356ee = { $busy } läuft noch

repo-workflows-repository_tools-repository-tools-act-07fb4089 = Klon unabhängig machen

repo-workflows-repository_tools-repository-tools-act-38bb44e4 = Aggregierter Endpunkt oder Sitzung fehlt

repo-workflows-repository_tools-repository-tools-act-3c1c7a3a = Veröffentlichungsgruppe speichern

repo-workflows-repository_tools-repository-tools-act-4e161156 = Remote-Konfiguration speichern

repo-workflows-repository_tools-repository-tools-act-66f98362 = Submodulzeiger auflösen

repo-workflows-repository_tools-repository-tools-act-69a2d9db = Replace-Referenz entfernen

repo-workflows-repository_tools-repository-tools-act-82d14313 = Pull- und Push-Routen speichern

repo-workflows-repository_tools-repository-tools-act-888e8b45 = Referenzspeicherung migrieren

repo-workflows-repository_tools-repository-tools-act-a7b0c90c = Importierte Verläufe vorübergehend verbinden

repo-workflows-repository_tools-repository-tools-act-ad133eb8 = Alte Grafts in entfernbare Replace-Referenzen umwandeln

repo-workflows-repository_tools-repository-tools-act-baa3959f = Branchstand fehlt

repo-workflows-repository_tools-repository-tools-act-e7cf34fe = Gespeicherter Durchgang fehlt

repo-workflows-repository_tools-review-prepared-push-88d2d1a8 = Ausgewählte Route pushen

repo-workflows-repository_tools-show-0256e5a0 = Rebase in isoliertem Worktree ausführen

repo-workflows-repository_tools-show-03dc5c4f = Abruf-Refspecs: +source:destination; ^source schließt einen Branch oder ein Präfix aus

repo-workflows-repository_tools-show-04245473 = Listen und Dateistände aktualisieren

repo-workflows-repository_tools-show-05519c89 = Bereits geprüfte identische Hunks bleiben über die Durchgänge dieser Sitzung verborgen. Geänderte Hunks erscheinen erneut. Markierungen bleiben lokal. Die Rücknahme prüft die Umkehrung gegen aktuelle Dateien und behält spätere nicht überlappende Änderungen sowie vorgemerkte Inhalte.

repo-workflows-repository_tools-show-057248ff = Auf mehreren Hosts veröffentlichen

repo-workflows-repository_tools-show-05e1813b = Gespeicherten Kandidaten öffnen

repo-workflows-repository_tools-show-05e305a1 = Ich habe andere Schreibprogramme für dieses Repository beendet und eine Sicherung erstellt

repo-workflows-repository_tools-show-05f3fddc = Stand als geprüft markieren und Prüfgrenze speichern

repo-workflows-repository_tools-show-06b251d5 = Gemeinsames Git-Verzeichnis: { $model-common-1 }

repo-workflows-repository_tools-show-07345f3f = Diesen exakten Subtree-Split senden

repo-workflows-repository_tools-show-07aa5dfe = Gemeinsame und geliehene Objekte

repo-workflows-repository_tools-show-089eb3ef = Pullen von

repo-workflows-repository_tools-show-0ae1bed9 = Branchentwurf speichern

repo-workflows-repository_tools-show-0b22ea73 = Gespeicherten Agent-Durchgang prüfen

repo-workflows-repository_tools-show-0d37e057 = Neue Basis:

repo-workflows-repository_tools-show-0e95ad58 = Submodul-Checkout: { $c-head-1 }

repo-workflows-repository_tools-show-107f87fc = Submodulabhängigkeiten vor dem übergeordneten Repository veröffentlichen

repo-workflows-repository_tools-show-111eda2a = Nur diesen Branch committen

repo-workflows-repository_tools-show-118694a9 = Abhängigkeiten veröffentlichen, dann übergeordneten Push prüfen

repo-workflows-repository_tools-show-11d962b1 = Spenderverzeichnis fehlt

repo-workflows-repository_tools-show-14d33bd0 = Nicht zugeordnet

repo-workflows-repository_tools-show-1560b7d2 = Submodul öffnen

repo-workflows-repository_tools-show-16312ab3 = Quelle lokal registrieren

repo-workflows-repository_tools-show-16d60e39 = Für einen übernommenen Ordner ohne Subtree-Verlauf: Abrufen, Unterschiede prüfen und dann ausdrücklich mit der abgerufenen Basis verbinden. Die Verbindung behält jedes Ordnerbyte und erfasst Unterschiede als lokale Änderungen. Nur künftige Upstream-Änderungen werden gemergt. Akzeptieren Sie die geprüfte Verbindung vor weiteren Aktualisierungen. Bestehende Git-Subtree-Importe können direkt „Aktualisierung vorbereiten“ verwenden.

repo-workflows-repository_tools-show-16f5b799 = Benannte Änderungslisten

repo-workflows-repository_tools-show-17c47592 = Nur diese Liste ablegen

repo-workflows-repository_tools-show-181a4047 = Sitzung { $aggregate-session-1 } · { $aggregate-saved-turns-2 } gespeicherte Durchgänge · feste Basis { $review-parent-3 } → Endpunkt { $review-turn-4 }. Spätere gespeicherte Durchgänge und aktuelle ungespeicherte Dateien sind ausgeschlossen.

repo-workflows-repository_tools-show-1919101c = Diesen Stack in getrennten Worktrees erneut anwenden

repo-workflows-repository_tools-show-19af9596 = Kandidaten von { $s-branch-1 } vergleichen

repo-workflows-repository_tools-show-19cca4e2 = Die Prüfung ruft Heads und Tags in temporäre App-Referenzen jedes Submoduls ab. Sie prüft den benannten Abrufendpunkt samt Gitlinks im ausgehenden Verlauf. Verschachtelte Abhängigkeiten werden zuerst veröffentlicht. Diese Pushes sind nicht atomar; Fehler stoppen das übergeordnete Repository.

repo-workflows-repository_tools-show-1a6e5e0d = Vorhandenen Ordner mit abgerufener Basis verbinden

repo-workflows-repository_tools-show-1b5dcef8 = Eingaben seit der Vorschau geändert. Erstellen Sie vor dem Speichern eine neue Vorschau.

repo-workflows-repository_tools-show-1bd5bcfa = Aktuelle Quelle: { $view-branch-1 }

repo-workflows-repository_tools-show-1c0954b2 = Listen verwalten ganze Dateien und haben unabhängige Nachrichtenentwürfe. Ein Commit verwendet den geprüften rohen Arbeitsstand, bleibt unsigniert und umgeht Hooks und Filter. Andere vorgemerkte Dateien bleiben vorgemerkt. Ablegen speichert vorgemerkte und Arbeitsänderungen getrennt. Zurückholen verweigert neuere Änderungen an betroffenen Pfaden.

repo-workflows-repository_tools-show-1e4f897e = Git-Verzeichnis: { $model-git-1 }

repo-workflows-repository_tools-show-1e9112fe = Registrierten Branch abrufen

repo-workflows-repository_tools-show-1f711390 = Aktualisierung in eigenem Worktree vorbereiten

repo-workflows-repository_tools-show-1f89e0a1 = Sichtbare ungeprüfte Hunks als geprüft markieren

repo-workflows-repository_tools-show-1fad1933 = bereits veröffentlicht

repo-workflows-repository_tools-show-1fbe68fb = Grenze der letzten Prüfung: { $aggregate-last-looked-as-deref-u-1 }

repo-workflows-repository_tools-show-202c8fbc = Einwilligung: Diesen einfachen Push online automatisch senden, sofern die Vorprüfung keine Befunde hat

repo-workflows-repository_tools-show-222bd382 = Ausgewählten Versand vorbereiten …

repo-workflows-repository_tools-show-2250c14b = Vorgemerkte Ordneränderungen

repo-workflows-repository_tools-show-2c00ff77 = Nur fehlgeschlagene Ziele wiederholen

repo-workflows-repository_tools-show-2d0adec7 = Objektspeicher: { $crate-remote-display-model-objec-1 }

repo-workflows-repository_tools-show-2ddef7c7 = Subtree-Register aktualisieren

repo-workflows-repository_tools-show-2e1d0813 = Ich habe diesen exakten eingereihten Commit, jeden Endpunkt und alle Vorprüfungsbefunde geprüft

repo-workflows-repository_tools-show-301d9809 = Diesen Klon unabhängig machen

repo-workflows-repository_tools-show-31a04bc4 = Erforderlich: { $oid }

repo-workflows-repository_tools-show-325b12c6 = Diesen festen gespeicherten Endpunkt erneut laden

repo-workflows-repository_tools-show-33463eb4 = Vorschlag schließen; Worktrees behalten

repo-workflows-repository_tools-show-34b10b8d = Rebase von { $name }

repo-workflows-repository_tools-show-36c73d64 = Exakter vorgeschlagener Branch-Patch

repo-workflows-repository_tools-show-3a7402e9 = Ich habe jedes Original → Kandidat-Objekt und den Kandidaten-Diff vor Annahme oder Rücknahme geprüft

repo-workflows-repository_tools-show-3acc29f8 = { $c-path-1 } · Basis { $c-base-as-deref-unwrap-or-none-2 } · unsere Seite { $c-ours-3 } · deren Seite { $c-theirs-4 }

repo-workflows-repository_tools-show-3cc9a2c6 = Gruppe veröffentlichen

repo-workflows-repository_tools-show-3e04e021 = Erneute Veröffentlichungsprüfung vorbereiten

repo-workflows-repository_tools-show-3e9abe63 = Rücknahme nur dieses Durchgangs ansehen …

repo-workflows-repository_tools-show-3f2768b0 = Geprüfte Abruf-URL: { $git-redact-n-fetch-url-none-1 } · { $if-n-published-already-published-2 }

repo-workflows-repository_tools-show-41251e4b = Unversionierte Dateien (nicht in der Split-Veröffentlichung enthalten):

repo-workflows-repository_tools-show-412ec52f = Geprüfte Eingaben des isolierten Restacks (Kindschichten verwenden das umgeschriebene Ergebnis ihres Vorgängers):

repo-workflows-repository_tools-show-41a222be = Nur diese Liste committen

repo-workflows-repository_tools-show-43a8ef7d = Bestehenden oder neuen übernommenen Ordner registrieren

repo-workflows-repository_tools-show-440174a7 = Akzeptierte Schichten rückgängig machen

repo-workflows-repository_tools-show-4498585c = Ergebnis: { $oid }

repo-workflows-repository_tools-show-44e05a73 = Bekannte Commits mit beiden Seiten (auch vollständige Commit-ID möglich):

repo-workflows-repository_tools-show-454ee70f = Commit-Nachrichtenentwurf für diese Liste

repo-workflows-repository_tools-show-4601e33d = Die Annahme benötigt einen vollständig sauberen ursprünglichen Checkout einschließlich unversionierter und ignorierter Dateien sowie den exakten ursprünglichen Branch und seine OID. Lösen Sie Konflikte und merken Sie die Lösungen im Kandidaten-Worktree vor. Ungewisse Annahmen werden ohne Wiederholung abgeglichen. Kein Kandidat wird automatisch bereinigt.

repo-workflows-repository_tools-show-469f9427 = Letzten virtuellen Vorgang rückgängig machen

repo-workflows-repository_tools-show-46aafc00 = Submodul zum Mergen öffnen

repo-workflows-repository_tools-show-4734d197 = Nur diesen Durchgang zurücknehmen

repo-workflows-repository_tools-show-4759498a = Erstellen

repo-workflows-repository_tools-show-4829ff52 = Quellbranch:

repo-workflows-repository_tools-show-49346e37 = Exakte Quelle: { $n-source-1 } · Push-URL: { $git-redact-n-push-url-none-2 }

repo-workflows-repository_tools-show-4c8748ea = Diesen exakten Commit über seine Push-Route einreihen

repo-workflows-repository_tools-show-4d07d136 = Nur der gespeicherte Commit wird gesendet. Umschreibungen, Zieländerungen sowie abgelehnte oder ungewisse Sendungen halten zur Prüfung an. Nach einem Absturz ist das Ergebnis ungewiss und wird nie automatisch wiederholt. Ausgangsdaten und geschützte Commits sind lokale App-Referenzen. Spiegeln Sie diese nicht mit Push.

repo-workflows-repository_tools-show-4e161156 = Remote-Konfiguration speichern

repo-workflows-repository_tools-show-4f480ac9 = Umkehrung von { $p-parent-1 } → { $p-turn-2 } gegen aktuelle Arbeitsdateien; { $p-paths-len-3 } betroffene Pfade

repo-workflows-repository_tools-show-507eaa08 = Original { $s-branch-1 }

repo-workflows-repository_tools-show-51768487 = Submodul-Konfliktgraph

repo-workflows-repository_tools-show-52b07a38 = Push-Ausgang

repo-workflows-repository_tools-show-5370d2e8 = Aktuelle Hunks und ihre Eigentümer:

repo-workflows-repository_tools-show-5471de9c = Exakte Annahmebelege abgleichen

repo-workflows-repository_tools-show-5673c401 = { $review-hunks-len-1 } Hunks · { $review-hunks-len-unreviewed-len-2 } geprüfte verborgen

repo-workflows-repository_tools-show-5d3cc263 = Alle Referenzen und Namensraumverläufe

repo-workflows-repository_tools-show-5dc87aef = Getrennte Pull- und Push-Routen

repo-workflows-repository_tools-show-5dcd35a3 = Jede Anzahl vergleicht diesen lokalen Branch mit der zuletzt abgerufenen Referenz der Route. Fehlende oder ausgeschlossene Tracking-Referenzen ergeben „unbekannt“. Die Änderung eines Push-Branches betrifft die gewöhnlichen Branch-Pushes von gitcraft. Git selbst verwendet weiterhin seine nativen push.default-/Refspec-Regeln.

repo-workflows-repository_tools-show-5dfdfed0 = Ich habe den exakten Split, alle Zielendpunkte, das gewählte Profil und Konto sowie die Vorprüfungsbefunde geprüft

repo-workflows-repository_tools-show-5e3184e8 = Vorschau bei 64000 Zeichen gekürzt.

repo-workflows-repository_tools-show-5f715b70 = Hunkanzeige gekürzt. Prüfen Sie vor der Zuordnung den Datei-Diff.

repo-workflows-repository_tools-show-5fa7aac5 = Bereit

repo-workflows-repository_tools-show-5fb1b07d = Gruppe { $p-group-1 } · { $p-commit-2 } → refs/heads/{ $p-branch-3 }

repo-workflows-repository_tools-show-616296e8 = { $s-branch-1 } nach { $s-old-base-2 } auf { $s-onto-3 } erneut anwenden

repo-workflows-repository_tools-show-625c67be = Exakte native Transaktionsbelege unterscheiden bestätigte Metadaten von ungewissen Bestätigungen. Diese Prüfung setzt keine Dateien zurück, entfernt keine Sperren und wiederholt keine Veröffentlichung.

repo-workflows-repository_tools-show-666eca6d = Quellremote:

repo-workflows-repository_tools-show-66dfd7a6 = Kandidaten-Patch prüfen

repo-workflows-repository_tools-show-67572031 = Remote: { $e-draft-name-1 } · alle Änderungen gelten nur für dieses Repository

repo-workflows-repository_tools-show-6b30bed0 = Jeder Hunk gehört zu einem echten Branch. Der ausgecheckte Branch bleibt an seinem Ankercommit und Gits echter Index bleibt erhalten. Virtuelle Branch-Commits verwenden geprüfte Rohinhalte, sind unsigniert und umgehen Hooks und Filter. Geänderte Zuordnungen und überlappende Aktivierungen benötigen eine ausdrückliche Lösung. Arbeitsordneränderungen benötigen natives macOS oder Linux.

repo-workflows-repository_tools-show-6bdc8428 = Kandidat noch unvollständig

repo-workflows-repository_tools-show-6c1991e9 = „Unabhängig machen“ kopiert ein vollständiges Pack für Referenzen, ursprüngliche Abstammung, Reflogs und Indexeinträge, prüft es in einem getrennten Repository und entfernt dann den Alternativverweis. Spender bleiben unverändert. Die ursprünglichen Alternativmetadaten bleiben als Wiederherstellungssicherung erhalten. Die Vorbereitung ist auf 128 MiB pro Pack begrenzt. Größere Repositorys können vor einer neuen Vorbereitung git repack -a verwenden.

repo-workflows-repository_tools-show-6c892519 = Nur die ersten 100 passenden Referenzen wurden geprüft. Fügen Sie eine vollständige Commit-ID für einen anderen Kandidaten ein.

repo-workflows-repository_tools-show-6f2680ab = { $b-name-1 } · { $if-b-active-active-else-inactive-2 } · { $b-owned-len-3 } zugeordnete Hunks

repo-workflows-repository_tools-show-71ba13e6 = Geprüfte unveränderte Hunks anzeigen

repo-workflows-repository_tools-show-72353618 = Zuordnungen und Prune-Löschungen anhand der Abruf-URL ansehen

repo-workflows-repository_tools-show-72456f4c = Anzeige bei 32000 Zeichen gekürzt. Öffnen Sie für mehr den Durchgangsvergleich.

repo-workflows-repository_tools-show-739ffe8f = Alles seit der letzten Prüfung · nur gespeicherte Durchgänge

repo-workflows-repository_tools-show-7432d850 = Lokale Änderungen prüfen

repo-workflows-repository_tools-show-7790f11b = Benannte Gruppe speichern

repo-workflows-repository_tools-show-77c3b92d = Keine gespeicherten Zustandsveröffentlichungen in diesem Worktree.

repo-workflows-repository_tools-show-78f32476 = Ausgewählten exakten Push senden

repo-workflows-repository_tools-show-798b0df6 = Zielbranch:

repo-workflows-repository_tools-show-79af8f6d = Neue Liste:

repo-workflows-repository_tools-show-7b868cc6 = Diese Umkehrung anwenden; bei überlappenden oder veralteten Änderungen stoppen. Ein Sicherheitsentwurf bewahrt die aktuellen Dateien.

repo-workflows-repository_tools-show-7bd990ec = Kandidat { $s-branch-1 }

repo-workflows-repository_tools-show-7c52fd3d = Vorschläge und Belege aktualisieren

repo-workflows-repository_tools-show-7d86e238 = Konflikte, die warten können

repo-workflows-repository_tools-show-7e381308 = Zusammengefasster Import

repo-workflows-repository_tools-show-7eb9930d = Veröffentlichungsbelege erneut laden

repo-workflows-repository_tools-show-7ffc5022 = Echter Branchstand: { $b-tip-1 }

repo-workflows-repository_tools-show-80c567ae = Alte Basis (nachfolgende Commits erneut anwenden):

repo-workflows-repository_tools-show-80ef5663 = Keine Vorprüfungsbefunde zu Identität, Signaturen, Nachrichten, Geheimnissen, großen Blobs oder Remote-Verlauf.

repo-workflows-repository_tools-show-83e94e8a = { $p-plan-commits-len-1 } ausgehende Commits

repo-workflows-repository_tools-show-8472b5d8 = Veröffentlicht: { $git-redact-output-none-1 }

repo-workflows-repository_tools-show-84e9b213 = Referenzen filtern:

repo-workflows-repository_tools-show-864a00e5 = Verleiht Objekte an { $borrower }

repo-workflows-repository_tools-show-88db1c04 = Veröffentlichung erforderlich

repo-workflows-repository_tools-show-89f4921c = Abruf-URLs (nur die erste wird abgerufen; eine pro Zeile)

repo-workflows-repository_tools-show-8bb0f122 = { $c-branch-1 } bei { $c-old-2 }

repo-workflows-repository_tools-show-8bbd26a7 = Veraltete Zuordnung { $h-key-8-1 } freigeben

repo-workflows-repository_tools-show-8cc484e4 = Alter Eintrag: Profil- oder Kontofreigabe fehlt

repo-workflows-repository_tools-show-8daa08a5 = Konflikt wartet: { $p }

repo-workflows-repository_tools-show-8dd5e5cc = Verlauf der Referenz / des Präfixes (leer = alle):

repo-workflows-repository_tools-show-8edc0123 = Ich habe jedes Ziel geprüft und akzeptiere eine teilweise Veröffentlichung

repo-workflows-repository_tools-show-8f3b56b3 = Ich habe jedes benannte Remote, den exakten Commit und den Zielbranch geprüft

repo-workflows-repository_tools-show-8f8df690 = Belege der Zustandsveröffentlichung

repo-workflows-repository_tools-show-8fb60b6e = Eingeschränkte Anmeldung: { $crate-git-redact-endpoint-none-1 } · Konto { $account-2 } · Benutzername { $username-3 }

repo-workflows-repository_tools-show-8fd0f155 = Anker: { $m-anchor-1 } bei { $crate-repo-short-m-base-2 }

repo-workflows-repository_tools-show-8fe4dcef = Routen ändern …

repo-workflows-repository_tools-show-9174163c = Isolierter Restack

repo-workflows-repository_tools-show-927164d1 = Ich habe diesen gespeicherten Stand bis zum angezeigten Endpunkt geprüft

repo-workflows-repository_tools-show-92958e20 = Diese Liste zurückholen

repo-workflows-repository_tools-show-93fdd760 = Vollständiger Upstream-Verlauf

repo-workflows-repository_tools-show-96a8a275 = Die Freigabe fixiert das gewählte Profil, Metadaten des Verbindungskontos und eine gegebenenfalls eingeschränkte temporäre Anmeldung. Ein geänderter oder fehlender älterer Nachweis benötigt erneute Freigabe. Vorbereitete Ergebnisse verfallen bei Profil- oder Anmeldungsänderungen.

repo-workflows-repository_tools-show-98a12458 = Kandidaten-Worktree fortsetzen / abgleichen

repo-workflows-repository_tools-show-99256c25 = Gewähltes Profil { $p-consent-profile-1 } · Metadaten des Verbindungskontos fixiert

repo-workflows-repository_tools-show-99d05df7 = Committeten Ordnerverlauf abspalten

repo-workflows-repository_tools-show-9aa26ad9 = Ordnen Sie jede aktuelle Datei einer aktiven Liste zu oder lassen Sie sie unzugeordnet:

repo-workflows-repository_tools-show-9b1cea76 = { $p-plan-commits-len-1 } ausgehende Commits geprüft

repo-workflows-repository_tools-show-9b9a8d05 = Lesen

repo-workflows-repository_tools-show-9bb7e70d = Gelösten Kandidaten fortsetzen / prüfen

repo-workflows-repository_tools-show-9cbacb41 = Diesen exakten Baum zugeordneter Hunks als unsignierten virtuellen Branch-Commit committen

repo-workflows-repository_tools-show-9d8f0223 = Ausstehend: behalten und prüfen

repo-workflows-repository_tools-show-9e153558 = Exakter Split { $p-plan-head-1 } → { $p-plan-remote-2 }/{ $p-plan-destination-3 }

repo-workflows-repository_tools-show-9e8617f1 = ursprünglicher Elterncommit dieser Sitzung

repo-workflows-repository_tools-show-9fbeaee4 = Push:

repo-workflows-repository_tools-show-a1016f72 = Teilklon: Einige Objekte benötigen möglicherweise dessen Promisor-Remote.

repo-workflows-repository_tools-show-a17b0445 = Exakter Commit { $p-plan-head-1 } → { $p-plan-remote-2 } / { $p-plan-destination-3 }

repo-workflows-repository_tools-show-a23601d6 = Verwendet den contrib-Subtree-Helfer des installierten Git nur bei Vorbereiten oder Fortsetzen. Abruf und Veröffentlichung sind ausdrückliche Netzwerkaktionen. Kandidaten, App-Referenzen und Wiederherstellungsverzeichnisse bleiben lokal erhalten. Spiegeln Sie App-Referenzen nicht per Push. Nur native Unix-Checkouts; ein fehlender oder scheiternder Helfer hinterlässt einen prüfbaren Vorschlag.

repo-workflows-repository_tools-show-a253af39 = Gespeichert und inaktiv: { $h-path-1 } · { $h-key-8-2 }

repo-workflows-repository_tools-show-a2869e42 = Ausgang erneut lesen

repo-workflows-repository_tools-show-a2bc21d3 = Freigegebenes Profil { $consent-profile-1 } · Metadaten des gewählten Kontos fixiert

repo-workflows-repository_tools-show-a39608b9 = Diese Hunks deaktivieren

repo-workflows-repository_tools-show-a503aa66 = Pushen nach

repo-workflows-repository_tools-show-a6888267 = Letzter akzeptierter Import: { $oid }

repo-workflows-repository_tools-show-a6b3283e = Ordner:

repo-workflows-repository_tools-show-aa785909 = Läuft oder wurde unterbrochen: Prüfen Sie den Worktree und gleichen Sie die Belege ausdrücklich ab

repo-workflows-repository_tools-show-aaac53fc = Nur die { $p-name-1 } zugeordneten Hunks gehen in diesen Branch-Commit ein. Ordner, Anker und echter Index bleiben wie geprüft.

repo-workflows-repository_tools-show-ab27abcb = Belege der Zustandsveröffentlichung prüfen

repo-workflows-repository_tools-show-ab31db17 = Veraltete Zuordnung: { $h-path-1 }

repo-workflows-repository_tools-show-ad16979e = Committete Quelle: { $v-branch-1 } bei { $v-head-2 }

repo-workflows-repository_tools-show-ae033504 = Gespeicherte Gruppen

repo-workflows-repository_tools-show-ae30be2e = Nachrichtenentwurf speichern

repo-workflows-repository_tools-show-aff83496 = Gruppe:

repo-workflows-repository_tools-show-b0a1bb3e = Exakten Annahmebeleg abgleichen

repo-workflows-repository_tools-show-b0e84ec1 = Diese Subtree-Aktualisierung akzeptieren

repo-workflows-repository_tools-show-b1249b5a = Ich habe die Pull- und Push-Ziele vor dem Speichern geprüft

repo-workflows-repository_tools-show-b26a038e = Wählen Sie einen vorhandenen Commit mit beiden Seiten oder öffnen Sie das Submodul, mergen Sie dort und aktualisieren Sie diesen Graphen. Das Erfassen ändert nur den übergeordneten Index.

repo-workflows-repository_tools-show-b2f82ce9 = { $p-branch-1 }: { $p-expected-2 } nach { $p-old-base-3 } → { $p-onto-4 }

repo-workflows-repository_tools-show-b7b34c26 = Gewählten Submodul-Commit im übergeordneten Repository erfassen

repo-workflows-repository_tools-show-b85186ac = Übergeordnet: { $p-branch-1 } bei { $p-commit-2 } → { $p-parent-remote-3 }/{ $p-parent-branch-4 } ({ $git-redact-p-parent-url-none-5 })

repo-workflows-repository_tools-show-b87ce680 = Pull:

repo-workflows-repository_tools-show-b9cc89c6 = Die erneute Freigabe behält den URSPRÜNGLICH fixierten Commit und akzeptiert das AKTUELLE Ziel und den Quellverlauf. Prüfen Sie ein ungewisses früheres Ergebnis zuerst auf dem Remote. Automatischer Versand ist deaktiviert.

repo-workflows-repository_tools-show-ba765b08 = Bereite Kandidaten akzeptieren

repo-workflows-repository_tools-show-ba8a03e2 = Topologie aktualisieren und erneut vorbereiten

repo-workflows-repository_tools-show-bbcce2f9 = Nach dem Merge aktualisieren

repo-workflows-repository_tools-show-be1e9f5f = Diese Hunks aktivieren

repo-workflows-repository_tools-show-c23fbbaa = Remote-URLs, Refspecs und Spiegelungsregeln

repo-workflows-repository_tools-show-c29574be = Vorschau gekürzt. Vergleichen Sie diesen Durchgang vor der Annahme.

repo-workflows-repository_tools-show-c2c3785a = Beide Routen speichern

repo-workflows-repository_tools-show-c757c39b = Rebase und Restack laufen in eigenen gespeicherten, losgelösten Worktrees. Originale bleiben bis zur Annahme unverändert. Lösen Sie Konflikte im vorgeschlagenen Worktree, merken Sie das Ergebnis vor und wählen Sie „Fortsetzen“. Die Wiederanwendung ist unsigniert und überspringt Repository-Hooks. Die Annahme verwendet die geschützte native Transaktion und verweigert veränderte oder veraltete Original-Worktrees. Teilerfolge werden pro Schicht gemeldet. Rückgängig stellt jeden angenommenen Originalstand mit denselben Schutzprüfungen wieder her. Kein Worktree wird automatisch gelöscht.

repo-workflows-repository_tools-show-c7879dfc = Split-Veröffentlichung prüfen

repo-workflows-repository_tools-show-cb9b7596 = Nicht vorgemerkte Ordneränderungen

repo-workflows-repository_tools-show-ccbcb47e = Übergeordnet: { $git-redact-s-none-1 }

repo-workflows-repository_tools-show-d05d82b6 = Push-URLs (leer übernimmt Abruf-URLs; Git pusht an alle aufgeführten URLs)

repo-workflows-repository_tools-show-d15b2630 = Diese Pushes sind nicht atomar. Ein erfolgreicher Host bleibt veröffentlicht, wenn ein anderer fehlschlägt. Jedes Ziel erhält den exakten angezeigten Commit. Wiederholen sendet nur an fehlgeschlagene Ziele.

repo-workflows-repository_tools-show-d3197597 = Committete Ordneränderungen

repo-workflows-repository_tools-show-d329d33b = Commit zugeordneter Hunks ansehen …

repo-workflows-repository_tools-show-d5680e1f = Bereinigung mit diesen Zuordnungen würde { $p-prune-len-1 } lokale Referenzen löschen:

repo-workflows-repository_tools-show-d5714f7d = Ich habe diesen exakten Kandidaten-Patch beziehungsweise die abgerufene Verbindungsbasis geprüft. Für einen erneuten Versand habe ich das Remote nach abgelehnten oder ungewissen Versuchen geprüft.

repo-workflows-repository_tools-show-d653a1c5 = Eintrag entfernen

repo-workflows-repository_tools-show-dad5a969 = Beleg

repo-workflows-repository_tools-show-db839840 = Worktrees mit diesem gemeinsamen Verzeichnis:

repo-workflows-repository_tools-show-deee0d61 = Diesen Hunk als geprüft markieren

repo-workflows-repository_tools-show-df2937d4 = Einschließlich App-, Bisect-, Sicherungs- und Namensraumreferenzen. Objekt-IDs benennen den exakten Stand unten.

repo-workflows-repository_tools-show-e01205cf = Liste umbenennen

repo-workflows-repository_tools-show-e0b832a0 = Primärer Speicher dieses Repositorys

repo-workflows-repository_tools-show-e0e54b30 = Abgerufene Quelle: { $oid }

repo-workflows-repository_tools-show-e27da1c1 = Verlauf (bis zu 200 Commits)

repo-workflows-repository_tools-show-e37c5523 = Spiegel-Pushes: alle Referenzen erzwingen und zusätzliche Zielreferenzen löschen

repo-workflows-repository_tools-show-e5c0b5df = Neuer echter Branch:

repo-workflows-repository_tools-show-e6eadbe7 = Ich habe diese URLs, Zuordnungen und Folgen der Spiegelung geprüft

repo-workflows-repository_tools-show-e8331aba = Quelle ausdrücklich abrufen, um committete Ordnerinhalte zu vergleichen

repo-workflows-repository_tools-show-e949fe01 = Übernommene Ordner und Subtree-Veröffentlichung

repo-workflows-repository_tools-show-e9e7ecb3 = Jede Abhängigkeit und jeden ausgehenden Gitlink prüfen

repo-workflows-repository_tools-show-ea03ec13 = Wählen Sie ein vorhandenes konfiguriertes Quellremote. Seine Abruf- und Push-Routen können abweichen. Die Registrierung speichert nur die Herkunft. Ein fehlender Ordner lässt sich nach ausdrücklichem Abruf mit „Aktualisierung vorbereiten“ importieren.

repo-workflows-repository_tools-show-ea2af9ef = Abrufspiegelung wird mit +refs/*:refs/* ausgedrückt. Sie schreibt direkt in lokale Namensräume samt Branches. Push-Spiegelung kann fremde Arbeit überschreiben oder löschen. Diese Vorschauen bereinigen und pushen nie.

repo-workflows-repository_tools-show-ea303dfe = Zustand: { $report-intent-state-ref-1 }
    Vorher: { $report-intent-previous-2 }
    Vorgeschlagen: { $report-intent-proposed-3 }
    Beleg: { $report-intent-receipt-ref-4 }
    Quellbranch: { $report-intent-branch-as-deref-un-5 }
    Erwarteter HEAD: { $report-intent-expected-head-as-d-6 }
    Wiederherstellungseinträge: { $report-intent-evidence-7 }

repo-workflows-repository_tools-show-ea721fba = Zuordnung und Arbeitsstand aktualisieren

repo-workflows-repository_tools-show-ed38427e = Nur dieser Split-Commit wird gesendet. Spätere Original- oder Kandidatenänderungen kommen nicht hinzu. Schreibzugriffe auf mehrere Endpunkte sind nicht atomar. Ablehnung oder fehlende Bestätigung pausiert den Vorschlag. Keine automatische Wiederholung.

repo-workflows-repository_tools-show-edc5ba83 = Ich habe den ausgewählten Commit geprüft und möchte diese übergeordnete Konfliktlösung erfassen

repo-workflows-repository_tools-show-eef651ca = Parallele Branches in diesem Ordner

repo-workflows-repository_tools-show-ef0a338b = Bestätigt: exakter Beleg

repo-workflows-repository_tools-show-efdfe52a = Erstellen Sie zuerst einen neuen Branch. Bestehende Arbeitshunks bleiben bis zur Eigentümerwahl unzugeordnet.

repo-workflows-repository_tools-show-f279f80b = Push-Refspecs (leer verwendet push.default)

repo-workflows-repository_tools-show-f2d4cf1c = Dieses Git kann beliebige Wurzelreferenzen nicht aufzählen. Bekannte Pseudoreferenzen werden eingeschlossen, sofern vorhanden.

repo-workflows-repository_tools-show-f41d4ab9 = Wiederherstellungsverzeichnisse behalten Original- und Kandidatendateien sowie vollständige Zugriffsmetadaten. Prüfen Sie diese Einträge und index.lock vor einer manuellen Wiederherstellung. Journale sind auf 256 Vorgänge begrenzt. Archivieren Sie erledigte Journalreferenzen erst nach Prüfung der Wiederherstellungseinträge.

repo-workflows-repository_tools-show-f5265a26 = Ungeprüft

repo-workflows-repository_tools-show-f581c406 = Commit-Nachrichtenentwurf

repo-workflows-repository_tools-show-f5b402c5 = Ordner gegenüber abgerufenem Upstream { $base }. Die ursprüngliche Importbasis ist unbekannt, daher sind auch Upstream-Unterschiede enthalten.

repo-workflows-repository_tools-show-f66ce8b9 = Exakten Commit und Ziele vorbereiten

repo-workflows-repository_tools-show-fad6057b = Geprüft

repo-workflows-repository_tools-show-faf582fc = Virtuellen Branch erstellen

repo-workflows-repository_tools-show-fb535d0d = Vorschlag schließen; alle Arbeit behalten

repo-workflows-repository_tools-show-fcb0aec1 = Remote für Prüfung / Veröffentlichung:

repo-workflows-repository_tools-show-fccdc9ec = Fixierten Commit und aktuelles Ziel prüfen …

repo-workflows-repository_tools-show-ff1cf26d = Committete lokale Änderungen gegenüber dem zuletzt akzeptierten Upstream { $base }

repo-workflows-repository_tools-show-fff50056 = Modus verlassen; Dateien und Branchreferenzen behalten

repo-workflows-repository_tools-start-send-628a48e6 = Profil oder Anmeldung des vorbereiteten Ausgangs geändert. Erneut vorbereiten und freigeben.

repo-workflows-repository_tools-start-send-994ea6b0 = Exakten eingereihten Push senden

repo-workflows-repository_tools-start-send-e7b0c78d = Automatischer Versand des freigegebenen einfachen Ausgangs-Pushes

repo-workflows-review_workflow-review-document-ui-7d9eb7ac = Schließen

repo-workflows-review_workflow-show-c3812fc4 = Entfernen

repo-workflows-stack-publish-dialog-19766ed6 = Abbrechen

repo-workflows-stack-restack-dialog-19766ed6 = Abbrechen

repo-workflows-stack-stack-dialogs-19766ed6 = Abbrechen

repo-workflows-surgery-footer-19766ed6 = Abbrechen

repo-workflows-test_lane-test-banner-7d9eb7ac = Schließen

repo-workflows-test_lane-test-output-window-e21f935f = Kopieren

repo-workflows-timeline-close-button-7d9eb7ac = Schließen

repo-workflows-timeline-commits-683768ab = und { $total-list-len-1 } weitere

repo-workflows-timeline-finished-19766ed6 = Abbrechen

repo-workflows-timeline-forget-confirm-19766ed6 = Abbrechen

repo-workflows-timeline-jump-controls-19766ed6 = Abbrechen

repo-workflows-working_drafts-history-7d9eb7ac = Schließen

repo-workflows-working_drafts-recovery-19766ed6 = Abbrechen

repo-workflows-activity-count = { $count ->
        [one] Ein Vorgang{ $running }, neueste zuerst
       *[other] { $count } Vorgänge{ $running }, neueste zuerst
    }

repo-workflows-activity-running-extra = { " und der laufende" }

repo-workflows-activity-entry-summary = { $outcome } · { $duration } · vor { $age }{ $background ->
        [yes] { " · im Hintergrund" }
       *[no] { "" }
    }{ $repeats ->
        [one] { "" }
       *[other] { " · " }{ $repeats } Mal in Folge
    }

repo-workflows-agent-stage-conflicts = { $count ->
        [one] Diese Konfliktdatei als gelöst vormerken: { $paths }. Git vergisst danach den Konflikt und seine drei Seiten.
       *[other] Diese Konfliktdateien als gelöst vormerken: { $paths }. Git vergisst danach die Konflikte und ihre drei Seiten.
    }

repo-workflows-agent-send-confirmation = { $what } wurde in das Terminal { $terminal } eingefügt. Drücken Sie dort Enter, um die Nachricht zu senden.

repo-workflows-agent-note-location = Notiz für den Agenten · { $path }:{ $line }{ $old ->
        [yes] { " (vorheriger Stand)" }
       *[no] { "" }
    }

repo-workflows-agent-session-title = { $first } · { $count ->
        [one] Ein Durchgang
       *[other] { $count } Durchgänge
    } · { $when }

repo-workflows-ai-coverage = Das Tool hat { $whole } von { $files ->
        [one] einer Datei
       *[other] { $files } Dateien
    } vollständig gesehen, den Rest nur teilweise oder als Namen (Einstellungen → KI bestimmt die übertragene Datenmenge).

repo-workflows-agent-hard-reset-body = Verschiebt { $target } von { $from } auf { $to } („{ $subject }“) und passt das Arbeitsverzeichnis daran an. { $lost ->
        [0] Derzeit gibt es keine uncommitteten Änderungen an versionierten Dateien.
        [one] Die uncommitteten Änderungen an einer versionierten Datei gehen unwiderruflich verloren.
       *[other] Die uncommitteten Änderungen an { $lost } versionierten Dateien gehen unwiderruflich verloren.
    } { $left ->
        [0] Es werden keine Commits entfernt.
        [one] Der Commit nach { $to } ist nicht mehr auf { $target }.
       *[other] Die { $left } Commits nach { $to } sind nicht mehr auf { $target }.
    } Unversionierte Dateien bleiben erhalten.

repo-workflows-agent-mixed-reset-body = Verschiebt { $target } von { $from } auf { $to } („{ $subject }“) und leert die Staging-Area. { $files ->
        [one] Die Änderungen der vorgemerkten Datei bleiben unvorgemerkt erhalten.
       *[other] Die Änderungen der { $files } vorgemerkten Dateien bleiben unvorgemerkt erhalten.
    } Das Arbeitsverzeichnis bleibt unverändert.

repo-workflows-agent-drop-body = Entfernt { $commit } („{ $subject }“) und seine Änderungen aus { $branch } und wendet { $after ->
        [one] den nachfolgenden Commit
       *[other] die { $after } nachfolgenden Commits
    } erneut an. Bei einem Konflikt hält der Rebase an, damit Sie ihn abschließen oder abbrechen können.

repo-workflows-agent-delete-body = Löscht den lokalen Branch „{ $name }“ bei { $commit } („{ $subject }“). { $force ->
        [yes] Auch löschen, wenn er nicht gemergt ist.
       *[no] Git verweigert dies, wenn er nicht gemergt ist.
    } Seine Kopie auf einem Remote bleibt erhalten.

repo-workflows-agent-force-lost = { $count ->
        [one] Der Commit auf { $to }, der { $branch } fehlt, wird für alle Benutzer entfernt.
       *[other] Die { $count } Commits auf { $to }, die { $branch } fehlen, werden für alle Benutzer entfernt.
    }

repo-workflows-agent-push-body = Sendet { $count ->
        [one] den neuen Commit
       *[other] die { $count } neuen Commits
    } von { $branch } an { $to }.

repo-workflows-dco-toggle-0 = { repo-workflows-dco--5e1a6b4e }. Hier aktiviert; dieses Repository verlangt es ebenfalls: { $reason }

repo-workflows-dco-toggle-1 = { repo-workflows-dco--5e1a6b4e }. Hier deaktiviert, obwohl dieses Repository es verlangt: { $reason }

repo-workflows-dco-toggle-2 = { repo-workflows-dco--5e1a6b4e }. Für dieses Repository aktiviert

repo-workflows-dco-toggle-3 = { repo-workflows-dco--5e1a6b4e }. Für dieses Repository deaktiviert

repo-workflows-dco-toggle-4 = { repo-workflows-dco--5e1a6b4e }. Automatisch aktiviert: { $reason }

repo-workflows-dco-toggle-5 = { repo-workflows-dco--5e1a6b4e }. Automatisch aktiviert, wenn ein Repository dies verlangt (DCO-Datei oder fehlgeschlagene DCO-Prüfung)

repo-workflows-dco-all-signed = { $count ->
        [one] ✔ Der Commit ist mit einem Sign-off versehen.
       *[other] ✔ Alle { $count } Commits sind mit einem Sign-off versehen.
    }

repo-workflows-dco-missing-signoffs = { $missing } von { $total } Commits fehlt ein von der DCO-Prüfung akzeptierter Sign-off des Autors oder Committers:

repo-workflows-bakeoff-unassembled = { $count ->
        [one] Eine Datei ausgewählt, aber nicht zusammengestellt. Erneutes Schließen verwirft sie.
       *[other] { $count } Dateien ausgewählt, aber nicht zusammengestellt. Erneutes Schließen verwirft sie.
    }

repo-workflows-collision-files = { $count ->
        [one] Eine Datei
       *[other] { $count } Dateien
    }

repo-workflows-collision-conflicts = { $a } und { $b } haben Konflikte in { $count ->
        [one] einer Datei
       *[other] { $count } Dateien
    }:

repo-workflows-collision-check = { $count ->
        [one] Einen Branch prüfen
       *[other] { $count } Branches prüfen
    }

repo-workflows-doctor-remove-setting = { $key } = { $value } aus { $file } entfernen? { $scope ->
        [global] Dies gilt für alle Ihre Repositorys.
        [system] Dies gilt für alle Benutzer dieses Rechners.
       *[local] Dies gilt hier.
    } Der Wert wird nicht gespeichert. Um ihn wiederherzustellen, müssen Sie ihn erneut setzen.

repo-workflows-doctor-history-loaded = Der Graph lädt beim Scrollen jeweils { $page } Commits, insgesamt höchstens { $max }. Auch die Suche lädt bei Bedarf bis zu dieser Grenze nach. Derzeit sind { $loaded } geladen.{ $more ->
        [yes] { " Weitere können geladen werden." }
       *[no] { "" }
    }

repo-workflows-doctor-missing-hooks = Dieser Ordner existiert nicht oder ist kein Ordner. Kein Hook wird ausgeführt.{ $custom ->
        [yes] { " Die Software, die core.hooksPath gesetzt hat (ein Hook-Manager?), ist nicht installiert. Führen Sie deren Installation aus oder entfernen Sie core.hooksPath." }
       *[no] { "" }
    }

repo-workflows-hotfix-change-summary = { $branch } · { $count ->
        [one] Eine Änderung
       *[other] { $count } Änderungen
    }{ $has-base ->
        [yes] { " seit " }{ $base }
       *[no] { "" }
    }

repo-workflows-history-calendar-day = { $day }
    { $count ->
        [one] { $count } Commit
       *[other] { $count } Commits
    }

repo-workflows-lfs-checkout-summary = { $files ->
        [0] Keine Datei dieses Checkouts wird in LFS gespeichert.
       *[other] { $missing ->
            [0] { $files ->
                [one] Eine Datei in LFS, heruntergeladen.
               *[other] { $files } Dateien in LFS, alle heruntergeladen.
            }
           *[other] { $files ->
                [one] Eine Datei in LFS;
               *[other] { $files } Dateien in LFS;
            } { $missing ->
                [one] eine liegt hier nur als Zeiger vor
               *[other] { $missing } liegen hier nur als Zeiger vor
            } ({ $bytes } nicht heruntergeladen).
        }
    }

repo-workflows-lfs-force-unlock-confirmation = { $count ->
        [one] Erneut klicken, um die Freigabe zu erzwingen. Der Inhaber verliert die Sperre.
       *[other] Erneut klicken, um die Freigabe zu erzwingen. Die Inhaber verlieren ihre { $count } Sperren.
    }

