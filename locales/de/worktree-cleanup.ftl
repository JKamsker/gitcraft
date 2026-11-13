# Arbeitsverzeichnisse zusammengeführter Branches aufräumen, und die Archive ihrer nicht versionierten Dateien (#213).

worktree-cleanup-in-use-this-tab = dieser Tab zeigt es
worktree-cleanup-in-use-git = Git arbeitet darin (sein Index ist gesperrt)
worktree-cleanup-blocker-primary = das Haupt-Arbeitsverzeichnis des Repositorys
worktree-cleanup-blocker-missing = sein Ordner fehlt oder Git führt es nicht mehr
worktree-cleanup-blocker-bare = ein Bare-Repository
worktree-cleanup-blocker-locked = gesperrt: { $reason }
worktree-cleanup-blocker-branch-changed = darin ist jetzt ein anderer Branch ausgecheckt
worktree-cleanup-blocker-moved = der Branch hat neue Commits, seit er als zusammengeführt erkannt wurde
worktree-cleanup-blocker-not-merged = nicht mehr als zusammengeführt erkannt
worktree-cleanup-blocker-changed = { $count ->
        [one] { $count } geänderte versionierte Datei
       *[other] { $count } geänderte versionierte Dateien
    }
worktree-cleanup-blocker-operation = ein Merge, Rebase, Cherry-Pick, Revert oder Bisect läuft
worktree-cleanup-blocker-in-use = in Gebrauch: { $why }
worktree-cleanup-blocker-untracked = { $count ->
        [one] { $count } nicht versionierte Datei (zum Aufräumen archivieren)
       *[other] { $count } nicht versionierte Dateien (zum Aufräumen archivieren)
    }
worktree-cleanup-blocker-not-archivable = { $count ->
        [one] { $count } nicht versionierte Datei kann nicht archiviert werden (binär, zu groß, ein Link oder unlesbar)
       *[other] { $count } nicht versionierte Dateien können nicht archiviert werden (binär, zu groß, Links oder unlesbar)
    }
worktree-cleanup-blocker-unpublished = { $count ->
        [one] { $count } Commit ist weder in der Basis noch auf einem Remote
       *[other] { $count } Commits sind weder in der Basis noch auf einem Remote
    }
worktree-cleanup-blocker-remote = Arbeitsverzeichnisse auf einem anderen Rechner werden von hier nicht aufgeräumt
worktree-cleanup-archive-bad-key = Kein Archiv dieses Speichers
worktree-cleanup-archive-quota = Mit dem Archiv kämen die Archive auf { $needed }, über die Grenze von { $quota }. Nichts wurde entfernt. Lösche alte Archive oder erhöhe die Grenze unter Einstellungen → Git.
worktree-cleanup-archive-no-name = Kein freier Name für das Archiv gefunden
worktree-cleanup-archive-unverified = Das Archiv ließ sich nicht so zurücklesen, wie es geschrieben wurde
worktree-cleanup-archive-changed = { $path } hat sich beim Archivieren geändert
worktree-cleanup-archive-not-text = { $path } ist keine Textdatei
worktree-cleanup-archive-failed = Das Archivieren ist fehlgeschlagen, das Arbeitsverzeichnis bleibt: { $error }
worktree-cleanup-restore-other-repository = Dieses Archiv gehört zu einem anderen Repository
worktree-cleanup-restore-not-top = Archivierte Dateien werden nur in den obersten Ordner eines Arbeitsverzeichnisses wiederhergestellt
worktree-cleanup-restore-bad-path = Der archivierte Pfad ist kein einfacher Pfad im Arbeitsverzeichnis
worktree-cleanup-restore-blocked = Eine Datei oder ein Link ist im Weg, oder der Ort liegt außerhalb des Arbeitsverzeichnisses
worktree-cleanup-path-not-utf8 = Der Pfad des Arbeitsverzeichnisses ist kein gültiges UTF-8
worktree-cleanup-put-back = { $error } Das Arbeitsverzeichnis bleibt, seine nicht versionierten Dateien wurden zurückgelegt.
worktree-cleanup-put-back-failed = { $error } Das Arbeitsverzeichnis bleibt; seine nicht versionierten Dateien sind im Archiv { $archive }.
worktree-cleanup-title = Arbeitsverzeichnisse zusammengeführter Branches aufräumen
worktree-cleanup-intro = Arbeitsverzeichnisse, deren Branch zusammengeführt ist. Entfernt werden nur solche, in denen nichts verloren gehen kann, und nur die angehakten und bestätigten; jedes wird direkt davor noch einmal geprüft. Die Branches selbst bleiben.
worktree-cleanup-basis = Zusammengeführt heißt: in { $base } (den für dieses Repository festgelegten Standard-Branch), geprüft gegen { $refs }.
worktree-cleanup-checking = Welche Branches zusammengeführt sind, wird noch gelesen…
worktree-cleanup-reading = Die Arbeitsverzeichnisse werden angesehen…
worktree-cleanup-none = In keinem Arbeitsverzeichnis ist ein zusammengeführter Branch ausgecheckt.
worktree-cleanup-summary = Gibt { $size } frei · { $changed } geändert · { $untracked } nicht versioniert ({ $text } Text) · { $ignored } ignoriert ({ $ignored-size })
worktree-cleanup-show-files = Dateien zeigen
worktree-cleanup-hide-files = Dateien verbergen
worktree-cleanup-more-files = …und { $count } weitere
worktree-cleanup-file-text = wird archiviert, wenn Archivieren an ist
worktree-cleanup-file-binary = nicht archiviert: binär
worktree-cleanup-file-large = nicht archiviert: größer als 1 MiB
worktree-cleanup-file-link = nicht archiviert: ein Link
worktree-cleanup-file-other = nicht archiviert: keine einfache lesbare Datei
worktree-cleanup-archive-option = Nicht versionierte Textdateien vor dem Aufräumen archivieren
worktree-cleanup-archive-option-tip = Nicht versionierte Textdateien kommen in ein geprüftes ZIP mit Manifest im Archivordner (Einstellungen → Git → Arbeitsverzeichnis-Archive), bevor das Arbeitsverzeichnis entfernt wird. Binäre, große und verlinkte Dateien werden nie archiviert: ein Arbeitsverzeichnis mit solchen bleibt. Schlägt das Archivieren fehl, wird nichts entfernt.
worktree-cleanup-confirm = { $count ->
        [one] Ich habe die Liste geprüft: das { $count } angehakte Arbeitsverzeichnis entfernen. Sein Branch bleibt.
       *[other] Ich habe die Liste geprüft: die { $count } angehakten Arbeitsverzeichnisse entfernen. Ihre Branches bleiben.
    }
worktree-cleanup-remove = { $count ->
        [one] { $count } Arbeitsverzeichnis entfernen
       *[other] { $count } Arbeitsverzeichnisse entfernen
    }
worktree-cleanup-look-again = Erneut ansehen
worktree-cleanup-removed = entfernt, { $freed } freigegeben
worktree-cleanup-removed-archived = { $count ->
        [one] entfernt, { $freed } freigegeben; { $count } nicht versionierte Datei archiviert
       *[other] entfernt, { $freed } freigegeben; { $count } nicht versionierte Dateien archiviert
    }
worktree-cleanup-kept = behalten: { $why }
worktree-cleanup-changed = behalten: es hat sich seit dem Ansehen geändert. Vor dem Entfernen erneut ansehen.
worktree-cleanup-archives = Archivierte nicht versionierte Dateien
worktree-cleanup-no-archives = Kein Archiv dieses Repositorys.
worktree-cleanup-archive-usage = Archive belegen { $used } von { $quota }.
worktree-cleanup-archive-line = { $branch } · { $count ->
        [one] { $count } Datei
       *[other] { $count } Dateien
    } · { $size } · { $when }{ $restored ->
        [0] {""}
       *[other] {" "}· wiederhergestellt
    }
worktree-cleanup-restore = Wiederherstellen
worktree-cleanup-restore-tip = Legt die archivierten Dateien in das gewählte Arbeitsverzeichnis zurück. Dort wird nichts überschrieben: eine vorhandene Datei mit anderem Inhalt bleibt, die archivierte kommt als name.restored daneben. Das Archiv bleibt.
worktree-cleanup-restored = { $created } in { $into } wiederhergestellt ({ $renamed } neben einer anderen Datei, { $same } schon vorhanden, { $failed } fehlgeschlagen).
worktree-cleanup-delete = Löschen
worktree-cleanup-delete-confirm = Endgültig löschen?
worktree-cleanup-archive-deleted = Das Archiv wurde gelöscht.
worktree-cleanup-offer = { $count ->
        [one] In { $count } Arbeitsverzeichnis ist ein zusammengeführter Branch ausgecheckt.
       *[other] In { $count } Arbeitsverzeichnissen ist ein zusammengeführter Branch ausgecheckt.
    }
worktree-cleanup-offer-button = Aufräumen der Arbeitsverzeichnisse prüfen…
worktree-cleanup-settings-intro = Beim Aufräumen von Arbeitsverzeichnissen zusammengeführter Branches können ihre nicht versionierten Textdateien zuerst archiviert werden (jedes Mal auf Wunsch). Archive werden nie gelöscht, um Platz zu schaffen.
worktree-cleanup-settings-label-archives = Arbeitsverzeichnis-Archive
worktree-cleanup-settings-label-location = Archivordner
worktree-cleanup-settings-label-limit = Speichergrenze der Archive
worktree-cleanup-settings-label-auto = Automatisch wiederherstellen
worktree-cleanup-settings-label-list = Archive
worktree-cleanup-settings-location-hint = Ein Ordner außerhalb deiner Arbeitsverzeichnisse; leer für den Datenordner der App
worktree-cleanup-settings-limit-hint = Alle Archive zusammen; weiteres Archivieren wird abgelehnt (und das Arbeitsverzeichnis behalten), bis alte gelöscht sind
worktree-cleanup-settings-auto-restore = Archivierte Dateien wiederherstellen, wenn der Branch erneut ausgecheckt wird
worktree-cleanup-settings-auto-restore-tip = Erscheint ein Arbeitsverzeichnis des Branches auf dem Commit, von dem archiviert wurde, kommen seine Dateien einmal zurück, nie über eine vorhandene Datei.
worktree-cleanup-settings-none = Noch keine Archive.
