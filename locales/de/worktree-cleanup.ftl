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
