interaction-operation-fetch = Abrufen
interaction-operation-pull = Pullen
interaction-operation-push = Pushen
interaction-operation-commit = Committen
interaction-operation-amend = Commit ändern
interaction-operation-stage = Vormerken
interaction-operation-stage-all = Alles vormerken
interaction-operation-unstage = Vormerkung aufheben
interaction-operation-unstage-all = Alle Vormerkungen aufheben
interaction-operation-discard = Verwerfen
interaction-operation-stash = Zwischenspeichern
interaction-operation-pop-stash = Stash anwenden und löschen
interaction-operation-apply-stash = Stash anwenden
interaction-operation-drop-stash = Stash löschen
interaction-operation-set-identity = Identität festlegen
interaction-operation-profile-author = Autor des Profils verwenden
interaction-operation-mark-resolved = Als gelöst markieren
interaction-operation-reload = Neu laden
interaction-operation-sync = Synchronisieren
interaction-operation-push-review = Zur Überprüfung pushen
interaction-operation-skip-hooks = Ohne Hooks pushen
interaction-operation-apply-patch = Patch anwenden
interaction-operation-cherry-pick = Cherry-Pick
interaction-operation-revert = Rückgängig-Commit
interaction-operation-rebase = Rebase
interaction-operation-merge = Zusammenführen
interaction-operation-dvc-pull = DVC: Abrufen
interaction-operation-abort-merge = Merge abbrechen
interaction-operation-abort-rebase = Rebase abbrechen
interaction-operation-abort-cherry-pick = Cherry-Pick abbrechen
interaction-operation-abort-revert = Revert abbrechen
interaction-operation-abort-am = Patch-Anwendung abbrechen
interaction-operation-continue-merge = Merge fortsetzen
interaction-operation-continue-rebase = Rebase fortsetzen
interaction-operation-continue-cherry-pick = Cherry-Pick fortsetzen
interaction-operation-continue-revert = Revert fortsetzen
interaction-operation-continue-am = Patch-Anwendung fortsetzen
interaction-operation-checkout-pull = { $value } auschecken und pullen
interaction-operation-checkout = { $value } auschecken
interaction-operation-create-branch = Branch { $value } erstellen
interaction-operation-delete-branch = Branch { $value } löschen
interaction-operation-rename-branch = Branch { $value } umbenennen
interaction-operation-release-tag = Release-Tag { $value } erstellen
interaction-operation-create-tag = Tag { $value } erstellen
interaction-operation-delete-tag = Tag { $value } löschen
interaction-operation-pick = Cherry-Pick von { $value }
interaction-operation-revert-item = { $value } durch Rückgängig-Commit aufheben
interaction-operation-stop-tracking = Tracking von { $value } beenden
interaction-operation-ignore = { $value } ignorieren
interaction-operation-track-lfs = { $value } in LFS verwalten
interaction-operation-apply-file = { $value } anwenden
interaction-operation-undo = Rückgängig: { $operation }
interaction-operation-redo = Wiederholen: { $operation }
interaction-operation-preparing = Vorbereitung: { $operation }
interaction-operation-push-after = Pushen nach { $operation }
interaction-operation-push-to = { $branch } nach { $remote } pushen
interaction-operation-merge-into = { $branch } in { $target } zusammenführen
interaction-operation-rebase-onto = Rebase von { $branch } auf { $target }
interaction-operation-fast-forward = { $branch } auf { $target } vorspulen
interaction-settings-compact-tip = Spuren liegen enger beieinander und verkleinern die Graph-Spalte

interaction-accounts-unnamed = Unbenanntes Profil

interaction-custom-display-failed = { $name } fehlgeschlagen (Exitstatus { NUMBER($code) })

interaction-custom-display-finished = { $name } abgeschlossen

interaction-custom-display-index = Befehl { NUMBER($index) }

interaction-custom-display-json = { $file } kann nicht gelesen werden: { $detail }

interaction-custom-display-large = { $file } ist größer als { NUMBER($limit) } KiB und wird daher nicht gelesen

interaction-custom-display-name = „{ $name }“

interaction-custom-display-problem = { $file }: { $command }: { $problem }

interaction-gerrit-amend-invalid = Ungültige Gerrit-Commit-Änderung

interaction-gerrit-amend-message = Nachricht zum Ändern des Commits fehlt

interaction-gerrit-ancestor = Der Zielbranch ist kein Vorfahr dieses Commits. Vor dem Push zur Prüfung abrufen und auf den Zielbranch rebasen.

interaction-gerrit-choose = Ein Remote, einen abgerufenen Zielbranch und ein gültiges Thema wählen

interaction-gerrit-duplicate = Zwei Commits haben dieselbe Change-Id ({ $id }); jede Änderung benötigt eine eigene

interaction-gerrit-empty = Über dem Zielbranch gibt es keine Commits zur Prüfung

interaction-gerrit-footer = Change-Id muss im letzten Trailer-Absatz stehen

interaction-gerrit-format = Change-Id muss aus I gefolgt von 40 Hexadezimalziffern bestehen (nicht nur Nullen)

interaction-gerrit-hook = { $commit }: { $reason }. Den commit-msg-Hook von Ihrem Gerrit-Server installieren und dann diesen Commit ändern, um seine Change-Id hinzuzufügen.

interaction-gerrit-keep = Beim Ändern einer Gerrit-Änderung die ursprüngliche Change-Id beibehalten

interaction-gerrit-merge = Merge-Commit { $commit } kann mit diesem Review-Ablauf nicht gepusht werden; zuerst die Serie rebasen

interaction-gerrit-missing = Change-Id fehlt im Nachrichtenfuß

interaction-gerrit-multiple = mehr als eine Change-Id im Nachrichtenfuß

interaction-gerrit-one-line = Change-Id muss in eine Zeile passen

interaction-gerrit-web = Ungültige gitcraft.gerritUrl: eine HTTP(S)-URL ohne Zugangsdaten, Abfrage oder Fragment verwenden. Die Webadresse des Remotes wird verwendet; Gerrit-Reviews können weiterhin eingereicht werden.

interaction-preferences-bad-variable = „{ $variable }“ ist kein Variablenname: Großbuchstaben, Ziffern und _, beginnend mit einem Buchstaben

interaction-preferences-built-in = ${ $variable } wird bereits von gitcraft gesetzt

interaction-preferences-empty-line = Die Befehlszeile ist leer

interaction-preferences-missing-name = Der Name fehlt

interaction-preferences-nul = Enthält ein NUL-Zeichen

interaction-preferences-reserved = ${ $variable } wird von der Shell oder Git verwendet; wähle einen anderen Namen

interaction-preferences-twice = ${ $variable } wird zweimal abgefragt

interaction-recipe-not-saved = Das Rezept wurde nicht gespeichert: { $problem }

interaction-recipe-path = { $path }: { $reason }

interaction-recipe-path-empty = leerer Pfad

interaction-recipe-path-line = ein Pfad muss in einer Zeile stehen

interaction-recipe-path-outside = ein Pfad darf den Worktree nicht verlassen (..)

interaction-recipe-path-relative = einen Pfad innerhalb des Worktrees angeben, relativ zu dessen oberstem Ordner

interaction-recipe-path-space = beginnt oder endet mit einem Leerzeichen

interaction-recipe-path-whole = das ist der gesamte Worktree

interaction-recipe-variable-duplicate = Variable „{ $name }“ wird zweimal gesetzt

interaction-recipe-variable-line = Variable „{ $name }“: ein Wert muss in einer Zeile stehen

interaction-recipe-variable-name = Variable „{ $name }“: Namen bestehen aus Buchstaben, Ziffern und _ und dürfen nicht mit einer Ziffer beginnen

interaction-recipe-variable-reserved = Variable „{ $name }“: gitcraft setzt sie selbst; einen anderen Namen wählen
