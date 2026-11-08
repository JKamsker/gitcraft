# Interaction guard presentation only. Raw control strings, receipts, commands,
# source evidence and persisted agent/recovery instructions are intentionally unchanged.
# Numeric variables: counts/line numbers/percentages. OIDs/refs/paths/issue IDs,
# source/config/command lexemes, sizes already formatted by Git, and diagnostics are opaque.

interaction-guards-push-preview-failed = Die lokale Push-Vorschau konnte nicht abgeschlossen werden. Rufe fehlende Historie ausdrücklich ab und versuche es erneut; es wurde kein Push ausgeführt.

interaction-guards-push-config-read = Die lokale Push-Konfiguration konnte nicht gelesen werden; es wurde kein Push ausgeführt.

interaction-guards-push-complete-destination = Wähle eine vollständige Zielreferenz, etwa refs/heads/main.

interaction-guards-push-routing-limit = Die Push-Routing-Konfiguration überschreitet die Größenbegrenzung der Vorschau.

interaction-guards-push-control-characters = Ein Identitäts- oder Routing-Wert enthält Steuerzeichen. Korrigiere ihn, bevor du die Push-Vorschau öffnest.

interaction-guards-push-single-ref = Diese Push-Option erfordert eine ausdrückliche Vorschau für eine einzelne Referenz; diese Prüfliste sendet keine Pushes für mehrere oder übereinstimmende Referenzen und keine Spiegel-, atomaren oder rekursiven Pushes.

interaction-guards-push-one-destination = Prüfe jeweils ein Ziel in der Vorschau; es wurde kein Push ausgeführt.

interaction-guards-push-no-overrides = Prüfe ausdrückliche Referenzen ohne vorübergehende Überschreibungen der Push-Konfiguration.

interaction-guards-push-finish-operation = Schließe zuerst diesen lokalen Vorgang ab und prüfe seinen Push danach gesondert.

interaction-guards-push-one-push = Prüfe jeweils einen Push in der Vorschau; es wurde kein Push ausgeführt.

interaction-guards-push-detached-route = Wähle bei losgelöstem HEAD ausdrücklich ein Remote und eine Quellreferenz.

interaction-guards-push-choose-remote = Wähle vor dem Push dieses Branches ein Remote.

interaction-guards-push-remote-removed = Das Push-Remote ist nicht mehr konfiguriert; öffne die Vorschau erneut.

interaction-guards-push-mirror = Spiegel-Pushes erfordern eine eigene Prüfung mehrerer Referenzen; es wurde kein Push ausgeführt.

interaction-guards-push-address-limit = Prüfe höchstens acht konfigurierte Push-Adressen; teile größere Spiegelgruppen auf getrennte Remotes auf.

interaction-guards-push-no-address = Das Remote hat keine unterstützte Push-Adresse.

interaction-guards-push-detached-source = Wähle bei losgelöstem HEAD ausdrücklich eine Quellreferenz.

interaction-guards-push-multiple-refs = Das Remote pusht mehrere Referenzen. Wähle für die Vorprüfung einen ausdrücklichen Branch in seinem Menü.

interaction-guards-push-matching = Pushes übereinstimmender Referenzen erfordern für jeden Branch eine eigene Vorschau.

interaction-guards-push-upstream-name = Der Upstream-Branch hat einen anderen Namen; wähle seinen ausdrücklichen Branch-Push im Menü.

interaction-guards-push-no-wildcards = Wähle eine vollständige Referenz; Pushes mit Platzhaltern werden nicht gesendet.

interaction-guards-push-delete-upstream = Beim Löschen kann kein Upstream gesetzt werden.

interaction-guards-push-delete-lease-required = Das Löschen erfordert eine Lease für die genaue Referenz.

interaction-guards-push-delete-lease = Die Lösch-Lease muss genau diese Referenz und dieses Objekt nennen.

interaction-guards-push-delete-target-changed = Das abgerufene Ziel weicht von der Lösch-Lease ab; öffne die ursprüngliche Aktion erneut.

interaction-guards-push-delete-fetch = Rufe vor dem Löschen genau dieses Ziel ab; es wurde keine Remote-Spitze geprüft.

interaction-guards-push-commit-limit = Die ausgehende Historie überschreitet 10.000 Commits; teile den Push auf oder rufe vor der Prüfung sein genaues Ziel ab.

interaction-guards-push-lfs-file = Die eingecheckte .lfsconfig muss eine reguläre Datei sein, bevor ihr Endpunkt geprüft werden kann.

interaction-guards-push-lfs-control = Der LFS-Endpunkt enthält Steuerzeichen.

interaction-guards-push-no-target = Kein Push-Ziel vorhanden.

interaction-guards-push-checkpoint-limit = Die Liste der Checkpoint-Referenzen überschreitet 1 MiB; schränke die Veröffentlichung vor dem Push ein.

interaction-guards-push-transcript = Die Veröffentlichung von Checkpoints kann Prompts, lokale Pfade, Assistentenzusammenfassungen und Werkzeugaktivitäten offenlegen. Genau dieses Objekt oder dieser Übertragungsweg enthält Agenten-Checkpoint-Historie. Prüfe Inhalt und Empfänger vor dem Senden ausdrücklich; das lokale Anzeigen von Prompts oder Sitzungen veröffentlicht selbst niemals Transkripte.

interaction-guards-push-tag-secret = Wahrscheinliches Geheimnis in der Nachricht eines annotierten Tags; der Wert wird zurückgehalten. Erstelle den Tag vor dem Push neu.

interaction-guards-push-metadata-limit = Die Metadaten der ausgehenden Commits überschreiten die begrenzte Vorschaugröße; kürze große Nachrichten oder teile den Push auf.

interaction-guards-push-metadata-unsafe = Die Commit-Metadaten konnten nicht sicher gelesen werden.

interaction-guards-push-unsigned = Der Commit enthält keine eingebettete Signatur; dieses Profil signiert Commits.

interaction-guards-push-wip = In der ausgehenden Historie ist noch eine vorläufige WIP-/fixup-Nachricht enthalten.

interaction-guards-push-message-secret = Wahrscheinliches Geheimnis in der Commit-Nachricht (Wert zurückgehalten).

interaction-guards-push-finding-limit = Mehr als 2.000 Befunde müssen geprüft werden. Repariere diese Historie oder teile sie vor dem Push auf.

interaction-guards-push-object-limit = Mehr als 50.000 ausgehende Objekte müssen geprüft werden. Teile den Push auf oder rufe das genaue Ziel ab.

interaction-guards-push-blob-size = Unbekannte Größe des ausgehenden Blobs.

interaction-guards-push-lfs-hash-unsupported = Nicht unterstützter LFS-Objekt-Hash.

interaction-guards-push-lfs-hash-malformed = Fehlerhafter LFS-Objekt-Hash.

interaction-guards-push-config-changed = Die Push-Konfiguration hat sich während der Prüfung geändert. Öffne die Vorprüfung erneut.

interaction-guards-push-source-changed = Die Quelle hat sich während der Prüfung geändert. Öffne die Vorprüfung erneut.

interaction-guards-push-review-changed = Der gewählte Push-Endpunkt, Commit oder die angekündigte Lease hat sich geändert. Rufe bei Bedarf das genaue Ziel ab und öffne dann die ursprüngliche Push-Aktion erneut; es wurde nichts gesendet.

interaction-guards-push-object-invalid = Ungültiges ausgehendes Objekt.

interaction-guards-push-scan-failed = Die Inhaltsprüfung ist fehlgeschlagen oder hat ihre Befundgrenze überschritten; es wurde kein Push ausgeführt.

interaction-guards-push-reopen = Öffne die Push-Vorprüfung erneut.

interaction-guards-push-expired = Diese Vorschau ist abgelaufen; öffne die Push-Vorprüfung erneut.

interaction-guards-push-lfs-working = Committe oder entferne Änderungen an .lfsconfig und entferne dort skip-/assume-unchanged-Markierungen, bevor du einen Push prüfst.

interaction-guards-push-lfs-differs = Die Arbeitskopie von .lfsconfig weicht vom geprüften Commit ab. Committe diese Routing-Änderung oder lege sie vor dem Push im Stash ab.

interaction-guards-push-branch-moved = Der ausgecheckte Branch hat sich verschoben. Öffne die Push-Vorprüfung erneut.

interaction-guards-push-state-changed = Branch, Checkout, Identität oder Push-Konfiguration haben sich geändert. Öffne die Push-Vorprüfung erneut; es wurde nichts gepusht.

interaction-guards-push-addresses-changed = Die Push-Adressen haben sich geändert. Öffne die Push-Vorprüfung erneut.

interaction-guards-push-force-fetch = Rufe vor einem erzwungenen Push genau dieses Ziel ab: Es wurde keine Remote-Lease geprüft.

interaction-guards-push-not-ancestor = Das abgerufene Ziel ist kein Vorfahr. Behalte seine Commits bei oder wähle bewusst force-with-lease.

interaction-guards-push-echo-mismatch = Die Prüferbestätigung gehört zu einem anderen genauen Push; bereite ihn erneut vor.

interaction-guards-push-route-missing = Der geprüfte Übertragungsweg wurde nicht erfasst.

interaction-guards-push-completed = Der Push zu den geprüften Zielen wurde abgeschlossen.

interaction-guards-push-clock = Keine Zeitangabe verfügbar.

interaction-guards-push-prepare-fix = Die Historienreparatur konnte nicht vorbereitet werden.

interaction-guards-push-prepare-remote-fix = Die Reparatur der Remote-Historie konnte nicht vorbereitet werden.

interaction-guards-push-no-fix-folder = Kein Reparaturordner vorhanden.

interaction-guards-push-checkout-fix = Checke diesen Branch aus, bevor du seine Historie reparierst, und öffne danach die Vorprüfung erneut.

interaction-guards-push-clean-fix = Committe alle Arbeitsänderungen und nicht verfolgten Dateien oder lege sie im Stash ab, bevor du die ausgehende Historie umschreibst.

interaction-guards-push-fetch-first = Rufe zuerst das Ziel ab.

interaction-guards-push-merged = Das abgerufene Ziel wurde zusammengeführt; öffne die Vorprüfung erneut. Strg+Z macht diese Zusammenführung rückgängig.

interaction-guards-push-merge-attention = Die Zusammenführung erfordert Aufmerksamkeit. Löse Konflikte oder brich sie ab, bevor du die Vorprüfung erneut öffnest.

interaction-guards-push-unknown-fix = Unbekannte Historienreparatur.

interaction-guards-push-secrets-before-lfs = Behebe Befunde zu wahrscheinlichen Geheimnissen, bevor du Dateien nach LFS verschiebst; LFS darf kein bekanntes Geheimnis verbergen.

interaction-guards-push-rewrite-failed = Das Umschreiben der Historie ist vor der Branch-Aktualisierung fehlgeschlagen; prüfe die Verfügbarkeit der Signierung und fehlende lokale Objekte.

interaction-guards-push-malformed-commit = Fehlerhafter Commit.

interaction-guards-push-no-tree = Kein Commit-Baum vorhanden.

interaction-guards-push-lfs-regular = Die LFS-Konvertierung unterstützt nur reguläre Dateien; es wurde keine Historie geändert.

interaction-guards-push-quote-attributes = Attribute konnten nicht maskiert werden.

interaction-guards-push-malformed-author = Fehlerhafte Autorenmetadaten.

interaction-guards-push-set-author = Lege vor dem Ändern der Historie den erwarteten Autorennamen und die E-Mail-Adresse des Hosts in den Profileinstellungen fest.

interaction-guards-push-no-commits = Es gibt keine ausgehenden Commits zu reparieren.

interaction-guards-push-fixed = Die ausgehende Historie wurde repariert. Öffne zur Prüfung die Vorprüfung erneut; Strg+Z stellt die vorherige Historie wieder her.

interaction-guards-push-remote-ownership = Die automatische Remote-Reparatur erfordert eine bis zum bestätigten Abschluss gültige Zuständigkeit; es wurde keine Remote-Reparatur gestartet. Vorschau und geprüfter Push bleiben verfügbar.

interaction-guards-push-utf8-metadata = Eine sichere Reparatur erfordert Metadatenpfade in UTF-8.

interaction-guards-push-metadata-controls = Eine sichere Reparatur erfordert Metadatenpfade ohne Steuerzeichen.

interaction-guards-push-repair-refused = Der abgesicherte Git-Reparaturbefehl wurde abgelehnt; Werkzeugdiagnosen wurden zurückgehalten.

interaction-guards-push-index-snapshot = Der ursprüngliche Index konnte nicht gesichert werden.

interaction-guards-push-index-read = Der ursprüngliche Index konnte nicht gelesen werden.

interaction-guards-push-index-limit = Die Index-Wiederherstellungssicherung überschreitet 64 MiB.

interaction-guards-push-index-save = Die zur Reparatur gehörende Index-Wiederherstellungssicherung konnte nicht gespeichert werden.

interaction-guards-push-index-dependency = Die gespeicherte Index-Abhängigkeit war kein absoluter Pfad; die Reparatur wurde abgelehnt.

interaction-guards-push-git-capability = Eine abgesicherte Reparatur erfordert Git 2.50 oder neuer und einen erfolgreichen nativen HEAD-/Referenz-Fähigkeitsnachweis. Es wurde keine Reparatur der Referenzen gestartet.

interaction-guards-push-backend-proof = Für dieses Git-Referenz-/Objekt-Backend kann die Sicherheit automatischer Reparaturen nicht nachgewiesen werden.

interaction-guards-push-direct-branch = Eine automatische Reparatur erfordert eine direkte lokale Branch-Referenz.

interaction-guards-push-absolute-metadata = Git hat keine absoluten Metadatenpfade geliefert; die sichere Reparatur wurde abgelehnt.

interaction-guards-push-index-flags = Entferne vor der automatischen Baumreparatur die Index-Markierungen sparse/skip-worktree/assume-unchanged.

interaction-guards-push-utf8-changed = Eine sichere Reparatur erfordert geänderte Pfade in UTF-8.

interaction-guards-push-create-evidence = Die zur Reparatur gehörenden Nachweise konnten nicht angelegt werden.

interaction-guards-push-index-utf8 = Der zur Reparatur gehörende Indexpfad ist nicht in UTF-8.

interaction-guards-push-recovery-write = Die Wiederherstellungsanweisungen konnten nicht geschrieben werden.

interaction-guards-push-hook-write = Der private Reparatur-Hook konnte nicht geschrieben werden.

interaction-guards-push-hook-protect = Der Reparatur-Hook konnte nicht geschützt werden.

interaction-guards-push-arm-local = Die lokale Reparatur konnte nicht aktiviert werden.

interaction-guards-push-capability-input = Die Eingabe für den Fähigkeitsnachweis konnte nicht geschrieben werden.

interaction-guards-push-arm-capability = Der Fähigkeitsnachweis konnte nicht aktiviert werden.

interaction-guards-push-hook-command-proof = Der zur Reparatur gehörende Hook-Befehlsnachweis konnte nicht geschrieben werden.

interaction-guards-push-capability-protect = Der zur Reparatur gehörende Fähigkeits-Hook konnte nicht geschützt werden.

interaction-guards-push-hook-stdin = Der Nachweis der Hook-Standardeingabe konnte nicht vorbereitet werden.

interaction-guards-push-hook-support = Git hat die Unterstützung von Befehl und Standardeingabe des ursprünglichen Hooks nicht nachgewiesen; es wurde keine Benutzeraktualisierung gestartet.

interaction-guards-push-native-proof = Dieses Git/Backend hat die Auswahl privater Hooks und eine atomare Aktualisierung von aktuellem HEAD und Branch nicht nachgewiesen. Es wurde keine Aktualisierung von Benutzerreferenzen gestartet.

interaction-guards-push-missing-proof = Der Eingabenachweis der nativen Transaktion fehlt.

interaction-guards-push-transaction-proof = Das Ergebnis der nativen HEAD-/Branch-Transaktion wurde nicht nachgewiesen; es wurde keine Benutzeraktualisierung gestartet.

interaction-guards-push-capability-ack = Keine Bestätigung der nativen Checkout-/Referenz-Fähigkeit; die Reparatur wurde vor dem Start abgelehnt.

interaction-guards-push-remote-metadata-result = Das Metadatenergebnis der Remote-Reparatur konnte nicht festgestellt werden; es wurde keine Reparaturtransaktion gestartet.

interaction-guards-push-remote-metadata = Die Metadaten der Remote-Reparatur sind nicht verfügbar oder zu groß.

interaction-guards-push-remote-absolute = Die Remote-Reparatur erfordert absolute Metadatenpfade auf dem Host.

interaction-guards-push-remote-git = Eine abgesicherte Remote-Reparatur erfordert Git 2.50 oder neuer auf dem Repository-Host.

interaction-guards-push-repair-oid = Die Objektidentität für die Reparatur ist ungültig.

interaction-guards-push-shared-absolute = Der Pfad zum gemeinsamen Index ist nicht absolut.

interaction-guards-push-remote-backend = Nicht unterstütztes Remote-Reparatur-Backend.

interaction-guards-push-remote-utf8 = Die Remote-Reparatur erfordert geänderte Pfade in UTF-8.

interaction-guards-push-wrapper-limit = Der Wrapper der Remote-Reparatur überschreitet seine Größenbegrenzung.

interaction-guards-push-index-parent = Kein übergeordneter Indexordner vorhanden.

interaction-guards-push-pending = Schließe ausstehende Remote-Reparaturen ab, bevor du eine weitere startest.

interaction-guards-push-identity = Die Autoren-E-Mail weicht von der Profilrichtlinie für { $target } ab.

interaction-guards-push-large = Git-Blob mit { $size }; erwäge Git LFS.

interaction-guards-push-secret-line = Wahrscheinliches Geheimnis in Zeile { $line }; der Wert wird zurückgehalten.

interaction-guards-push-remote-history = Ein erzwungener Push würde { $count } Remote-Commits verwerfen, davon { $others } von anderen Autoren. Grundlage ist das abgerufene Ziel; die Lease muss weiterhin übereinstimmen.

interaction-guards-push-local-uncertain = Die lokale Reparatur hat keine vollständige Bestätigung ihrer Zuständigkeit erhalten. Bewahre gleichzeitige Änderungen auf; prüfe recovery.txt und den gespeicherten Index samt Abhängigkeit in { $directory }. Kein automatisches Zurücksetzen hat spätere Arbeit überschrieben.

interaction-guards-push-remote-held = Für eine frühere Remote-Reparatur liegt kein bestätigtes Endergebnis vor. Prüfe nach Abschluss auf dem Host erneut oder untersuche die Wiederherstellung in { $directory }. Es wurde keine Ersatzreparatur gestartet.

interaction-guards-push-remote-uncertain = Das Ergebnis der Remote-Reparatur ist unbekannt. Sie läuft möglicherweise noch; ihre zugehörige Absicherung und Wiederherstellung bleiben in { $directory }. Prüfe nach Abschluss auf dem Host erneut; es wurde weder ein Ersatz noch eine Bereinigung gestartet.

interaction-guards-push-remote-incomplete = Die Remote-Reparatur hat keinen vollständigen Erfolg bestätigt. Prüfe den tatsächlichen HEAD, gleichzeitige Änderungen und die Wiederherstellung in { $directory }; kein Zurücksetzen hat spätere Arbeit überschrieben.

interaction-guards-push-authentication-failed = Die Authentifizierung für das geprüfte Push-Ziel ist fehlgeschlagen.

interaction-guards-push-local-target = Lokales Repository: { $path }

interaction-guards-push-sensitive-path = [sensibler Pfad]

interaction-guards-drift-no-base = Es gibt keinen Commit des Hauptbranches, mit dem der Worktree verglichen werden kann.

interaction-guards-fence-no-base = Es gibt keinen Hauptbranch für den Vergleich.

interaction-guards-fence-no-history = Es gibt keine gemeinsame Historie mit dem Hauptbranch.

interaction-guards-fence-no-folder = Der Ordner des Worktrees ist nicht vorhanden.

interaction-guards-fence-not-object = .claude/settings.local.json ist kein JSON-Objekt.

interaction-guards-fence-permissions-object = „permissions“ in .claude/settings.local.json ist kein Objekt.

interaction-guards-fence-deny-list = „permissions.deny“ in .claude/settings.local.json ist keine Liste.

interaction-guards-collision-stopped = Angehalten

interaction-guards-collision-not-merged = Nicht zusammengeführt

interaction-guards-collision-needs-git = Die Kollisionsmatrix benötigt Git 2.38 oder neuer (git merge-tree --write-tree).

interaction-guards-authorship-read-all = Die Commit-Urheberschaft konnte nicht gelesen werden: { $detail }

interaction-guards-authorship-read-one = Die Urheberschaft von { $oid } konnte nicht gelesen werden: { $detail }

interaction-guards-authorship-figures-error = Die Urheberschaft auf { $branch } konnte nicht ermittelt werden: { $detail }

interaction-guards-authorship-not-branches = { $branch } oder { $base } ist kein Branch.

interaction-guards-authorship-not-commit = { $oid } ist kein Commit.

interaction-guards-drift-measure-error = Die Änderungen des Worktrees konnten nicht vermessen werden: { $detail }

interaction-guards-drift-not-revision = { $reference } ist keine Revision.

interaction-guards-drift-git-folder = Der Git-Ordner des Worktrees konnte nicht gefunden werden: { $detail }

interaction-guards-drift-read-task = Die Aufgabe des Worktrees konnte nicht gelesen werden: { $detail }

interaction-guards-drift-save-task = Die Aufgabe des Worktrees konnte nicht gespeichert werden: { $detail }

interaction-guards-drift-unsupported-forge = Issues von { $forge } werden noch nicht gelesen.

interaction-guards-drift-issue-error = Issue #{ $issue } konnte nicht gelesen werden: { $detail }

interaction-guards-drift-missing-tools = Kein Kommandozeilenprogramm eines Coding-Agenten ist installiert ({ $tools }): Die obigen Zahlen sind der exakt ermittelte Teil.

interaction-guards-drift-run-error = { $tool } konnte nicht ausgeführt werden: { $detail }

interaction-guards-drift-wait-error = Auf { $tool } konnte nicht gewartet werden: { $detail }

interaction-guards-drift-timeout = { $tool } hat länger als { $seconds } s benötigt.

interaction-guards-drift-tool-failed = { $tool } ist fehlgeschlagen: { $detail }

interaction-guards-fence-no-exceptions = { $pattern }: Eine Agentensperre erlaubt keine Ausnahmen (`!`); liste stattdessen die gesperrten Pfade auf.

interaction-guards-fence-line-break = { $entry } enthält einen Zeilenumbruch.

interaction-guards-fence-check-error = { $target } konnte nicht gegen die Agentensperre geprüft werden: { $detail }

interaction-guards-fence-not-revisions = { $base } oder { $tip } ist keine Revision.

interaction-guards-fence-invalid-json = .claude/settings.local.json enthält kein gültiges JSON: { $detail }

interaction-guards-fence-git-folder = Der Git-Ordner des Worktrees konnte nicht gefunden werden: { $detail }

interaction-guards-fence-read-error = { $path } konnte nicht gelesen werden: { $detail }

interaction-guards-fence-folder-error = Der Ordner von { $path } konnte nicht angelegt werden: { $detail }

interaction-guards-fence-write-error = { $path } konnte nicht geschrieben werden: { $detail }

interaction-guards-fence-reach-error = Der Ordner des Worktrees konnte nicht erreicht werden: { $detail }

interaction-guards-fence-record-error = Die geschriebenen Regeln konnten nicht protokolliert werden: { $detail }

interaction-guards-fence-read-fence = Die Agentensperre konnte nicht gelesen werden: { $detail }

interaction-guards-fence-save-fence = Die Agentensperre konnte nicht gespeichert werden: { $detail }

interaction-guards-fence-save-targets = Die Schreibziele der Agentensperre konnten nicht gespeichert werden: { $detail }

interaction-guards-collision-not-branch = { $reference } ist kein Branch.

interaction-guards-collision-invalid-pair = { $a } konnte nicht mit { $b } im Speicher zusammengeführt werden: Eines davon ist weder ein Branch noch ein Commit.

interaction-guards-collision-no-history = { $a } und { $b } haben keine gemeinsame Historie.

interaction-guards-collision-merge-error = { $a } konnte nicht mit { $b } im Speicher zusammengeführt werden: { $detail }

interaction-guards-collision-merge-empty = { $a } konnte nicht mit { $b } im Speicher zusammengeführt werden.

interaction-guards-weakened-read-error = Die Änderungen konnten nicht auf abgeschwächte Prüfungen untersucht werden: { $detail }

interaction-guards-weakened-list-error = Die nicht verfolgten Dateien konnten nicht zur Suche nach abgeschwächten Prüfungen aufgelistet werden: { $detail }

interaction-guards-weakened-size-error = Diese Änderungen sind zu groß für die Suche nach abgeschwächten Prüfungen ({ $megabytes } MB).

interaction-guards-authorship-source-git-ai = git-ai-Notizen

interaction-guards-authorship-source-trace = Agent Trace

interaction-guards-authorship-source-author = Agentenkonto als Autor

interaction-guards-authorship-source-trailer = Co-authored-by-Trailer

interaction-guards-authorship-agent = Von einem Agenten geschrieben; Quelle: { $source }

interaction-guards-authorship-agent-agents = Von einem Agenten geschrieben ({ $agents }); Quelle: { $source }

interaction-guards-authorship-agent-lines = Enthält von Agenten geschriebene Zeilen; Quelle: { $source }

interaction-guards-authorship-agent-lines-agents = Enthält von Agenten geschriebene Zeilen ({ $agents }); Quelle: { $source }

interaction-guards-authorship-mixed = Von einem Menschen und einem Agenten gemeinsam geschrieben; Quelle: { $source }

interaction-guards-authorship-mixed-agents = Von einem Menschen und einem Agenten gemeinsam geschrieben ({ $agents }); Quelle: { $source }

interaction-guards-authorship-mixed-lines = Enthält Zeilen von Agenten und Menschen; Quelle: { $source }

interaction-guards-authorship-mixed-lines-agents = Enthält Zeilen von Agenten und Menschen ({ $agents }); Quelle: { $source }

interaction-guards-authorship-human = Von einem Menschen geschrieben; Quelle: { $source }

interaction-guards-authorship-human-agents = Von einem Menschen geschrieben ({ $agents }); Quelle: { $source }

interaction-guards-authorship-unknown = Urheberschaft nicht erfasst; Quelle: { $source }

interaction-guards-authorship-unknown-agents = Urheberschaft nicht erfasst ({ $agents }); Quelle: { $source }

interaction-guards-authorship-no-lines = { $branch } fügt seit der Abzweigung von der Basis keine Zeilen hinzu.

interaction-guards-authorship-headline = { $branch }: { $percent }% von Agenten geschrieben ({ $agent } von { $total } hinzugefügten Zeilen in { $commits ->
        [one] einem Commit
       *[other] { $commits } Commits
    }){ $has_agent ->
        [yes] , davon { $edited }% seitdem von einem Menschen bearbeitet
       *[no] { "" }
    }{ $has_mixed ->
        [yes] , { $mixed }% von einem Menschen und einem Agenten
       *[no] { "" }
    }{ $has_unknown ->
        [yes] , { $unknown }% ohne erfasste Urheberschaft
       *[no] { "" }
    }{ $partial ->
        [yes] { " (nur die neuesten Commits)" }
       *[no] { "" }
    }

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

interaction-guards-fence-how-committed = committet

interaction-guards-fence-how-uncommitted = nicht committet

interaction-guards-fence-how-untracked = unverfolgt

interaction-guards-fence-how-ignored = von Git ignoriert

interaction-guards-weakened-reason-assertions = Entfernt Assertions

interaction-guards-weakened-reason-tests = Entfernt Tests

interaction-guards-weakened-reason-file = Löscht eine Testdatei

interaction-guards-weakened-reason-skip = Überspringt, deaktiviert oder fokussiert Tests

interaction-guards-weakened-reason-trivial = Fügt eine Assertion hinzu, die nicht fehlschlagen kann

interaction-guards-weakened-reason-tolerance = Lockert eine Toleranz

interaction-guards-weakened-reason-snapshot = Schreibt einen Snapshot vollständig um

interaction-guards-weakened-reason-ci-failure = Lässt CI trotz Fehlern bestehen

interaction-guards-weakened-reason-ci-job = Entfernt einen CI-Job oder Auslöser

interaction-guards-weakened-reason-hooks = Überspringt Git-Hooks

interaction-guards-weakened-badge = schwächt Prüfungen ab

interaction-guards-weakened-line = Zeile { $line }: { $source }

interaction-guards-weakened-old-line = Alte Zeile { $line }: { $source }

interaction-guards-weakened-line-detail = Zeile { $line } ({ $detail }): { $source }

interaction-guards-weakened-old-line-detail = Alte Zeile { $line } ({ $detail }): { $source }

interaction-guards-weakened-trigger = den Auslöser `{ $key }`

interaction-guards-weakened-job = den Job `{ $key }`

interaction-guards-weakened-deletes = Löscht { $path }

interaction-guards-weakened-deletes-snapshot = Löscht den Snapshot { $path }

interaction-guards-weakened-replaces = Ersetzt { $removed } von { $old } Zeilen

interaction-guards-weakened-more = und { $count } weitere

interaction-guards-weakened-heading = { $reason }:


interaction-guards-fence-record = das Sperrregelprotokoll

interaction-guards-push-lfs-attributes-file = .gitattributes ist keine reguläre Datei; behebe dies vor der Konvertierung nach LFS.

interaction-guards-push-lfs-no-pointer = git-lfs hat keinen Zeiger erzeugt; installiere git-lfs, bevor du die Historie konvertierst.

interaction-guards-push-default-nothing = push.default=nothing: Wähle einen ausdrücklichen Branch-Push.

interaction-guards-push-lfs-filter = .lfsconfig darf bei der Prüfung der Endpunktkonfiguration keinen Inhaltsfilter durchlaufen.
