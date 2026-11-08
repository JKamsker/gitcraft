interaction-guards-collision-invalid-pair = { $a } konnte nicht mit { $b } im Speicher zusammengeführt werden: Eines davon ist weder ein Branch noch ein Commit.

interaction-guards-collision-merge-empty = { $a } konnte nicht mit { $b } im Speicher zusammengeführt werden.

interaction-guards-collision-merge-error = { $a } konnte nicht mit { $b } im Speicher zusammengeführt werden: { $detail }

interaction-guards-collision-needs-git = Die Kollisionsmatrix benötigt Git 2.38 oder neuer (git merge-tree --write-tree).

interaction-guards-collision-no-history = { $a } und { $b } haben keine gemeinsame Historie.

interaction-guards-collision-not-branch = { $reference } ist kein Branch.

interaction-guards-collision-not-merged = Nicht zusammengeführt

interaction-guards-collision-stopped = Angehalten

interaction-guards-drift-git-folder = Der Git-Ordner des Worktrees konnte nicht gefunden werden: { $detail }

interaction-guards-drift-headline = { $files ->
        [one] Eine Datei geändert
       *[other] { $files } Dateien geändert
    }{ $unrelated ->
        [yes] , { $unrelated_files ->
            [one] eine Datei
           *[other] { $unrelated_files } Dateien
        } in { $folders ->
            [one] einem Ordner, den
           *[other] { $folders } Ordnern, die
        } die Aufgabe nicht erwähnt
       *[no] { "" }
    }, { $dependencies ->
        [one] eine Abhängigkeit hinzugefügt
       *[other] { $dependencies } Abhängigkeiten hinzugefügt
    }, { $signatures ->
        [one] eine öffentliche Signatur geändert
       *[other] { $signatures } öffentliche Signaturen geändert
    }, { $percent }% reine Formatierung

interaction-guards-drift-issue-error = Issue #{ $issue } konnte nicht gelesen werden: { $detail }

interaction-guards-drift-measure-error = Die Änderungen des Worktrees konnten nicht vermessen werden: { $detail }

interaction-guards-drift-missing-tools = Kein Kommandozeilenprogramm eines Coding-Agenten ist installiert ({ $tools }): Die obigen Zahlen sind der exakt ermittelte Teil.

interaction-guards-drift-no-base = Es gibt keinen Commit des Hauptbranches, mit dem der Worktree verglichen werden kann.

interaction-guards-drift-not-revision = { $reference } ist keine Revision.

interaction-guards-drift-read-task = Die Aufgabe des Worktrees konnte nicht gelesen werden: { $detail }

interaction-guards-drift-run-error = { $tool } konnte nicht ausgeführt werden: { $detail }

interaction-guards-drift-save-task = Die Aufgabe des Worktrees konnte nicht gespeichert werden: { $detail }

interaction-guards-drift-timeout = { $tool } hat länger als { $seconds } s benötigt.

interaction-guards-drift-tool-failed = { $tool } ist fehlgeschlagen: { $detail }

interaction-guards-drift-unsupported-forge = Issues von { $forge } werden noch nicht gelesen.

interaction-guards-drift-wait-error = Auf { $tool } konnte nicht gewartet werden: { $detail }

interaction-guards-fence-check-error = { $target } konnte nicht gegen die Agentensperre geprüft werden: { $detail }

interaction-guards-fence-deny-list = „permissions.deny“ in .claude/settings.local.json ist keine Liste.

interaction-guards-fence-folder-error = Der Ordner von { $path } konnte nicht angelegt werden: { $detail }

interaction-guards-fence-git-folder = Der Git-Ordner des Worktrees konnte nicht gefunden werden: { $detail }

interaction-guards-fence-how-committed = committet

interaction-guards-fence-how-ignored = von Git ignoriert

interaction-guards-fence-how-uncommitted = nicht committet

interaction-guards-fence-how-untracked = unverfolgt

interaction-guards-fence-invalid-json = .claude/settings.local.json enthält kein gültiges JSON: { $detail }

interaction-guards-fence-line-break = { $entry } enthält einen Zeilenumbruch.

interaction-guards-fence-no-base = Es gibt keinen Hauptbranch für den Vergleich.

interaction-guards-fence-no-exceptions = { $pattern }: Eine Agentensperre erlaubt keine Ausnahmen (`!`); liste stattdessen die gesperrten Pfade auf.

interaction-guards-fence-no-folder = Der Ordner des Worktrees ist nicht vorhanden.

interaction-guards-fence-no-history = Es gibt keine gemeinsame Historie mit dem Hauptbranch.

interaction-guards-fence-not-object = .claude/settings.local.json ist kein JSON-Objekt.

interaction-guards-fence-not-revisions = { $base } oder { $tip } ist keine Revision.

interaction-guards-fence-permissions-object = „permissions“ in .claude/settings.local.json ist kein Objekt.

interaction-guards-fence-reach-error = Der Ordner des Worktrees konnte nicht erreicht werden: { $detail }

interaction-guards-fence-read-error = { $path } konnte nicht gelesen werden: { $detail }

interaction-guards-fence-read-fence = Die Agentensperre konnte nicht gelesen werden: { $detail }

interaction-guards-fence-record = das Sperrregelprotokoll

interaction-guards-fence-record-error = Die geschriebenen Regeln konnten nicht protokolliert werden: { $detail }

interaction-guards-fence-save-fence = Die Agentensperre konnte nicht gespeichert werden: { $detail }

interaction-guards-fence-save-targets = Die Schreibziele der Agentensperre konnten nicht gespeichert werden: { $detail }

interaction-guards-fence-write-error = { $path } konnte nicht geschrieben werden: { $detail }
