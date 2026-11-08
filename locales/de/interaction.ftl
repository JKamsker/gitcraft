interaction-operation-abort-am = Patch-Anwendung abbrechen
interaction-operation-abort-cherry-pick = Cherry-Pick abbrechen
interaction-operation-abort-merge = Merge abbrechen
interaction-operation-abort-rebase = Rebase abbrechen
interaction-operation-abort-revert = Revert abbrechen
interaction-operation-amend = Commit ändern
interaction-operation-apply-file = { $value } anwenden
interaction-operation-apply-patch = Patch anwenden
interaction-operation-apply-stash = Stash anwenden
interaction-operation-checkout = { $value } auschecken
interaction-operation-checkout-pull = { $value } auschecken und pullen
interaction-operation-cherry-pick = Cherry-Pick
interaction-operation-commit = Committen
interaction-operation-continue-am = Patch-Anwendung fortsetzen
interaction-operation-continue-cherry-pick = Cherry-Pick fortsetzen
interaction-operation-continue-merge = Merge fortsetzen
interaction-operation-continue-rebase = Rebase fortsetzen
interaction-operation-continue-revert = Revert fortsetzen
interaction-operation-create-branch = Branch { $value } erstellen
interaction-operation-create-tag = Tag { $value } erstellen
interaction-operation-delete-branch = Branch { $value } löschen
interaction-operation-delete-tag = Tag { $value } löschen
interaction-operation-discard = Verwerfen
interaction-operation-drop-stash = Stash löschen
interaction-operation-dvc-pull = DVC: Abrufen
interaction-operation-fast-forward = { $branch } auf { $target } vorspulen
interaction-operation-fetch = Abrufen
interaction-operation-ignore = { $value } ignorieren
interaction-operation-mark-resolved = Als gelöst markieren
interaction-operation-merge = Zusammenführen
interaction-operation-merge-into = { $branch } in { $target } zusammenführen
interaction-operation-pick = Cherry-Pick von { $value }
interaction-operation-pop-stash = Stash anwenden und löschen
interaction-operation-preparing = Vorbereitung: { $operation }
interaction-operation-profile-author = Autor des Profils verwenden
interaction-operation-pull = Pullen
interaction-operation-push = Pushen
interaction-operation-push-after = Pushen nach { $operation }
interaction-operation-push-review = Zur Überprüfung pushen
interaction-operation-push-to = { $branch } nach { $remote } pushen
interaction-operation-rebase = Rebase
interaction-operation-rebase-onto = Rebase von { $branch } auf { $target }
interaction-operation-redo = Wiederholen: { $operation }
interaction-operation-release-tag = Release-Tag { $value } erstellen
interaction-operation-reload = Neu laden
interaction-operation-rename-branch = Branch { $value } umbenennen
interaction-operation-revert = Rückgängig-Commit
interaction-operation-revert-item = { $value } durch Rückgängig-Commit aufheben
interaction-operation-set-identity = Identität festlegen
interaction-operation-skip-hooks = Ohne Hooks pushen
interaction-operation-stage = Vormerken
interaction-operation-stage-all = Alles vormerken
interaction-operation-stash = Zwischenspeichern
interaction-operation-stop-tracking = Tracking von { $value } beenden
interaction-operation-sync = Synchronisieren
interaction-operation-track-lfs = { $value } in LFS verwalten
interaction-operation-undo = Rückgängig: { $operation }
interaction-operation-unstage = Vormerkung aufheben
interaction-operation-unstage-all = Alle Vormerkungen aufheben

interaction-release-bump = Committet { $files } auf { $branch }: „chore(release): { $tag }“

interaction-release-changelog = { $file } (Release Notes am Anfang)

interaction-release-forge = der Forge

interaction-release-publish = { $pre ->
        [yes] Erstellt das Vorab-Release { $tag } auf { $forge } mit den Release Notes
       *[no] Erstellt das Release { $tag } auf { $forge } mit den Release Notes
    }

interaction-release-push-bump = Pusht den Versions-Commit nach { $branch } auf { $remote } und den Tag nach { $remote }, beide oder keinen (git push --atomic)

interaction-release-push-tag = Pusht den Tag nach { $remote }

interaction-release-step-bump = Version aktualisieren

interaction-release-step-publish = Release auf der Forge erstellen

interaction-release-step-push = Pushen

interaction-release-step-tag = Tag

interaction-release-step-upload = Dateien anhängen

interaction-release-tag-object = { $kind ->
        [signed] Erstellt den signierten Tag { $tag } an { $commit } mit den Release Notes als Nachricht (git tag -s { $tag })
       *[annotated] Erstellt den annotierten Tag { $tag } an { $commit } mit den Release Notes als Nachricht (git tag -a { $tag })
    }

interaction-release-tag-version = { $kind ->
        [signed] Erstellt den signierten Tag { $tag } an diesem Commit mit den Release Notes als Nachricht (git tag -s { $tag })
       *[annotated] Erstellt den annotierten Tag { $tag } an diesem Commit mit den Release Notes als Nachricht (git tag -a { $tag })
    }

interaction-release-upload = Hängt { $files } daran an
