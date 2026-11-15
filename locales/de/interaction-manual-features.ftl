# Full manual FEATURES prose; source checkpoint e8a6f4fb37c808998798195d0272ee2d570a091c.
# IDs are permanent: keep paragraph numbers stable when adding or moving content.
# Commands, paths, refs, config keys, sample content and written key gestures stay literal.

# Repositories and tabs
interaction-manual-feature-repositories-tabs-title = Repositorys und Tabs

interaction-manual-feature-repositories-tabs-01 = Über die Startseite kannst du ein Repository öffnen, erstellen oder klonen. Zuletzt verwendete Repositorys stehen dort mit ihrem Branch (oder dem Commit bei losgelöstem HEAD), einem Punkt bei nicht committeten Änderungen und dem Zeitpunkt des letzten Öffnens. Bei einem Repository auf einem anderen Rechner wird der Branch nur angezeigt, solange eine Verbindung dorthin besteht; dafür allein wird keine Verbindung aufgebaut.

interaction-manual-feature-repositories-tabs-02 = Wenn es zuletzt verwendete Repositorys gibt, kannst du auf der Startseite tippen, um sie nach Name oder Pfad zu filtern. ↑ / ↓ wählen eines aus, Enter öffnet es, Esc leert den Filter.

interaction-manual-feature-repositories-tabs-03 = Ziehe einen Ordner auf das Fenster, um sein Repository zu öffnen. Ziehe eine .patch-, .diff-, .eml- oder .mbox-Datei auf einen Repository-Tab, um sie anzuwenden (eine Mail als Commits, wie bei „Patch anwenden“). Mehrere gleichzeitig abgelegte Dateien werden nacheinander angewendet, jeweils im Tab, auf dem sie abgelegt wurden. Schlägt eine fehl, werden die folgenden nicht angewendet.

interaction-manual-feature-repositories-tabs-04 = „Ordner nach Repositorys durchsuchen“ auf der Startseite durchsucht einige Ordnerebenen, überspringt dabei versteckte Ordner, node_modules, target und Ähnliches und listet gefundene Repositorys auf. Du kannst eines öffnen oder bisher nicht aufgeführte zur Liste der zuletzt verwendeten hinzufügen, soweit Platz ist; dafür wird die Liste nie geleert. „Stoppen“ behält die bisherigen Funde.

interaction-manual-feature-repositories-tabs-24 = Alle Repositorys (Startseite, unter Zuletzt) führt jedes für das Profil gefundene Repository auf: in Repository-Stammordnern (Repository-Stammordner… oder „Als Stammordner speichern“ nach einer Suche; jeder wird bis zu seiner eigenen Tiefe durchsucht, standardmäßig 1) und als „In der Nähe“ die anderen Repositorys im Ordner jedes Repositorys, das du öffnest (eine Ebene, nie dein Benutzerordner, ein Laufwerk oder ein anderer Rechner). Filtere nach Alle, Zuletzt, In der Nähe oder einem Stammordner und suche wie unter Zuletzt; eine Zeile öffnet ihr Repository (das erst dann unter Zuletzt landet), und das Auge blendet es dauerhaft aus der Liste aus. Die Suchen laufen im Hintergrund, höchstens zwei gleichzeitig, innerhalb von Ordner-, Repository- und Zeitgrenzen; eine unvollständige behält, was sie hatte, als veraltet markiert. Für ein gefundenes Repository wird nichts abgeholt und kein git ausgeführt, bis es geöffnet wird.

interaction-manual-feature-repositories-tabs-05 = Arbeitsbereiche (+ neben ARBEITSBEREICHE auf der Startseite) sind benannte Gruppen von Repositorys auf diesem oder anderen Rechnern. Jede Zeile zeigt Branch, nicht committete Dateien, voraus / zurück und den letzten Fetch. Ein Klick öffnet das Repository; beim Darüberfahren erfährst du, warum eine Abfrage oder ein Fetch scheiterte. Auf einem anderen Rechner wird nur ungefragt nachgesehen, solange gitcraft dorthin verbunden ist.

interaction-manual-feature-repositories-tabs-06 = Bei einem Arbeitsbereich fügt + ein offenes oder zuletzt verwendetes Repository oder einen anderen Ordner hinzu. ↻ fragt erneut ab und verbindet sich bei Bedarf. Die Wolke fetcht alle Repositorys ohne Passwortabfrage; benötigt eines eine Anmeldung, steht das dabei. Der Ordner öffnet alle in Tabs, das Zahnrad benennt den Arbeitsbereich um oder löscht ihn. ✕ in einer Zeile (beim Darüberfahren) entfernt das Repository daraus.

interaction-manual-feature-repositories-tabs-07 = „Wo ist meine Arbeit?“ (Startseite neben ARBEITSBEREICHE oder in der Befehlspalette) prüft alle gitcraft bekannten Repositorys: zuletzt verwendete aller Profile, offene Tabs, Arbeitsbereiche und auf einem Rechner gefundene. Gesucht werden nicht gepushte Commits, Commits in Repositorys ohne Remote, Stashes, nicht committete Änderungen hier oder in anderen Worktrees und Branches, deren Remote-Branch verschwunden ist, solange sie Commits haben, die auf keinem Remote liegen. „Pushen“ sendet einen Branch ohne Passwortabfrage an seinen gleichnamigen Upstream oder an origin und richtet dort das Tracking ein. Ein Branch mit andersnamigem Upstream oder einem Branch desselben Repositorys als Upstream wird im Repository selbst gepusht. „Öffnen“ öffnet das Repository. Auf einem nicht verbundenen Rechner wird erst mit „Verbinden und nachsehen“ nachgesehen und mit „Verbinden und pushen“ gepusht. „Weitere Repositorys suchen“ durchsucht dessen Home-Ordner: hier, in einer WSL-Distribution oder auf einem Host aus ~/.ssh/config.

interaction-manual-feature-repositories-tabs-08 = „Pull Requests für dich“ auf der Startseite zeigt über alle geöffneten Repositorys auf GitHub, GitLab, Forgejo und Gitea hinweg die Pull Requests, die dein Review anfordern, sowie deine eigenen mit fehlgeschlagenen Prüfungen (bei GitLab einer Pipeline) oder ungesehenen Kommentaren anderer. Forgejo und Gitea werden einmal je Host durchsucht, sodass auch ein Pull Request von deinem Fork an dessen Upstream gefunden wird. Ein Klick öffnet ihn im Tab seines Repositorys; GitLab-Merge-Requests und solche aus hier nicht geöffneten Repositorys öffnen sich im Browser. Der bisherige Inhalt gilt dann als gesehen, ebenso der Inhalt eines eigenen Pull Requests beim ersten Erfassen im Posteingang. Solange die Startseite sichtbar ist, wird alle fünf Minuten erneut abgefragt. Die ersten acht werden angezeigt, „Mehr anzeigen“ öffnet die übrigen. Beim Darüberfahren erfährst du, welche Forges nicht abgefragt werden konnten. Fehlt das Werkzeug oder eine Anmeldung, wird die Forge erst mit ↻ erneut abgefragt. ↻ fragt sofort ab, auch Repositorys auf nicht verbundenen Rechnern. Verwendet werden die Anmeldungen von gh und glab, bei Forgejo und Gitea GITEA_SERVER_URL / GITEA_SERVER_TOKEN, tea oder eine bereits in einem Tab verwendete Anmeldung. Bei automatischen Abfragen wird nie Gits Credential-Helper ausgeführt, der zum Entsperren eines Schlüsselbunds auffordern könnte.

interaction-manual-feature-repositories-tabs-09 = Die Befehlspalette exportiert Git-Bundles ausgewählter Refs oder eines inkrementellen Bereichs und prüft lokale Bundle-Header und Voraussetzungen, ohne Objekte zu entpacken oder Refs anzulegen. Der Import bleibt zurückgestellt, bis Ressourcen für nicht vertrauenswürdige Packs begrenzt werden können. Dateiauswahldialoge verwenden die Platte dieses Rechners; bei SSH/WSL wird eine temporäre Kopie geprüft. Grenzen: 128 MiB, 1000 Header-Einträge und zwei Minuten je Git-Aufruf. Pack-Inhalte sind ungeprüft.

interaction-manual-feature-repositories-tabs-10 = Ein Tab pro Repository. Änderungen aus Editor oder Terminal erscheinen von selbst.

interaction-manual-feature-repositories-tabs-11 = Mit einem Rechtsklick auf einen Tab kannst du ihm ein eigenes Symbol und eine Hervorhebungsfarbe geben.

interaction-manual-feature-repositories-tabs-12 = „Diagnose kopieren“ oben in diesem Handbuch oder „Hilfe: Diagnose kopieren“ in der Befehlspalette legt die Angaben für einen Fehlerbericht in die Zwischenablage: Version und Kanal von gitcraft, System, Grafikadapter, git, gh, glab und tea (angemeldet oder nicht, niemals ein Token) sowie die wirksamen Git-Einstellungen, ohne Namen, E-Mail-Adresse, Signaturschlüssel, Pfade oder Home-Ordner. Prüfe den Inhalt vor dem Veröffentlichen.

interaction-manual-feature-repositories-tabs-13 = Ziehe einen Tab entlang der Leiste, um die Tabs umzuordnen.

interaction-manual-feature-repositories-tabs-14 = Eingaben bleiben je Repository erhalten, wenn gitcraft abstürzt oder geschlossen wird: Commit-Nachricht, ungesendete Pull-Request-Kommentare, Reviews und Antworten sowie eine vorbereitete interaktive Rebase sind beim erneuten Öffnen wieder da. Sie werden entfernt, sobald sie committet, gesendet oder gestartet werden oder du sie leerst oder abbrichst. Nach einem Absturz meldet gitcraft dies beim nächsten Start. Eingaben in Passwort- oder Anmeldedialogen werden nie gespeichert.

interaction-manual-feature-repositories-tabs-15 = Cmd/Ctrl+1…9 wählen Tabs aus; Cmd/Ctrl+Alt+1/2/3 setzen den Fokus auf Seitenleiste, Graph und rechten Bereich. Alle registrierten Tastenkürzel lassen sich in den Einstellungen ändern oder entfernen, auch mehrteilige Tastenfolgen. Für die Hauptaktionen gibt es Kürzel: Fetch, Pull, Push, Stash, neuer Branch, Terminal, alles stagen und committen sowie zum Wechseln der Tabs (unten und unter Einstellungen → Tastenkürzel aufgeführt). Tooltips, Befehlspalette und Menüs zeigen sie an. Solange ein Textfeld oder das Terminal den Tastaturfokus hat, lösen sie nichts aus; Tippen startet also keine Aktion. Nur Ctrl+Enter in der Commit-Nachricht committet.

interaction-manual-feature-repositories-tabs-16 = Unter einem ausgegrauten Menüeintrag steht der Grund dafür.

interaction-manual-feature-repositories-tabs-17 = Ein Fenster: Wird gitcraft während des Betriebs erneut gestartet (gitcraft <folder>, „In gitcraft öffnen“, der Menüeintrag), öffnet es den Ordner in einem Tab des vorhandenen Fensters oder wechselt zum vorhandenen Tab und holt das Fenster nach vorn. Verhindert das System dies, blinkt dessen Taskleistenschaltfläche. Ohne Ordner wird es nur nach vorn geholt.

interaction-manual-feature-repositories-tabs-18 = Ein Repository ohne Commits zeigt im Graphen die ersten Schritte: ersten Commit erstellen, Remote hinzufügen, veröffentlichen.

interaction-manual-feature-repositories-tabs-19 = Unter dem Graphen erscheint täglich ein anderer „Schon gewusst?“-Tipp. Ein Hinweis erscheint einmalig, wenn du erstmals etwas tust, für das es in der Nähe ein Tastenkürzel gibt.

interaction-manual-feature-repositories-tabs-20 = Das Auge neben einem Tipp schaltet Tipps und Hinweise aus. Einstellungen → Allgemein → Tipps schaltet sie wieder ein und kann bereits gesehene Hinweise erneut anzeigen.

interaction-manual-feature-repositories-tabs-21 = Beim Start wiederhergestellte Tabs werden erst beim ersten Öffnen eingelesen. Ein Tab, der 30 Minuten ohne Aktivität im Hintergrund liegt (keine Operation, kein laufendes Terminal, keine eingegebene Nachricht, kein offener Editor, Review, Pull Request oder Build), beendet Ordnerüberwachung, Fetches und Build-Verfolgung und verwirft Graph und Diff. Beim Öffnen wird er mit demselben Commit und Diff erneut eingelesen.

interaction-manual-feature-repositories-tabs-22 = „Remote“ auf der Startseite öffnet ein Repository in WSL oder über SSH: Git läuft dort, dieses Fenster zeigt es an. „Durchsuchen…“ listet die Ordner des Rechners auf und markiert Repositorys. Die Werkzeugleiste nennt den Rechner; das Terminal öffnet dort eine Shell. Was ssh abfragt (Passwort, Passphrase, neuer Hostschlüssel), wird in diesem Fenster gefragt. „Verbindung merken“ speichert ein Passwort verschlüsselt im Anmeldedatenspeicher des Systems; „Gespeicherte SSH-Anmeldungen vergessen“ (Ctrl+P) entfernt sie.

interaction-manual-feature-repositories-tabs-23 = Rechtsklick auf einen Tab und „Teleportieren nach…“ (oder Ctrl+P, „Auf einen anderen Rechner teleportieren…“) überträgt seine Arbeit auf diesen Rechner, eine WSL-Distribution oder einen SSH-Host: Branch, gestagte und ungestagte Änderungen sowie unversionierte Dateien kommen dort in einem neuen Worktree an, der in einem Tab geöffnet wird. Ziel ist ein dort gefundener Klon desselben Projekts, ein anderes dortiges Repository, ein zuvor erstellter Klon oder ein neues Repository aus den übertragenen Daten. Hier ändert sich nichts. Push oder Forge sind nicht erforderlich.

# Portable copy
interaction-manual-feature-portable-title = Portable Kopie

interaction-manual-feature-portable-01 = Eine leere Datei gitcraft.portable oder ein leerer Ordner data neben gitcraft (unter macOS neben gitcraft.app) lässt gitcraft mit frischen Daten starten und Einstellungen, offene Tabs, Profile samt Git-Konfigurationsdateien, Avatar-Cache, heruntergeladene Updates und Mergiraf in diesem data-Ordner aufbewahren. Einstellungen → Allgemein → Portabler Modus zeigt den Ort.

interaction-manual-feature-portable-02 = Eine portable Kopie speichert SSH-Passwörter nur dann im Anmeldedatenspeicher dieses Computers, wenn „SSH-Anmeldungen im Anmeldedatenspeicher dieses Computers merken“ eingeschaltet ist (Einstellungen → Allgemein → Portabler Modus; anfangs ausgeschaltet). Andernfalls werden sie nach einem Neustart erneut abgefragt.

# Command palette
interaction-manual-feature-palette-title = Befehlspalette

interaction-manual-feature-palette-01 = Ctrl+P listet alle Aktionen auf; tippe einige Buchstaben einer Aktion. Die besten Treffer kommen zuerst: Buchstaben an Wortanfängen oder direkt hintereinander, etwa reb für Rebase oder cp für Cherry-pick. Die eingegebenen Buchstaben werden hervorgehoben.

interaction-manual-feature-palette-02 = Ohne Eingabe kommen zuerst die Aktionen zur aktuellen Auswahl, danach die zuletzt verwendeten (auch nach einem Neustart). Bei ausgewähltem Commit: Cherry-pick, Revert, main hierher zurücksetzen…, interaktive Rebase darauf sowie Branch, Tag oder Patch daraus. Bei geöffnetem Datei-Diff: stagen, aus der Staging-Area nehmen oder verwerfen, Datei, Verlauf oder Blame ansehen.

interaction-manual-feature-palette-03 = „Branch auschecken…“, „Mergen in…“, „Rebase von … auf…“, „… vergleichen mit…“ und „Anderen Branch pushen…“ wählen den Branch in einem zweiten Schritt mit eigener Suche. Esc führt zum ersten Schritt zurück, ebenso Backspace im leeren Feld.

interaction-manual-feature-palette-04 = Die Eingabe eines Branch-Namens bietet die wenigen am besten passenden Branches zum Auschecken an; „Branch auschecken…“ listet alle auf.

# From the terminal and the file manager
interaction-manual-feature-terminal-file-manager-title = Aus Terminal und Dateimanager

interaction-manual-feature-terminal-file-manager-01 = Mit gitcraft . öffnest du das Repository des aktuellen Ordners, auch aus Unterordnern oder Worktrees. gitcraft <folder> tut dasselbe für den angegebenen Ordner.

interaction-manual-feature-terminal-file-manager-02 = gitcraft log <file> öffnet den Dateiverlauf, gitcraft blame <file>:<line> die Blame-Ansicht an dieser Zeile, gitcraft show <rev> einen Commit und gitcraft compare a..b die unterschiedlichen Dateien (mit a...b die Änderungen seit der Verzweigung). gitcraft --help listet diese Befehle auf. Läuft gitcraft bereits, öffnen sie sich in dessen Fenster.

interaction-manual-feature-terminal-file-manager-03 = Links aus dem Browser: Ein Link gitcraft://open?url=<link> oder der Link selbst, in Befehlspalette oder Startseite eingefügt oder an gitcraft übergeben, öffnet einen Pull Request, Commit oder file#L40 von GitHub, GitLab, Gitea / Forgejo oder Bitbucket in deinem vorhandenen Klon. Gesucht wird in offenen Tabs und zuletzt verwendeten Repositorys; ohne passenden Klon wird das Klonen angeboten.

interaction-manual-feature-terminal-file-manager-04 = Dateien haben im Kontextmenü „Verlauf in gitcraft“ und „Blame in gitcraft“: im Explorer nach Einrichtung durch den Installer, in Dolphin durch die .deb- und .rpm-Pakete sowie im Finder (Schnellaktionen) und in Dateien (Skripte), sobald Einstellungen → Allgemein sie dort hinzufügt.

# Gerrit
interaction-manual-feature-gerrit-title = Gerrit

interaction-manual-feature-gerrit-01 = Für Gerrit setzt du git config gitcraft.forge gerrit und lädst anschließend neu und fetchst. Hosts namens gerrit.… und *.googlesource.com werden automatisch erkannt.

interaction-manual-feature-gerrit-02 = „Zum Review pushen“ wählt Ziel-Branch und optionales Topic, prüft jeden Commit oberhalb des gefetchten Ziels auf eine eindeutige Change-Id und pusht dann genau den in der Vorschau gezeigten Commit nach refs/for/<branch>. Installiere den commit-msg-Hook deines Servers und ergänze fehlende IDs per Amend. Bei auseinanderentwickelten oder gemergten Serien zuerst fetchen und rebasen.

interaction-manual-feature-gerrit-03 = Der Pfeil in einer Graphzeile oder „Gerrit-Change öffnen“ in deren Menü öffnet den Change. „Letzten Commit ändern“ behält seine ursprüngliche Change-Id auch bei neuer Beschreibung. Erneutes Pushen zum Review lädt ein weiteres Patchset hoch.

interaction-manual-feature-gerrit-04 = Für eine andere Webadresse oder ein Reverse-Proxy-Präfix setzt du git config gitcraft.gerritUrl https://review.example.org/gerrit. Git übernimmt Transport und Anmeldung.

# Graph
interaction-manual-feature-graph-title = Graph

interaction-manual-feature-graph-01 = Jeder Branch hat eine Plakette an seinem neuesten Commit: ausgefüllt für den ausgecheckten Branch, mit Laptop für einen lokalen und mit Wolke, wenn ein Remote ihn hat.

interaction-manual-feature-graph-02 = Ein Doppelklick auf eine Plakette checkt den Branch aus und pullt ihn, wenn sein Remote voraus ist. Bei der Remote-Plakette des ausgecheckten Branches wird nur gepullt. Fahre über +N, um alle Branches des Commits zu sehen, und öffne per Rechtsklick deren Menüs.

interaction-manual-feature-graph-03 = Ein Rechtsklick auf einen Commit oder eine Plakette bietet Branches, Tags, Merge, Rebase, Cherry-pick, Revert, Reset (soft, mixed, keep oder hard) und mehr.

interaction-manual-feature-graph-04 = Ein Roboter in einer Commit-Zeile bedeutet, dass ein Agent ihn geschrieben hat (bernsteinfarben: zusammen mit einem Menschen), entsprechend den Angaben des Commits: git-ai-Notizen, Agent Trace, Agentenkonto oder Co-authored-by-Trailer. „Von Agenten geschrieben“ in der Suchleiste führt durch diese Commits. Blame und Diffs zeigen je Zeile einen Streifen; das Branch-Menü nennt den von Agenten geschriebenen Anteil.

interaction-manual-feature-graph-05 = Suche nach Nachrichten, Autoren und Hashes oder wechsle zu „Code“, um Commits zu finden, die bestimmten Text hinzugefügt oder entfernt haben.

interaction-manual-feature-graph-06 = Suchoperatoren grenzen die Suche ein und lassen sich miteinander und mit Text kombinieren: author: und committer: (Name oder E-Mail), path: (Datei oder Ordner, Groß-/Kleinschreibung wie im Repository; *.md funktioniert), after: und before: (Commit-Datum: 2024-05-01, 2024-05, 2024, today, yesterday oder Zeitabstand wie 12h, 3d, 2w, 6m für Monate, 1y), message:, contents: (git log -S), regex: (git log -G), merges: und -merges:. Werte mit Leerzeichen stehen in Anführungszeichen: author:"Jane Doe". Namen und Nachrichten ignorieren die Groß-/Kleinschreibung von A–Z; Änderungen beachten die eingegebene Schreibung, außer wenn auch nach Namen oder Nachrichten gesucht wird. Ein Hash im Text findet weiterhin seinen Commit.

interaction-manual-feature-graph-07 = Beim Eingeben eines Operators erscheinen Vorschläge: Autoren und Committer aus dem Verlauf mit ihren .mailmap-Namen, Dateien und Ordner des ausgecheckten Commits sowie Daten. ↑ / ↓ markieren einen Vorschlag, Tab oder Enter übernehmen ihn, Esc schließt die Liste. Eine fehlerhafte Suche wird unter dem Feld erklärt.

interaction-manual-feature-graph-08 = Ziehe die Grenzen in der Spaltenüberschrift, um Breiten zu ändern. Ihr Kontextmenü wählt die Spalten, macht die Graphspalte kompakt oder setzt die Spalten zurück.

interaction-manual-feature-graph-09 = Die Branchspalte lässt sich bis auf 70 px verkleinern, genug für einen kurzen Namen wie dev. Lange Branch- und Tag-Plaketten behalten standardmäßig ihr Ende (…xyz). Einstellungen → Graph → Gekürzte Branch-Namen kann stattdessen ihren Anfang (xyz…) behalten. In einer schmalen Plakette weichen Symbole dem Namen.

interaction-manual-feature-graph-10 = „Commit aufteilen…“ im Menü eines Commits des ausgecheckten Branches: Wähle links einen der neuen Commits oder drücke seine Nummer und klicke rechts auf Dateien, Hunks oder Zeilen, um sie ihm zuzuordnen. Jeder neue Commit hat seine eigene Nachricht. „Aufteilung prüfen“ zeigt die einzelnen Patches; „Commit aufteilen“ ersetzt den alten Commit und spielt die nachfolgenden erneut ab. Ctrl+Z macht dies rückgängig.

interaction-manual-feature-graph-11 = „Gestagte Korrekturen verteilen…“ über den gestagten Dateien oder in der Befehlspalette ordnet jede gestagte Änderung dem früheren Branch-Commit zu, der ihre Zeilen zuletzt änderte. Eindeutige Treffer werden übernommen, die übrigen vorgeschlagen oder gestagt belassen. Prüfe sie und übernimm sie dann mit Absorb. Ctrl+Z macht dies rückgängig; ein installiertes git-absorb kann es stattdessen erledigen.

interaction-manual-feature-graph-12 = „Änderungen in einen anderen Commit verschieben…“ im selben Menü: Klicke an, was aus dem Commit heraus soll (erneutes Klicken behält es), wähle einen anderen Commit des Branches oder die Arbeitsänderungen (gestagt), prüfe, was bleibt und was wandert, und verschiebe es. Ein dadurch leerer Commit entfällt. Ctrl+Z macht dies rückgängig.

interaction-manual-feature-graph-13 = Klicke auf GRAPH in der Spaltenüberschrift, um den Verlauf topologisch, nach Commit-Datum oder Autorendatum zu ordnen oder „Nur erster Eltern-Commit“ anzuzeigen: einen Merge ohne die von ihm eingebrachten Commits. Die Auswahl wird je Repository gespeichert.

interaction-manual-feature-graph-14 = Bei losgelöstem HEAD weist ein Banner darauf hin und bietet „Hier Branch erstellen“ und „Zurück zum vorherigen Branch“. Wenn du dort committet hast und etwas anderes auscheckst, wird zuerst gefragt und du kannst diese Commits auf einem neuen Branch behalten.

interaction-manual-feature-graph-15 = Interaktive Rebase: Commits umordnen, umbenennen, bearbeiten, zusammenfassen, per Fixup zuordnen oder entfernen und dann starten. Ziehe eine Zeile am Griff, um sie zu verschieben. „Bearbeiten“ hält die Rebase nach diesem Commit an: Stage die gewünschten Änderungen; „Rebase fortsetzen“ ändert den Commit und fährt fort.

interaction-manual-feature-graph-16 = Ziehe die Plakette eines Branches oder seine Zeile links auf einen anderen Branch oder Commit. Ein Menü bietet die dort sinnvollen Aktionen Merge, Rebase, interaktive Rebase oder Fast-forward. Geeignete Ziele leuchten auf; Esc bricht ab.

interaction-manual-feature-graph-17 = Ziehe einen Commit auf den ausgecheckten Branch für Cherry-pick oder auf einen anderen Commit dieses Branches, um ihn darüber oder darunter zu verschieben oder per Squash oder Fixup einzuarbeiten. Ctrl+Z macht das Ablegen wie die entsprechenden Menüaktionen rückgängig.

interaction-manual-feature-graph-18 = Probelauf: Lass den Zeiger auf „Rebase“, „Hard Reset“, „Pull“, „Force Push“, „Löschen“ oder „Commit entfernen“ im Menü oder in der Werkzeugleiste ruhen; mit gedrücktem Alt entfällt die Wartezeit. Der Graph zeigt das Ergebnis: neue Commits blass mit gestrichelten Linien, Plaketten an ihren künftigen Positionen umrandet, verlorene Commits durchgestrichen. Ein Streifen über dem unteren Graphrand listet diese Commits mit Autoren, die betroffenen Arbeitsdateien und die Stelle eines möglichen Konfliktstopps auf. Anschauen ändert nichts; erst die Auswahl führt die Aktion aus.

interaction-manual-feature-graph-19 = Zurück und vorwärts (Alt+← / Alt+→, die entsprechenden Maustasten oder die Pfeile am Anfang der Werkzeugleiste) führen durch die letzten Ansichten des Tabs: Graph mit ausgewähltem Commit, Diffs, Dateiverlauf, Blame oder Dateiinhalt, Pull Requests und Builds. Das Durchklicken von Commits zählt als ein Schritt; einem Link zu Eltern-Commit, Branch oder Tag zu folgen zählt separat.

interaction-manual-feature-graph-20 = Die Bereichsschaltfläche im Diff-Kopf oder Einstellungen → Graph → Diffs zeigt den Diff unter dem Graphen statt an dessen Stelle. Der Graph bleibt bedienbar, der Diff folgt der nächsten geöffneten Datei, und die Grenze dazwischen lässt sich verschieben. Esc schließt den Diff weiterhin.

interaction-manual-feature-graph-21 = Nach einer Rebase, einem Amend oder einer anderen von gitcraft vorgenommenen Umschreibung zeigt „Vorher / nachher vergleichen“ neben der Erfolgsmeldung oder „Vorher / nachher der letzten Umschreibung vergleichen“ im Branch-Menü alte und neue Commits paarweise: unveränderte verblasst, geänderte, hinzugefügte, entfernte und verschobene markiert. Ein Klick auf ein Paar zeigt die Patch-Änderung. Die alten Commits bleiben 30 Tage erhalten.

interaction-manual-feature-graph-22 = „Hotfix von v2.3.1 starten…“ im Menü eines Versions-Tags erstellt dort hotfix/2.3.2. „Hotfix abschließen…“ in dessen Menü oder der Befehlspalette mergt ihn in develop (falls verwendet) und main, taggt den Merge auf main, löscht den Hotfix-Branch und pusht alles. Alles wird vorher gezeigt und ist mit Ctrl+Z rückgängig zu machen, auch bei einem Abbruch nach Teilschritten.

interaction-manual-feature-graph-23 = „Release erstellen…“ im Commit-Menü oder der Befehlspalette schlägt anhand der Conventional Commits seit dem letzten Tag die nächste Version vor und entwirft die Release-Notizen. Es kann die Version in Cargo.toml / package.json und CHANGELOG.md anpassen, einen annotierten oder signierten Tag setzen, pushen und auf der Forge ein Release mit Dateien erstellen. Jeder Schritt wird vorher gezeigt. Ein fehlgeschlagener Schritt lässt sich fortsetzen; Ctrl+Z macht die hier ausgeführten Schritte rückgängig.

interaction-manual-feature-graph-24 = „Beschreibung“ erhält die flexible Breite; „Branch / Tag“ passt sich den sichtbaren Plaketten an, bis du den Rand ziehst. Im Kontextmenü des Kopfes kannst du „An sichtbare Branch-/Tag-Plaketten anpassen“ wiederherstellen, Spalten wählen, den Graphen kompakt machen oder Breiten zurücksetzen. Blasse Linienbeschriftungen kennzeichnen Spitzen außerhalb der Ansicht; über einer Beschreibung erscheint beim Darüberfahren die ganze Nachricht.

# Compare
interaction-manual-feature-compare-title = Vergleichen

interaction-manual-feature-compare-01 = Ctrl+click auf einen zweiten Commit listet die Dateien auf, die sich zwischen beiden unterscheiden; die Änderungszeile steht für den Arbeitsbaum.

interaction-manual-feature-compare-02 = Im Branch-Menü zeigt „Änderungen, die x zu main hinzufügt“ wie ein Pull Request die Änderungen seit der Verzweigung. „main mit x vergleichen“ zeigt sämtliche Unterschiede.

interaction-manual-feature-compare-03 = Bei einem Merge zeigt „Was dieser Merge gelöst hat“ nur die Abweichungen von dem Merge, den Git aus dessen Eltern erstellt: Konfliktlösungen (Hunks mit „löst einen Konflikt“) und darüber hinausgehende Änderungen von Hand oder durch einen Merge mit anderen Optionen („über den Merge hinaus bearbeitet“). Die Kennzeichnung „gelöst“ oder „bearbeitet“ an einer Merge-Zeile im Graphen weist darauf hin; ein Klick zeigt sie. Für die Kennzeichnungen werden die angezeigten Merges im Hintergrund erneut berechnet, außer in Repositorys mit eigenen Merge-Treibern. Erfordert git 2.36 oder neuer; Octopus-Merges mit mehr als zwei Eltern kann Git nicht erneut berechnen.

# Updates
interaction-manual-feature-updates-title = Updates

interaction-manual-feature-updates-01 = Eine neue Version erscheint in einer Leiste unter den Tabs. „Jetzt aktualisieren“ startet mit ihr neu und behält die Tabs.

interaction-manual-feature-updates-02 = Ein Update behält die vorherige Version. Einstellungen → Allgemein → Updates → Zurück zu x.y.z startet mit ihr neu und bleibt dabei, bis du dort „Fixierung aufheben“ wählst.

# Bisect
interaction-manual-feature-bisect-title = Bisect

interaction-manual-feature-bisect-01 = Rechtsklick auf einen Commit und „Bisect hier starten…“ sucht den Commit, der eine Änderung eingeführt hat: Gib an, ob er gut (vor der Änderung) oder schlecht (mit der Änderung) ist, und nenne das andere Ende. HEAD gilt als schlecht, sofern du keinen anderen Commit angibst. Bei nicht committeten Änderungen, Konflikten oder laufendem Merge oder Rebase wird der Start verweigert.

interaction-manual-feature-bisect-02 = Ein Banner über dem Graphen nennt dann den zum Testen ausgecheckten Commit und die ungefähr verbleibenden Schritte. „Gut“, „Schlecht“ und „Überspringen“ markieren ihn; über das Kontextmenü eines anderen Commits markierst du diesen. Im Graphen sind gute Commits grün, schlechte rot und übersprungene grau.

interaction-manual-feature-bisect-03 = „Befehl ausführen…“ im Banner lässt git bisect run jeden Commit auf dem Rechner des Repositorys testen: hier, in WSL oder über SSH. Exitcode 0 ist gut, 125 überspringt den Commit, jeder andere bis 127 ist schlecht, ab 128 wird gestoppt. Die Ausgabe erscheint fortlaufend unter dem Banner. „Lauf stoppen“ beendet ihn; Bisect bleibt an der erreichten Stelle.

interaction-manual-feature-bisect-04 = Am Ende wird der erste schlechte Commit ausgewählt und seine Detailansicht geöffnet. „Abschließen“ oder vor dem Ende „Stoppen“ nach Rückfrage führt git bisect reset aus und checkt wieder den Stand vom Beginn aus.

interaction-manual-feature-bisect-05 = Eine im Terminal gestartete Bisect-Suche erscheint im Banner, sobald gitcraft sie bemerkt, und lässt sich dort fortsetzen. Jeder Worktree hat seine eigene.

# Parallel bisect
interaction-manual-feature-parallel-bisect-title = Paralleles Bisect

interaction-manual-feature-parallel-bisect-01 = „Parallel ausführen…“ im Bisect-Banner, „Ab hier parallel bisecten…“ im Commit-Menü oder die Befehlspalette testet mehrere Commits gleichzeitig, jeweils in einem eigenen Worktree, bis der erste schlechte Commit gefunden ist. Das dauert etwa log zur Basis (k + 1) von n Runden statt log zur Basis 2 von n. Dein Checkout bleibt unangetastet; es muss kein git bisect laufen.

interaction-manual-feature-parallel-bisect-02 = Gib schlechten und guten Commit, Testbefehl, Rechner (diesen Computer, eine WSL-Distribution oder einen SSH-Host aus ~/.ssh/config), Parallelität und Zeitlimit je Test an. Der Befehl wird wie bei git bisect run ausgewertet: Exitcode 0 ist gut, 125 überspringt, jeder andere bis 127 ist schlecht. Ab 128, also bei einem Absturz, wird an einem Ende gestoppt, sonst der Commit mit Warnung übersprungen. Die Parallelität entspricht standardmäßig der Kernzahl des Rechners, bis zu 16; höchstens 32 sind möglich. Ein Test über dem Zeitlimit wird gestoppt und gilt als übersprungen.

interaction-manual-feature-parallel-bisect-03 = Auf einem anderen Rechner als dem des Repositorys werden die Commits zunächst dorthin übertragen: in einen von dir angegebenen Klon oder eine von gitcraft unter ~/.cache/gitcraft/bisect verwaltete Kopie.

interaction-manual-feature-parallel-bisect-04 = Beide Enden werden in der ersten Runde zusammen mit den ersten ausgewählten Commits getestet. Schlägt bereits das gute Ende fehl oder besteht das schlechte, wird dies gemeldet und gestoppt. Das Banner zeigt Runde, Anzahl möglicher erster schlechter Commits und Anzahl laufender Tests. Der Graph markiert laufende Tests und färbt getestete Commits grün, rot, grau (übersprungen) oder bernsteinfarben (Zeitüberschreitung). Eine dünne Linie neben den noch möglichen Commits schrumpft von Runde zu Runde. Beim Darüberfahren über einen getesteten Commit siehst du das Ende seiner Ausgabe.

interaction-manual-feature-parallel-bisect-05 = Ein bereits getesteter Dateibaum wird nicht erneut getestet, wenn Befehl und Rechner gleich sind und der Tab noch offen ist.

interaction-manual-feature-parallel-bisect-06 = Am Ende wird der erste schlechte Commit ausgewählt. Läuft eine Bisect-Suche zwischen denselben Markierungen, markiert „In Bisect markieren“ ihn dort als schlecht und seine Eltern als gut und beendet damit jene Suche am selben Commit. „Abbrechen“ stoppt alle Tests und entfernt die Worktrees, wie jedes andere Ende auch. Nach einem Absturz zurückgebliebene Worktrees werden beim Öffnen des Tabs oder beim nächsten parallelen Bisect entfernt.

interaction-manual-feature-parallel-bisect-07 = Ein Branch, der von seinem Upstream abgewichen ist (rebased, noch nicht gepusht), hat „Commits mit origin/x vergleichen“ im Menü: die Commits beider Seiten, wie bei git range-diff paarweise zugeordnet.

# Code owners
interaction-manual-feature-code-owners-title = Code-Verantwortliche

interaction-manual-feature-code-owners-01 = Mit einer CODEOWNERS-Datei zeigt jede Dateizeile ihre Verantwortlichen. Unterstützt werden GitHubs Format, GitLabs Abschnitte sowie die regulären Ausdrücke von Gitea und Forgejo, jeweils am Ort und nach den Regeln der Forge gelesen. Über den Dateien eines Commits, Vergleichs, Pull Requests oder der Staging-Area steht, wer benötigt wird, etwa „Benötigt payments und infra“.

interaction-manual-feature-code-owners-02 = „Pull Request erstellen“ schlägt die Verantwortlichen für die Änderungen des Branches als Reviewer vor, anhand der CODEOWNERS der Basis; bei einem Entwurf niemanden. Entferne die Häkchen bei denen, die du nicht anfragen möchtest.

interaction-manual-feature-code-owners-03 = „CODEOWNERS prüfen“ (Ctrl+P) listet Zeilen auf, die die Forge anders auslegt, als sie aussehen, Muster ohne passende Datei und Dateien ohne Verantwortliche.

# Tests over history
interaction-manual-feature-history-tests-title = Tests im Verlauf

interaction-manual-feature-history-tests-01 = Rechtsklick auf einen Commit und „Commits bis hier testen…“ oder „Commits testen…“ in der Befehlspalette führt einen Befehl wie cargo test auf jedem Commit vom Basis-Branch bis zu diesem aus. Die Tests laufen auf dem Rechner des Repositorys (hier, in WSL oder über SSH) in temporären Worktrees in dessen temporärem Ordner. Mehrere Commits werden gleichzeitig im Hintergrund getestet, auf einem anderen Rechner höchstens 4; du kannst weiterarbeiten. Exitcode 0 besteht, jeder andere schlägt fehl.

interaction-manual-feature-history-tests-02 = Der Graph erhält eine Testspur: ein kleines Quadrat je Commit, grün mit Häkchen bei Erfolg, rot mit Kreuz bei Fehler, umrandet beim Warten oder Ausführen, bernsteinfarben mit Punkt, wenn kein Test möglich war. Beim Darüberfahren wird die Bedeutung erklärt. Ein Klick zeigt die letzten Ausgabezeilen mit „Kopieren“, sofern der Test seit dem Start von gitcraft lief; ältere Ergebnisse oder solche von git test haben keine verfügbare Ausgabe. Über das Kontextmenü der Spaltenüberschrift oder Einstellungen → Spalten blendest du die Spur ein oder aus; der Kolben darüber öffnet „Commits testen…“.

interaction-manual-feature-history-tests-03 = Ein Banner über dem Graphen zeigt den Fortschritt mit „Stoppen“ (fertige Tests behalten ihr Ergebnis), danach das Gesamtergebnis und den ältesten fehlgeschlagenen Commit, den ein Klick auswählt.

interaction-manual-feature-history-tests-04 = Ergebnisse werden je Dateibaum in git notes gespeichert, wie bei git-test: refs/notes/tests/default, der Befehl in test.default.command. So teilen git test auf der Befehlszeile und gitcraft die Ergebnisse. Eine Rebase, die die Dateien eines Commits beibehält, behält sein Quadrat. Ein einmal getesteter Dateibaum wird nur erneut getestet, wenn „Commits mit vorhandenem Ergebnis erneut testen“ aktiviert ist. Beim Ändern des Befehls können die Ergebnisse des alten verworfen werden.

# Rehearse
interaction-manual-feature-rehearse-title = Probelauf

interaction-manual-feature-rehearse-01 = Rechtsklick auf einen Branch (Plakette oder linker Bereich) und „Merge oder Rebase proben…“, oder die Befehlspalette für den Basis-Branch, probiert eine Rebase des ausgecheckten Branches darauf oder einen Merge dieses Branches vorab aus. Der Probelauf erfolgt in einem temporären Worktree auf dem Rechner des Repositorys. Deine Branches und Dateien bleiben unverändert.

interaction-manual-feature-rehearse-02 = Der Bericht nennt die Stelle des Stopps, bei einer Rebase etwa Commit 3 von 5, und die konfliktbehafteten Dateien oder meldet einen problemlosen Durchlauf. Mit Testbefehl zeigt er auch, ob das Ergebnis besteht; ein bereits für diese Dateien gespeichertes Ergebnis wird übernommen, statt erneut zu testen.

interaction-manual-feature-rehearse-03 = Aktiviere „Auch den Merge proben“ oder entsprechend die Rebase, um beide nebeneinander und die Gleichheit ihrer Dateien zu sehen. „Jetzt rebasen“ oder „Jetzt mergen“ führt die gewählte Aktion tatsächlich aus, nachdem geprüft wurde, dass sich keiner der Branches inzwischen bewegt hat. Die Aktion landet im Rückgängig-Verlauf (Ctrl+Z); Konflikte öffnen sich wie üblich im Konflikteditor.

# Find the interaction
interaction-manual-feature-interaction-title = Wechselwirkung finden

interaction-manual-feature-interaction-01 = Wenn zwei Branches ihre Tests jeweils bestehen, zusammen aber scheitern, öffne das Kontextmenü eines Branches (Plakette oder linker Bereich) und wähle „Wechselwirkung finden…“ oder tippe dies in die Befehlspalette. Gesucht werden die Commits dieses und des ausgecheckten Branches, die gemeinsam scheitern.

interaction-manual-feature-interaction-02 = Im Hintergrund werden Teilmengen ihrer Commits auf den gemeinsamen Ausgangs-Commit angewendet und in einem temporären Worktree auf dem Rechner des Repositorys getestet (Delta-Debugging). Das läuft, bis sich beispielsweise sagen lässt: Diese beiden Commits scheitern gemeinsam; einzeln bestehen sie. Ein Klick auf einen Commit wählt ihn im Graphen aus.

interaction-manual-feature-interaction-03 = Zuerst wird geprüft, ob beide zusammen scheitern und jeder allein besteht. Ist das nicht so oder lassen sich ihre Commits nicht gemeinsam anwenden, wird dies gemeldet. Es laufen höchstens so viele Tests, wie du festlegst. Danach wird die kleinste bisher gefundene fehlschlagende Menge gezeigt. Ein Fehlschlag wird vor Anerkennung erneut getestet, standardmäßig einmal; lässt er sich nicht wiederholen, wird er als instabil beiseitegelegt. „Stoppen“ beendet die Suche.

interaction-manual-feature-interaction-04 = Beide Branches dürfen seit ihrer Verzweigung zusammen höchstens 200 Commits und keine Merges enthalten. Die Checkouts der Suche führen keine Hooks aus. Commits eines Branches, die ohne den anderen scheitern, etwa weil der spätere reparierende Commit fehlt, zählen nicht als Wechselwirkung. Mit dem Testbefehl des Repositorys werden Ergebnisse je Dateibaum mit der Testspur geteilt.

# Changes and commits
interaction-manual-feature-changes-commits-title = Änderungen und Commits

interaction-manual-feature-changes-commits-01 = Im rechten Bereich kannst du ganze Dateien stagen, aus der Staging-Area nehmen oder verwerfen.

interaction-manual-feature-changes-commits-02 = „Pfad / Baum“ oben im rechten Bereich listet Dateien flach oder in ihren Ordnern auf; ein Klick auf einen Ordner klappt ihn ein.

interaction-manual-feature-changes-commits-03 = Im Datei-Diff kannst du einzelne Hunks stagen, aus der Staging-Area nehmen oder verwerfen oder gewünschte Zeilen anklicken und nur diese bearbeiten.

interaction-manual-feature-changes-commits-04 = Einen Teil eines früheren Commits rückgängig machen: Öffne im Kontextmenü einer Datei in der Liste „Änderungen an dieser Datei zurücknehmen“ oder nutze im Diff „Datei zurücknehmen“, „Hunk zurücknehmen“ oder nach Auswahl von Zeilen „N Zeilen zurücknehmen“. Die Änderung landet zur Prüfung vor dem Commit im Arbeitsbaum. Ctrl+Z macht sie rückgängig, wenn sich die Datei seitdem nicht geändert hatte. Andernfalls wird sie per Dreiwege-Merge gestagt eingearbeitet; Konflikte werden in der Datei markiert. Binärdateien lassen sich so nicht zurücknehmen.

interaction-manual-feature-changes-commits-05 = Eine in Git LFS gespeicherte Datei zeigt ihre LFS-Objekte samt Größen. Bei installiertem git-lfs bietet die Befehlspalette „LFS: Fetch“ und „LFS: Pull“.

interaction-manual-feature-changes-commits-06 = LFS-Sperren: Ein Schloss in der Dateizeile zeigt, wer sie hält; beim Darüberfahren siehst du, seit wann. „Sperren“ und „Entsperren“ stehen im Menü. „LFS: Sperren…“ (Ctrl+P) listet alle Sperren, entsperrt mehrere zugleich, hebt fremde zwangsweise auf (nur für Administratoren) und macht ein Muster sperrbar. Über den Änderungen werden geänderte Dateien mit fremder Sperre sowie geänderte sperrbare Dateien ohne Sperre markiert. Nach einem Push bietet gitcraft an, deine Sperren für die übertragenen Dateien freizugeben.

interaction-manual-feature-changes-commits-07 = „LFS: Speicher…“ (Ctrl+P in einem LFS-Repository) zeigt den Platzbedarf der LFS-Objekte, reine Zeigerdateien mit „Herunterladen“, die von git-lfs heruntergeladenen Ordner und „Bereinigen“. In „Alle Dateien des ausgecheckten Commits anzeigen“ markiert eine Wolke diese Dateien; ihr Menü bietet „LFS: Herunterladen“. Das Datei-Menü enthält außerdem „LFS: Nur diesen Ordner fetchen“ und für noch nicht in LFS gespeicherte Typen „*.ext in LFS verfolgen“.

interaction-manual-feature-changes-commits-08 = Unity-Projekte: „Unity: Einrichten…“ (Ctrl+P) prüft Force Text, sichtbare .meta-Dateien, .gitattributes und .gitignore, ergänzt Fehlendes und registriert UnityYAMLMerge. Ist es vorhanden, bietet eine konfliktbehaftete Szene oder ein Prefab „Mit UnityYAMLMerge lösen“. Ein ohne seine .meta-Datei gestagtes Asset oder der umgekehrte Fall wird über den Änderungen mit Korrekturmöglichkeit markiert.

interaction-manual-feature-changes-commits-09 = Andere Binärdateien zeigen Typ, Größen und Details vorher und nachher sowie beiderseits „Text“ (Word, PowerPoint-Folien in Präsentationsreihenfolge, OpenDocument, Excel-Zellen mit Formeln, PDFs mit lesbarem Text), „Einträge“ bei ZIP-basierten Dateien oder „Bytes“ als Hexdarstellung. Ein Hinweis meldet unvollständigen oder unlesbaren Text einer Seite oder nicht verglichene Einträge. Ein über textconv verglichener Datei-Diff ist gekennzeichnet; die Datei wird nur als Ganzes gestagt oder aus der Staging-Area genommen.

interaction-manual-feature-changes-commits-10 = Die Pfeile im Diff-Kopf springen von Änderung zu Änderung. Markierungen neben der Bildlaufleiste zeigen die Positionen der Änderungen; ein Klick springt dorthin.

interaction-manual-feature-changes-commits-11 = Der Hunk-Kopf nennt die Zahl der darüber eingeklappten unveränderten Zeilen, etwa „42 unveränderte Zeilen“. Seine Schaltflächen öffnen sie: ↑ direkt über diesem Hunk, ↓ direkt unter dem vorherigen, „Alle anzeigen“ oder „Funktion“ für die umgebende Funktion, soweit verfügbar. Neben einem Hunk geöffnete Zeilen bleiben beim erneuten Einlesen des Diffs offen; in einem Pull Request kommentierte Zeilen öffnen sich von selbst. Hunks und Zeilen lassen sich wie zuvor stagen.

interaction-manual-feature-changes-commits-12 = Innerhalb einer Datei verschobener Code erhält eine eigene gedämpfte Farbe und an beiden Stellen einen Balken. „Verschoben zu Zeile… / Verschoben von Zeile…“ an der ersten Blockzeile öffnet einen Bereich mit beiden Stellen, markierten zwischenzeitlichen Bearbeitungen und je einer Sprungschaltfläche. Esc schließt ihn.

interaction-manual-feature-changes-commits-13 = Schreibe Zusammenfassung und Beschreibung und committe; ändere den letzten Commit; wähle die Commit-Identität oder entferne mit „Autor des Profils verwenden“ die eigene Identität des Repositorys.

interaction-manual-feature-changes-commits-14 = Das Commit-Feld beginnt mit commit.template des Repositorys ohne Kommentarzeilen und den Ergänzungen seines prepare-commit-msg-Hooks, wie Gits Editor. Der Hook wird nach 10 Sekunden gestoppt; Fehler erscheinen unter der Nachricht und verhindern nie einen Commit. Eine unveränderte Vorlage wird nicht committet, da Git sie ablehnt. Beim Amend oder Merge tritt dessen Nachricht an ihre Stelle.

interaction-manual-feature-changes-commits-15 = Über der Nachricht stehen Vorlagen (commit.template, „prepare-commit-msg ausführen“, gespeicherte Vorlagen dieses oder aller Repositorys, „Diese Nachricht als Vorlage speichern“; „Rückgängig“ holt eine ersetzte Nachricht zurück), die Typ-/Scope-Auswahl (Conventional Commits oder Gitmoji), „Co-Autor hinzufügen“ (Repository-Autoren mit .mailmap-Namen als Co-authored-by:-Trailer) und Optionen: Conventional Commits, Gitmoji oder keine Auswahl, Vorausfüllen per Hook und Muster für Issue-Links.

interaction-manual-feature-changes-commits-16 = Beim Schreiben werden Namen geänderter Dateien und am Anfang der Zusammenfassung zuvor verwendete Typen und Scopes vervollständigt. Solange die Liste offen ist, übernimmt Tab den markierten Vorschlag, statt zum nächsten Feld zu wechseln; ↑ / ↓ bewegen die Auswahl, Esc schließt sie.

interaction-manual-feature-changes-commits-17 = Issue-Verweise in einer Commit-Nachricht verlinken darunter auf ihre Seite: #12 zum Issue der Forge sowie jedes in den Optionen des Commit-Felds ergänzte Muster (JIRA-\d+ → https://jira.example.com/browse/$0).

interaction-manual-feature-changes-commits-18 = „Hooks überspringen“ im Commit-Feld oder Rechtsklick auf „Push“ und „Pushen ohne Hooks“ führt nur diesen Commit oder Push mit --no-verify aus. Ein fehlgeschlagener Hook wird in der Fehlermeldung genannt.

interaction-manual-feature-changes-commits-19 = Profile (Titelleiste oder Einstellungen → Profil) haben jeweils eigene Tabs, zuletzt verwendete Repositorys, Theme, Git-Autor und Commit-Signatur. Die Git-Konfiguration des Repositorys (user.name, user.email, Signierung) hat weiterhin Vorrang vor der des Profils.

interaction-manual-feature-changes-commits-20 = Themes findest du über das Palettensymbol in der Titelleiste oder Einstellungen → Erscheinungsbild mit Vorschaubildern; fahre für eine Vorschau darüber. Importiere ein VS-Code-Theme oder Base16-Schema, folge dem Theme von VS Code oder speichere eine Kopie als bearbeitbare Datei. Akzent, Ecken und Linienfarben lassen sich für jedes Theme anpassen.

interaction-manual-feature-changes-commits-21 = Zeige auf eine Einstellung; der Bereich daneben zeigt ihre Wirkung mit den aktuell eingestellten Werten.

interaction-manual-feature-changes-commits-22 = Einstellungen → Commit-Signierung legt Format, Schlüssel und Signierprogramm sowie das standardmäßige Signieren von Commits und Tags in deiner globalen Git-Konfiguration fest. Unter Einstellungen → Profil kannst du stattdessen nur für das Profil signieren.

interaction-manual-feature-changes-commits-23 = Die Funkeln-Schaltfläche im Commit-Feld lässt ein vorhandenes KI-Werkzeug (claude -p, codex exec, gh copilot oder einen eigenen Befehl) die Nachricht aus den gestagten Änderungen schreiben, beim Amend aus dem letzten Commit samt diesen Änderungen. Es läuft auf dem Rechner des Repositorys, hier, in WSL oder über SSH; gitcraft speichert keinen API-Schlüssel. Die Funktion bleibt aus, bis du ein Werkzeug wählst: an der Schaltfläche oder unter Einstellungen → KI, wo auch Prompts und der Umfang des gesendeten Diffs stehen. Eine selbst geschriebene Nachricht wird nie ungefragt ersetzt. „Abbrechen“ stoppt das Werkzeug.

interaction-manual-feature-changes-commits-24 = Ein Doppelklick auf die Nachricht eines Commits im rechten Bereich bearbeitet sie: möglich für jeden Commit des ausgecheckten Branches, auf den kein Merge folgt.

interaction-manual-feature-changes-commits-25 = Stash (Werkzeugleiste oder Befehlspalette) speichert alles, ausgewählte Dateien, nur Gestagtes (git 2.35 oder neuer) oder einen Schnappschuss, der einen Stash anlegt und Dateien sowie Staging unverändert lässt. Du wählst Nachricht und Einbeziehen unversionierter Dateien; ausgewählte unversionierte Dateien werden immer einbezogen.

interaction-manual-feature-changes-commits-26 = Ctrl+click auf Dateien in den Änderungen wählt sie für einen Stash aus; alternativ Rechtsklick und „Diese Datei stashen…“. Die Leiste über der Liste stasht die Auswahl. Konfliktdateien sowie gestagte Umbenennungen oder Löschungen lassen sich nicht dateiweise stashen: Nimm sie zuerst aus der Staging-Area oder stashe das Gestagte.

interaction-manual-feature-changes-commits-27 = Unter dem Commit-Hash lassen sich Abschnitte aufklappen: „Signatur“ mit Gültigkeit, Fehlergrund, Unterzeichner und Git-Ausgabe sowie „Erneut prüfen“ nach Schlüsselimport; „Enthalten in“ mit Branches, Tags und erstem nachfolgendem Tag; „Notizen“ zum Hinzufügen, Ändern und Löschen im ausgewählten Namensraum, dessen „Notizen fetchen“ und „Notizen pushen“ ihn mit einem Remote teilen und bei beidseitig exklusiven Notizen stoppen, statt zu überschreiben; „Rückverweise“ mit späteren Commits, die ihn zurücknehmen, cherry-picken, in einem Trailer wie Fixes: nennen oder seinen Hash erwähnen. Ein Klick öffnet den jeweiligen Commit.

interaction-manual-feature-changes-commits-28 = Gestagte Änderungen in einen älteren Commit einarbeiten: Rechtsklick auf ihn im Graphen und „Gestagte Änderungen in diesen Commit einarbeiten“ oder „Einarbeiten…“ neben „Letzten Commit ändern“. Zuerst werden der Commit und die dadurch umgeschriebenen Folge-Commits gezeigt; bereits gepushte werden gewarnt. „Als fixup!-Commit für später behalten“ committet sie lediglich obenauf als „fixup! <summary>“. Konflikte löst du wie bei einer Rebase; „Abbrechen“ kehrt zum Stand vor dem Einarbeiten zurück. Ctrl+Z macht es rückgängig und lässt die Änderungen wieder gestagt.

interaction-manual-feature-changes-commits-29 = Commits, Merges, Rebases, Einarbeitungen, Resets, Checkouts, Branches, Tags und Stashes lassen sich rückgängig machen und wiederholen.

interaction-manual-feature-changes-commits-30 = Reflog (linker Bereich, Branch-Menü oder Ctrl+P) zeigt, wo HEAD und jeder Branch waren, was sie bewegt hat (Commit, Checkout, Reset, Rebase…) und von welchem zu welchem Commit, auch bei Aktionen im Terminal oder vor dem Öffnen von gitcraft. Wähle einen Eintrag für seinen Commit. „Hier Branch erstellen“ oder ein Doppelklick holt ihn zurück; du kannst ihn auch cherry-picken, auschecken oder seinen Hash kopieren.

interaction-manual-feature-changes-commits-31 = „Verlorene Commits finden“ sucht auch nach Ablauf ihres Reflogs nach Commits, die kein Branch, Tag oder Stash mehr hält, neueste zuerst, mit denselben Schaltflächen. Einen gelöschten Stash stellst du mit „Stash anwenden“ wieder her. Dafür wird jedes Objekt gelesen; große Repositorys brauchen Zeit. „Abbrechen“ stoppt die Suche.

interaction-manual-feature-changes-commits-32 = Vor Reset, Löschen eines lokalen oder entfernten Branches, Entfernen eines Commits oder Verwerfen einer Datei erklärt der Dialog die Wirkung: welche Commits den Branch verlassen (beim Löschen jene, die der ausgecheckte Branch nicht hat), jeweils als noch auf einem anderen Branch oder nur hier markiert; welche Dateien nicht committete Änderungen verlieren; ob umgeschriebene Commits schon gepusht wurden und deshalb Force Push nötig wird; die ausgeführten Git-Befehle in einem eingeklappten Abschnitt und ob Ctrl+Z die Aktion wiederherstellt.

# Coding agents
interaction-manual-feature-agents-mcp-title = Coding-Agenten

interaction-manual-feature-agents-mcp-01 = gitcraft als MCP-Server zu einem Agenten hinzufügen: claude mcp add --scope user gitcraft -- gitcraft mcp (Claude Code) oder codex mcp add gitcraft -- gitcraft mcp (Codex). Unter Einstellungen → Agenten stehen beide Befehle zum Kopieren bereit.

interaction-manual-feature-agents-mcp-02 = Agenten lesen Graph, Status, Diffs, Blame, Branches, Konflikte, CI-Builds und Pull Requests, mit oder ohne geöffnetes Fenster.

interaction-manual-feature-agents-mcp-03 = Änderungen eines Agenten (Stagen, Commit, Branches, Checkout, Stash, Cherry-pick, Revert, Reset, Commit verschieben) laufen im Repository-Tab als Schritt seiner Rückgängig-Liste. Der Roboter auf der Rückgängig-Schaltfläche zeigt an, dass das nächste Ctrl+Z den Agentenschritt zurücknimmt.

interaction-manual-feature-agents-mcp-04 = Push, Branch löschen, Hard Reset, Commit entfernen oder eine Konfliktdatei stagen wartet auf eine Karte mit „Erlauben / Ablehnen“, Agentenname und Git-Befehlen. Ohne Antwort innerhalb von zwei Minuten wird abgelehnt.

interaction-manual-feature-agents-mcp-05 = Einstellungen → Agenten schaltet Änderungen durch Agenten aus; Lesen funktioniert weiterhin.

# Timeline and undo tree
interaction-manual-feature-timeline-title = Zeitleiste und Rückgängig-Baum

interaction-manual-feature-timeline-01 = Ctrl+Shift+Z oder die Uhr in der Werkzeugleiste zeigt die Zeitleiste: jede Operation mit den bewegten Branches und geänderten Dateien, auch nach einem Neustart. Wiederholen ist Ctrl+Y.

interaction-manual-feature-timeline-02 = Auch anderswo bewegte Branches (Terminal, andere Anwendung, während gitcraft geschlossen war) erscheinen, als „anderswo“ markiert und nach dem Reflog benannt. Ctrl+Z macht auch diese Schritte rückgängig.

interaction-manual-feature-timeline-03 = Mache einige Schritte rückgängig und führe etwas Neues aus: Die zurückgenommenen Schritte bleiben als Seitenpfad des Baums erhalten. Wähle einen Eintrag und „Hierhin wechseln“ oder doppelklicke ihn, um die Branches auf seinen Endstand zu setzen. Dateiänderungen bleiben erhalten; gestagte Änderungen sind danach ungestagt.

interaction-manual-feature-timeline-04 = Die früheren Branch-Spitzen eines Eintrags kehren mit „… zurücksetzen“ oder als neuer Branch mit „Dort neuen Branch erstellen“ zurück, auch nach git gc: Sie werden unter refs/gitcraft/undo/ aufbewahrt.

interaction-manual-feature-timeline-05 = „Vergessen…“ entfernt nach Rückfrage ältere Einträge. Über die neuesten 300 hinaus verschwinden sie von selbst.

interaction-manual-feature-timeline-06 = Nach Rebase, Squash, Umordnen oder Amend zeigt ein Beleg, ob die endgültigen Dateien identisch und die Autoren erhalten sind, wie viele Nachrichten geändert wurden und was mit Signaturen geschah. In der Zeitleiste ordnet „Jeden Commit verfolgen“ jedem alten Commit seinen Ersatz zu, wie git range-diff.

interaction-manual-feature-timeline-07 = „Diesen Schritt entfernen, spätere Arbeit behalten“ bei einem Eintrag, der nur Commits auf einem Branch erzeugte (Commits, Merge, Cherry-picks), spielt die seither darauf entstandenen Commits ohne ihn erneut ab: auf einem neuen Branch in einem Worktree neben dem Repository. Der ursprüngliche Branch bleibt bestehen. Ein nicht mehr anwendbarer Commit stoppt den Vorgang und listet Konfliktdateien auf. Löse sie in diesem Worktree oder verwirf den Versuch.

# Repository doctor
interaction-manual-feature-doctor-title = Repository-Doktor

interaction-manual-feature-doctor-01 = Der Schraubenschlüssel in der Werkzeugleiste oder „Doktor: …“ in der Befehlspalette öffnet den Repository-Doktor im großen Bereich mit seinen Abschnitten links. Esc schließt ihn.

interaction-manual-feature-doctor-02 = Ignore-Regeln und Attribute: Gib einen Pfad ein oder wähle den gleichnamigen Eintrag im Menü einer Arbeitsdatei. Du siehst die Regel, die sie ignoriert oder mit ! wieder einschließt, ob sie versioniert ist, ihre Zeilenenden im Repository und Arbeitsbaum sowie ihre Attribute: Text oder binär, LFS, Merge- und Diff-Treiber und benannte, aber hier nicht eingerichtete Treiber. Ignorierte Dateien werden aufgelistet. Bearbeite die .gitignore-Dateien des Repositorys, .git/info/exclude oder deine eigene Ausschlussdatei, prüfe die Vorschau der dadurch ignorierten oder wieder eingeschlossenen Dateien allein nach den Regeln dieser Datei und speichere dann.

interaction-manual-feature-doctor-03 = Hooks: Angezeigt werden der Hook-Ordner von Git (core.hooksPath oder .git/hooks), jeder Hook mit den auslösenden Operationen und Hindernisse: nicht ausführbar, CRLF-Zeilenenden, fehlender Interpreter oder ein Ordner mit Hook-Namen. „Ausführbar machen“ und „In LF umwandeln“ beheben dies mit einem Klick. husky, lefthook und pre-commit werden erkannt, samt Hinweis, wo ihre Hooks geändert werden. Die Ausgabe von Git und Hooks der letzten Operationen bleibt bis zum Schließen von gitcraft zum Nachlesen im Speicher.

interaction-manual-feature-doctor-04 = Zustand und Wartung: Commits, gepackte und lose Objekte mit Größe, Laufzeit von git status und den letzten Tab-Neuladungen sowie mögliche Beschleunigungen: Commit-Graph, geplante Wartung, Dateisystemmonitor, soweit Git ihn unterstützt, Untracked-Cache und Packen loser Objekte. Jede lässt sich mit einem Klick aktivieren. Auch die Bedeutung der Graphgrenze von 100 000 Commits für dieses Repository wird erklärt.

interaction-manual-feature-doctor-05 = Welche Konfiguration gewinnt: Jede Einstellung (Pull, Signierung, Zeilenenden, …) mit wirksamem Wert, Geltungsbereich und Datei. Aufklappen zeigt alle von Git gelesenen Werte, den wirksamen oben. Entferne einen Wert aus seiner Datei, damit andere wieder gelten, setze ihn für Repository, Worktree (mit extensions.worktreeConfig), globale Konfiguration oder alle auf dem Rechner (bei den letzten beiden nach Rückfrage) und vergleiche ihn zwischen Worktrees. Anmeldedatenbezogene Einstellungen werden nicht angezeigt.

interaction-manual-feature-doctor-06 = Versteckte Änderungen: Versionierte Dateien mit assume-unchanged oder skip-worktree erscheinen nie unter „Änderungen“. Hier siehst du die Bedeutung beider Flags und kannst versehentlich gesetzte nach Rückfrage löschen, alle oder ausgewählte. Von sparse-checkout ausgelassene Dateien stehen separat: Das sind keine von jemandem gesetzten Flags.

# Branches, remotes and more
interaction-manual-feature-branches-remotes-title = Branches, Remotes und mehr

interaction-manual-feature-branches-remotes-01 = Linker Bereich: lokale Branches mit Voraus-/Zurück-Zahlen, Remotes, Worktrees, Submodule, Stashes, Pull Requests und Tags.

interaction-manual-feature-branches-remotes-02 = Submodule zeigen, ob sie geändert, nicht initialisiert oder veraltet sind. Ihr Kontextmenü aktualisiert sie oder öffnet sie in einem Tab. Einstellungen → Git kann sie nach jedem Checkout, Pull, Merge und Rebase aktuell halten.

interaction-manual-feature-branches-remotes-03 = Fahre über einen Branch und klicke auf sein Auge, um ihn im Graphen auszublenden, oder wähle „Solo“ im Menü, um nur solo geschaltete Branches zu sehen. „Alle anzeigen“ oben bringt alle zurück.

interaction-manual-feature-branches-remotes-04 = „Oben anheften“ im Branch-Menü hält ihn oben unter „Lokal“. „Name / Zuletzt“ im Branch-Kopf sortiert die übrigen nach Name oder letztem Commit. Beides wird je Repository gespeichert.

interaction-manual-feature-branches-remotes-05 = Ein als „gemergt“ markierter Branch hat seine Arbeit im Standard-Branch: gemergt, rebased, cherry-gepickt oder per Pull Request squash-gemergt. „Veraltet“ bedeutet seit 90 Tagen kein Commit (Einstellungen → Git); „entfernt“ bedeutet, sein Upstream wurde auf dem Remote gelöscht.

interaction-manual-feature-branches-remotes-06 = Der Besen im Branch-Kopf oder „Veraltete oder gemergte Branches auswählen…“ in der Befehlspalette listet solche Branches mit Begründung auf; nur gemergte sind vorab angehakt. Die Auswahl wird in einem Schritt hier und auf Wunsch auch unter gleichem Namen auf dem Remote gelöscht und lässt sich mit „Rückgängig“ wiederherstellen. Ausgecheckter Branch, Standard-Branch und in einem Worktree ausgecheckte Branches werden nie angeboten.

interaction-manual-feature-branches-remotes-07 = Konflikte öffnen sich in einem Editor mit drei Bereichen und „Unsere“, „Ihre“ und „Beide“ für jeden Konflikt.

interaction-manual-feature-branches-remotes-08 = Schon vor dem Klick zeigen „Merge“, „Rebase“ und „Cherry-pick“ im Menü „Konflikte in 3 Dateien“, „problemlos anwendbar“ oder „möglicherweise Konflikte“. Beim Darüberfahren siehst du die Dateien, bei einer Rebase den Stopp-Commit und ob Mergiraf jeden Konflikt voraussichtlich lösen könnte. Eine Kennzeichnung neben dem Branch in der Werkzeugleiste meldet Konflikte mit seinem Upstream oder dem Standard-Branch. Diese Prüfung ändert nichts. Nicht committete Änderungen werden nicht einbezogen; betreffen sie dieselben Dateien, kann Git den Start verweigern.

interaction-manual-feature-branches-remotes-09 = Öffne „Original“ eines Konflikts, um den gemeinsamen Ausgangstext und die Einfügungen und Löschungen beider Seiten relativ dazu zu sehen. Haben beide Seiten die Datei hinzugefügt, gibt es kein Original; die Karte weist darauf hin.

interaction-manual-feature-branches-remotes-10 = „Beide behalten…“ bei einer nicht zusammenführbaren Konfliktdatei wie Bild oder Archiv zeigt eine Vorschau und schreibt dann jede Version daneben, nach ihrem Branch benannt: logo.main.png, logo.feature-x.png; ohne Branch ours / theirs, bei belegtem Namen mit -2. Vorhandene Dateien werden nie überschrieben. Prüfe sie in der Änderungsliste und wähle „Beide stagen“. Dabei wird der alte Name entfernt; Verweise darauf müssen eventuell angepasst werden.

interaction-manual-feature-branches-remotes-11 = Rechtsklick auf eine Datei im Arbeitsbaum, Commit oder Vergleich und „In Diff-Werkzeug öffnen“ zeigt ihren Diff extern. Das Konfliktdatei-Menü und „In Merge-Werkzeug lösen“ im Konflikteditor lösen sie extern. Einstellungen → Git → Externe Werkzeuge wählt das durch diff.tool / merge.tool benannte Werkzeug oder Beyond Compare, Araxis, VS Code, Meld, KDiff3, P4Merge oder WinMerge. Beim Schließen wird die Ansicht aktualisiert. Gelöst gilt die Datei erst, wenn das Werkzeug Erfolg meldet (Gits mergetool.<tool>.trustExitCode; andernfalls, wenn es die Datei gespeichert hat). mergetool.keepBackup bestimmt, ob eine .orig-Kopie bleibt. Unter WSL verwendet eine Voreinstellung das Windows-Programm mit Dateien über \\wsl.localhost; über SSH läuft das Werkzeug auf jenem Rechner und muss dort installiert sein.

interaction-manual-feature-branches-remotes-12 = „Wie dieser Konflikt entstand“ über Konflikten bei Merge, Rebase, Cherry-pick oder Revert listet je Seite die Commits auf, die die Datei berührt haben. Wähle einen pro Seite, um seine Änderung zu sehen oder beide probeweise zu mergen. „Ursprung finden“ sucht das erste Paar, dessen Merge schon einen Konflikt in dieser Datei erzeugt: Test-Merges mit git merge-tree, ohne deine Dateien oder den Index zu ändern. „Abbrechen“ stoppt die Suche.

interaction-manual-feature-branches-remotes-13 = Mit „Konfliktlösungen merken“ über dem Ergebnis des Konflikteditors oder in der Befehlspalette wird ein bereits gelöster Konflikt bei erneutem Auftreten genauso gelöst (git rerere). Der Editor meldet dies und zeigt den wiederverwendeten Text unter jedem Konflikt. Erst beim Speichern wird gestagt. „Diese Lösung vergessen“ bringt den Konflikt zurück.

interaction-manual-feature-branches-remotes-14 = „Grund notieren…“ unter einem Konflikt speichert in Git eine Begründung und einen Testbefehl zusammen mit seiner Lösung. Derselbe Konflikt wird beim nächsten Auftreten genauso gelöst, mit Begründung und letztem Testergebnis. Hat sich der umgebende Code geändert, wird die Lösung nur als veralteter Vorschlag angeboten. „Nach dem Speichern testen“ führt deine Tests aus, sobald die Datei gespeichert ist. „Notiz und Test behalten“ übernimmt die einer Entscheidung als deine eigenen. Die Befehlspalette holt oder sendet Entscheidungen über den Remote; die neuere gewinnt.

interaction-manual-feature-branches-remotes-15 = Das Kontextmenü einer Datei öffnet ihren Verlauf, ihre Blame-Ansicht oder die ganze Datei im Stand des Commits.

interaction-manual-feature-branches-remotes-16 = Eine Datei oder ein Hunk mit abgeschwächten Prüfungen erhält die Kennzeichnung „schwächt Prüfungen“: entfernte Assertions oder Tests, übersprungene, deaktivierte oder fokussierte Tests, stets erfolgreiche Assertions, größere Toleranzen, vollständig umgeschriebene Snapshots, erlaubte CI-Fehler, entfernte CI-Jobs oder hinzugefügtes --no-verify. Beim Darüberfahren werden die auslösenden Zeilen gezeigt.

interaction-manual-feature-branches-remotes-17 = „Verlauf dieser Zeilen“: Wähle Zeilen im Diff durch Ziehen oder Klicken geänderter Zeilen, in einer Datei durch Klick, Shift+click oder Ziehen und öffne das Kontextmenü. Ctrl+C kopiert sie. Aufgelistet werden alle Commits bis zu dieser Version, die diese Zeilen berührt haben, durch frühere Bearbeitungen und Umbenennungen zurückverfolgt. Ein Klick zeigt nur die Änderungen daran.

interaction-manual-feature-branches-remotes-18 = Wechsle im Diff von „Diff“ zu „Datei“, um die ganze Datei zu lesen. „Alle Dateien anzeigen“ unter den Dateien eines Commits listet sämtliche enthaltenen Dateien.

interaction-manual-feature-branches-remotes-19 = Ctrl+F sucht im Diff, in einer Datei oder einer Blame-Ansicht innerhalb dieser Ansicht; im Graphen sucht es Commits. Jeder Treffer wird markiert. Enter und Shift+Enter im Suchfeld oder die konfigurierbaren Befehle „Nächster/vorheriger Treffer“ wechseln zwischen Treffern. Aa beachtet Groß-/Kleinschreibung, ab findet nur ganze Wörter, .* behandelt die Eingabe als regulären Ausdruck.

interaction-manual-feature-branches-remotes-20 = „Link zu … kopieren“ und „… auf GitHub öffnen“ (auch GitLab, Forgejo, Gitea) stehen im Kontextmenü von Commit, Branch oder Datei sowie von Commit-Diff-Zeilen (Rechtsklick auf eine Zeile oder Textauswahl für einen Bereich) und Dateizeilen eines Commits (Klick, Shift+click für einen Bereich, dann Rechtsklick). Links auf Commit, Datei oder Zeilen verwenden den Commit-Hash; Branch-Links folgen dem Branch. Ziel ist der Remote, den der ausgecheckte oder angeklickte Branch verfolgt, sonst origin, sonst deine Auswahl. Die Statuszeile warnt, wenn Commit oder Branch noch nicht auf diesem Remote liegt.

interaction-manual-feature-branches-remotes-21 = Manche Dateien haben eine eigene Leseansicht über eine Schaltfläche im Diff-Kopf. Erneutes Klicken zeigt wieder die Zeilen; Staging bleibt dort. Ein Klick auf einen Teil führt zurück zu dessen Zeile oder der nächsten angezeigten. Kann die Datei so nicht gelesen werden, erscheint der normale Diff mit Begründung.

interaction-manual-feature-branches-remotes-22 = Fließtext (Buch-Schaltfläche für .md, .adoc, .rst und .txt): Absätze werden auf Bereichsbreite umbrochen. In bearbeiteten Absätzen sind entfernte Wörter rot hinterlegt und durchgestrichen, hinzugefügte grün. Nur anders umbrochene Absätze heißen „neu umbrochen“. Lange Folgen unveränderter Absätze werden eingeklappt; ein Klick zeigt sie.

interaction-manual-feature-branches-remotes-23 = Nach Schlüssel (Listen-Schaltfläche für JSON, YAML und TOML): geänderte Schlüsselpfade wie server.timeout: 30 → 60, hinzugefügte und entfernte Schlüssel, Werte mit geändertem Typ und separat nur verschobene Schlüssel. Ein Klick springt zur Schlüsselzeile.

interaction-manual-feature-branches-remotes-24 = Als Tabelle (für CSV und TSV): hinzugefügte, entfernte und geänderte Zeilen im Raster, geänderte Zellen als alt → neu, erkannte Spaltenumbenennungen. .dvc-Dateien zeigen die verfolgten Daten mit Größe und Hash vorher und nachher.

interaction-manual-feature-branches-remotes-25 = Abhängigkeitsänderungen (Paket-Schaltfläche für Cargo.lock, package-lock.json, npm-shrinkwrap.json, pnpm-lock.yaml, poetry.lock, uv.lock, composer.lock und go.sum): Pakete mit neuerer oder älterer Version, hinzugefügte und entfernte Pakete mit Links zur Registry und den Änderungen sowie Lizenzänderungen, soweit die Lockdatei sie enthält.

interaction-manual-feature-branches-remotes-26 = API-Vertragsänderungen (Server-Schaltfläche für OpenAPI- und Swagger-Dateien oder aus der Schlüsselansicht): inkompatible Änderungen wie entfernte Endpunkte, neu erforderliche Parameter und eingeschränkte Antworten, jeweils mit Endpunkt. Ein Klick öffnet die Stelle in der Datei.

interaction-manual-feature-branches-remotes-27 = Im Diff einer Markdown-Datei rendert das Auge die Änderungen blockweise oder mit „Datei“ die ganze Datei.

interaction-manual-feature-branches-remotes-28 = „Synchronisieren“ in Werkzeugleiste oder Befehlspalette pullt den aktuellen Branch gemäß der Pull-Einstellung und pusht ihn danach. Scheitert der Pull an Konflikten oder fehlender Fast-forward-Möglichkeit, wird nichts gepusht.

interaction-manual-feature-branches-remotes-29 = Lehnt der Remote einen Push wegen dortiger zusätzlicher Commits ab, erscheint ein Banner mit ihrer Anzahl und der Einordnung: hier umgeschriebener Branch (Amend, Rebase, Rückgängig), beidseitige Weiterentwicklung oder nur Rückstand. „Pullen, dann pushen“ führt sie zusammen. „Force Push (mit Lease)…“ nennt zuerst die vom Remote entfernten Commits und pusht nichts, wenn sich Remote oder eigener Branch seit der Prüfung bewegt haben.

interaction-manual-feature-branches-remotes-30 = Lehnt ein Hook oder Branch-Schutz den Push ab, wird Gits vollständige Meldung ohne Force-Push-Angebot gezeigt. Für einen noch auf keinem Remote liegenden Branch mit mehreren Remotes wird das Veröffentlichungsziel erfragt.

interaction-manual-feature-branches-remotes-31 = „Diesen Branch reviewen“ im Branch-Menü lässt das KI-Werkzeug prüfen, was der Branch dem Ziel-Branch für Pull Requests hinzufügt: main, master oder develop; bei diesem selbst dem ausgecheckten Branch. Die Antwort erscheint in einem Fenster mit „Kopieren“ und „Erneut reviewen“.

interaction-manual-feature-branches-remotes-32 = Rechtsklick auf einen Worktree und „Kollisionsmatrix…“ zeigt, welche Worktree-Branches beim Merge kollidieren würden, im Speicher und ohne Änderungen, und in welcher Integrationsreihenfolge möglichst wenige Branches überarbeitet werden müssen.

interaction-manual-feature-branches-remotes-33 = Rechtsklick auf einen Worktree und „Agentenbegrenzung…“ listet für Agenten gesperrte Pfade. Sie werden in die Deny-Regeln von Claude Code in jedem Worktree geschrieben und auf Wunsch auch in .cursorignore und AGENTS.md. Ein verknüpfter Worktree, der solche Pfade dennoch berührt hat (committete, geänderte, unversionierte oder ignorierte Dateien) oder nicht geprüft werden kann, wird rot. Merge oder Rebase auf einen Branch mit solchen Änderungen fragt zuerst nach.

interaction-manual-feature-branches-remotes-34 = „Worktree hinzufügen“ speichert ein gemeinsames Standard-Stammverzeichnis für die Worktrees dieses Repositorys, auch für Pull Requests, Issues und Agentenaufgaben. Anfangs gilt ../<repo>.wt/: feat/awesome-stuff landet unter ../<repo>.wt/feat/awesome-stuff. Gib einen anderen Stammordner ein oder wähle ihn aus; „Im Repository“ verwendet .worktrees/. Relative Stammordner beginnen beim Haupt-Checkout. Ordner innerhalb eines Checkouts werden lokal über info/exclude ignoriert. „Worktree-Ordner“ lässt sich weiterhin für einen einzelnen Worktree überschreiben.

interaction-manual-feature-branches-remotes-35 = Rechtsklick auf einen verknüpften Worktree und „Abweichung vom Aufgabenumfang…“ stellt seine Änderungen der Aufgabe oder dem verknüpften Issue gegenüber: nicht erwähnte Ordner, neue Abhängigkeiten, geänderte öffentliche Signaturen und der reine Formatierungsanteil. Ein Klick öffnet die zugehörigen Dateien.

interaction-manual-feature-branches-remotes-36 = Ordnet ein Diff unzusammenhängende Zeilen einander zu, wähle im Kopf einen anderen Algorithmus: Myers, Minimal, Patience, Histogram. Die oberste Zeile bleibt sichtbar, die Auswahl wird für diesen Dateityp gespeichert und die Repository-Konfiguration bleibt unverändert.

interaction-manual-feature-branches-remotes-37 = Generierte Dateien (linguist-generated oder -diff in .gitattributes), Lockdateien und minifizierte Dateien sind anfangs eingeklappt: am Ende von Dateilisten und im Diff, etwa aus dem Verlauf geöffnet, hinter einer Zeile wie „+1,204 −880, lockfile“. Ein Klick öffnet sie. Einstellungen → Git → Diffs ergänzt Muster oder schaltet das Einklappen aus.

interaction-manual-feature-branches-remotes-38 = Stacks: „Dieser Branch basiert auf…“ im Branch-Menü merkt den darunterliegenden Branch, zunächst mit dem aus dem Verlauf vermuteten Vorschlag. STACKS links zeigt jeden Stack, etwa main → schema → api → ui, mit den Commits jeder Schicht, ihrem Pull Request und einer Warnung, wenn sich ihr Eltern-Branch weiterbewegt hat und Restack nötig ist. „Änderungen, die x seinem Eltern-Branch hinzufügt“ zeigt nur die eigenen Änderungen; der Tooltip einer Graphplakette nennt ihren Stack. Beim Umbenennen eines Branches ziehen die darauf basierenden Schichten mit.

interaction-manual-feature-branches-remotes-39 = Restack: Ändert sich ein unterer Branch, zeigt STACKS die Anzahl zu aktualisierender abhängiger Branches. „Restack…“ zeigt zuerst deren neue Basen und spielt dann jeweils nur ihre eigenen Commits auf die Spitzen der Eltern, Eltern zuerst. Ein in einem anderen Worktree ausgecheckter Branch wird dort rebased. Ein Konflikt stoppt: lösen und „Restack fortsetzen“ oder „Restack abbrechen“, um alle Branches zurückzusetzen. Auch einmal Rückgängig setzt alle zurück. Das Schichtmenü bietet „Darauf basierende Branches restacken…“; die interaktive Rebase bietet „Abhängige Branches aktualisieren“ (--update-refs), um einen Stack von oben zu rebasen.

interaction-manual-feature-branches-remotes-40 = Gestapelte Pull Requests: Rechtsklick auf den Stack-Titel in STACKS oder „Pull Requests seines Stacks veröffentlichen…“ im Schichtmenü zeigt Basis und Patch jeder Schicht, pusht die Branches und öffnet die Pull Requests von unten nach oben, jeweils gegen den Branch darunter. Erneutes Ausführen aktualisiert alle durch Push und Korrektur ihrer Basen. Wird ein unterer gemergt, zielen die darüber auf dessen Ziel; ein ohne Merge geschlossener wird neu geöffnet. Die Forge wird je Schicht nach ihrem Branch gefragt, sodass ein unterbrochenes Veröffentlichen keinen Pull Request doppelt öffnet. Über unbekannte Commits eines Teammitglieds wird nie gepusht. Unterstützt: GitHub, GitLab, Forgejo und Gitea.

interaction-manual-feature-branches-remotes-41 = Ein Bild-Diff vergleicht in 2-up, Wischen, Zwiebelschalenansicht oder Differenz. Mausrad und Ziehen zoomen und verschieben alle Ansichten gleich; Doppelklick passt sie ein. Der Streifen darunter ist der Bildverlauf; ein Klick öffnet die Änderung eines Commits.

interaction-manual-feature-branches-remotes-42 = Ein Notebook-Diff zeigt Zellen einzeln mit Ausgaben, auch Bildern, vorher und nachher. „JSON“ zeigt die Zeilen. „Ausgaben beim Committen entfernen“ setzt nur in diesem Klon einen Clean-Filter: nbstripout, sonst ein Python-3-Skript. Gestagte Notebooks verlieren Ausgaben und Zähler; die Datei auf der Platte behält sie. Zellen werden niemals ausgeführt.

interaction-manual-feature-branches-remotes-43 = „Syntax“ im lokalen Diff vergleicht mit Difftastic (difft). Die Git-Vorbereitung erfordert --no-lazy-fetch und hat 20 Sekunden Zeit; das Werkzeug bekommt weitere 20 Sekunden. SSH und WSL verwenden den Patch, bis entferntes Aufräumen unabhängig überwacht werden kann. Geänderte Tokens bleiben trotz Einrückungs- und Umbruchänderungen sichtbar. Schalte „Syntax“ aus, um zu stagen oder Formatierungsänderungen zu sehen. Bei nicht unterstützten Sprachen, Parser- oder Werkzeugfehlern erscheint der Patch mit Hinweis.

interaction-manual-feature-branches-remotes-44 = Ein Klick auf einen Pull oder Merge Request links fokussiert seine Branch-Plakette und zeigt alle vorgeschlagenen Änderungen seit der Merge-Basis. Doppelklick öffnet Details, bei GitLab im Browser. Rechtsklick und „In Worktree öffnen“ öffnet seinen Branch in einem eigenen Worktree, der bei Bedarf angelegt wird.

interaction-manual-feature-branches-remotes-45 = „Arbeit aus Issue starten“ (Ctrl+P oder die Schaltfläche im Pull-Request-Kopf) listet offene, dir zugewiesene Issues auf GitHub, GitLab, Forgejo oder Gitea. Die Auswahl erstellt vom Standard-Branch des Remotes einen nach dem Issue benannten Branch, etwa 42-fix-login-crash, hier oder in einem neuen Worktree. Solange er ausgecheckt ist, hält eine Karte über dem Graphen die Akzeptanzkriterien, ersatzweise Aufgabenliste oder Beschreibung, im Stand des Abrufdatums sichtbar. Sein Pull Request, bei GitLab Merge Request, beginnt mit Issue-Titel und Closes #42.

interaction-manual-feature-branches-remotes-46 = gitcraft merkt sich, welche Version eines Pull Requests du angesehen hast. Nach erneutem Push des Autors, auch Force Push, meldet die Seite Änderungen, etwa „Seit deinem letzten Besuch umgeschrieben: 2 Commits geändert, 1 hinzugefügt“. Ein Klick vergleicht beide Versionen. In der Liste steht „seit deinem letzten Besuch geändert“; das Menü bietet „Änderungen seit deinem letzten Besuch“ und „Als gesehen markieren“. Das erste Öffnen der Pull-Request-Seite oder das Anzeigen ihres Vergleichs zählt als gesehen.

interaction-manual-feature-branches-remotes-47 = Pull Requests aus Forks zeigen „wartet auf Freigabe“ („Erlauben und ausführen“), „kann bearbeiten“ und „erstmals“. „Zum Autor pushen: auschecken“ fügt dessen Fork als Remote hinzu und checkt den Branch aus, sodass „Push“ dorthin zurückgeht. „Auf main rebasen und zum Autor pushen“ rebased ihn und führt Force Push mit Lease aus. „Unbenutzte Fork-Remotes entfernen“ (Ctrl+P) räumt danach auf.

interaction-manual-feature-branches-remotes-48 = Ein Terminalbereich mit Tabs im Ordner des Repositorys.

interaction-manual-feature-branches-remotes-49 = Die Auskoppeln-Schaltfläche im Kopf von Diff, Terminal, Pull Request oder CI-Ansicht oder „Aktuelle Ansicht auskoppeln / Terminal auskoppeln“ in der Befehlspalette verschiebt sie in ein eigenes Fenster, etwa auf einem anderen Monitor. Das Hauptfenster zeigt die darunterliegende Ansicht mit einer Erinnerung unten, die das Fenster hervorholt oder die Ansicht zurückbringt. Dort funktioniert alles wie zuvor; Tabwechsel lassen es offen. Die Auswahl eines Commits im Hauptfenster lässt es ebenfalls offen; der nächste Diff, Pull Request oder die nächste CI-Ansicht öffnet sich dort. Schließen dieses Fensters oder die Zurückholen-Schaltfläche bringt die Ansicht ins Hauptfenster. Das Schließen der Ansicht selbst über × oder Esc außerhalb des Terminals schließt das Fenster.

# Keyboard and screen readers
interaction-manual-feature-accessibility-title = Tastatur und Screenreader

interaction-manual-feature-accessibility-01 = Tab und Shift+Tab bewegen den Fokus durch Schaltflächen, Zeilen und Felder. Eine gezeichnete Schaltfläche oder Zeile mit Tastaturfokus erhält einen Ring in der Akzentfarbe; Enter oder Space betätigt sie.

interaction-manual-feature-accessibility-02 = Der Graph ist ein einzelner Tab-Stopp beim ausgewählten Commit. ↑ und ↓ verschieben Auswahl und Tastaturfokus gemeinsam; der nächste Tab verlässt den Graphen.

interaction-manual-feature-accessibility-03 = Screenreader lesen jede Zeile mit ihrem Namen, einen Commit etwa als „Commit a301da9, seine Zusammenfassung, sein Autor, wie lange her, seine Branches und Tags“. Entsprechend werden Dateien, Branches, Tabs und Schaltflächen vorgelesen.

interaction-manual-feature-accessibility-04 = Kontextmenüs per Rechtsklick und Doppelklicks zum Auschecken einer Plakette benötigen weiterhin die Maus.

interaction-manual-feature-accessibility-05 = Sind beim Start von gitcraft Systemanimationen ausgeschaltet (GNOME oder KDE, „Bewegung reduzieren“ auf macOS, „Animation beim Minimieren und Maximieren von Fenstern“ auf Windows), erscheinen Tabs und Bereiche ohne Gleiten an ihrem Platz. GITCRAFT_REDUCE_MOTION=1 oder 0 überschreibt diese Entscheidung.

# Custom commands
interaction-manual-feature-custom-commands-title = Eigene Befehle

interaction-manual-feature-custom-commands-01 = Einstellungen → Eigene Befehle fügt eigene Befehle zum Menü eines Commits, Branches oder einer Datei oder zur Befehlspalette (Ctrl+P) hinzu.

interaction-manual-feature-custom-commands-02 = Eine Befehlszeile läuft mit sh auf dem Rechner des Repositorys (hier, in WSL oder über SSH) in dessen Ordner. $SHA, $SHORT_SHA, $BRANCH, $REMOTE, $FILE und $REPO werden als Shellvariablen übergeben, nie in den Befehlstext eingesetzt. Schreibe "$FILE", damit ein Name mit Leerzeichen, Anführungszeichen oder $(…) ein einziges Wort bleibt.

interaction-manual-feature-custom-commands-03 = Eingabeaufforderungen fragen zuerst nach Text oder einem Branch. Jede Eingabe wird dem Befehl als eigene Variable übergeben ($MESSAGE…). „Vor dem Ausführen fragen“ zeigt die Befehlszeile vor dem Start.

interaction-manual-feature-custom-commands-04 = Die Ausgabe erscheint in einer Kurzmeldung (nur die letzten 8 Zeilen, bei Fehler rot) oder in einem eigenen Terminal-Tab, wo sie verfolgt und beantwortet werden kann. Enter schließt ihn.

interaction-manual-feature-custom-commands-05 = Ein Repository kann eigene Befehle in .gitcraft/commands.json mitbringen. Sie werden erst angeboten, nachdem du die Datei über „Repository-Befehle prüfen…“ im Menü oder der Befehlspalette gelesen und ihr vertraut hast. Bei jeder Änderung wird erneut gefragt. Einstellungen → Eigene Befehle listet vertrauenswürdige Repositorys auf, um das Vertrauen zurückzunehmen.

# When git fails
interaction-manual-feature-git-failures-title = Wenn Git scheitert

interaction-manual-feature-git-failures-01 = Ein von gitcraft erkannter Fehler zeigt unter der Werkzeugleiste oder auf der Startseite eine Karte mit Ursache und Handlungsvorschlag. „Was Git gesagt hat“ enthält Gits unveränderte Worte.

interaction-manual-feature-git-failures-02 = Die Schaltfläche behebt, was behebbar ist: „Diesem Ordner vertrauen“ bei einem Repository eines anderen Benutzers (safe.directory, nur dieser Ordner), „Veraltete Sperre entfernen“ nach Anzeige von Alter und laufendem Git, „Lange Pfade aktivieren“ und „Windows-Zertifikate verwenden“ für Git for Windows.

interaction-manual-feature-git-failures-03 = Eine Korrektur erklärt zuerst genau die Änderung und die betroffene Konfigurationsdatei und läuft erst nach Bestätigung. Eine Konfigurationsänderung hat „Rückgängig“ auf der Karte. „Erneut versuchen“ wiederholt die fehlgeschlagene Aktion, soweit möglich.

interaction-manual-feature-git-failures-04 = Erklärt werden Zeilenenden (core.autocrlf gegenüber .gitattributes), unterbrochene Übertragungen oder zu große Anfragen (HTTP 413), unabhängige Verläufe, Push aus einem flachen Klon und abgelehnte SSH-Schlüssel.

# Maintainers
interaction-manual-feature-maintainers-title = Maintainer

interaction-manual-feature-maintainers-01 = „Sign-off“ im Commit-Feld ergänzt Signed-off-by mit deinem Namen und deiner E-Mail-Adresse (DCO). In einem Repository mit DCO-Datei oder .github/dco.yml sowie bei einem geöffneten Pull Request mit fehlgeschlagener DCO-Prüfung ist es automatisch an. Eine abweichende Auswahl wird für das Repository gespeichert.

interaction-manual-feature-maintainers-02 = Wo Sign-offs erforderlich sind, steht ein fehlendes Autoren-Sign-off unter dem Autor des Commits. Die Pull-Request-Seite listet unter „Sign-off (DCO)“ die von der DCO-Prüfung abgelehnten Commits. Bei ausgechecktem Branch fügt „Commits des Branches mit Sign-off versehen…“ jedem dein Sign-off hinzu (rebase --signoff) und führt nach Vorschau Force Push mit Lease aus. Ctrl+Z stellt die alten Commits hier und auf dem Remote wieder her.

interaction-manual-feature-maintainers-03 = Die Taskleistenschaltfläche zeigt den Fortschritt von Clone, Fetch, Pull oder Push und wird bei einem Fehler im Hintergrund rot; unter Linux fordert sie Aufmerksamkeit an. Ihr Kontextmenü listet zuletzt verwendete Repositorys, „Klonen…“ und „Öffnen…“ auf.

interaction-manual-feature-maintainers-04 = Fetch, Pull, Push und Clone zeigen Gits Fortschritt (etwa „Receiving objects: 45%“) sowie einen dünnen Balken unter der Werkzeugleiste. „Abbrechen“ stoppt Git und seine gestarteten Prozesse und lässt den Rückgängig-Verlauf unverändert. Ein abgebrochener Clone entfernt die bereits geklonten Daten.

interaction-manual-feature-maintainers-05 = „Aktivität“, die Listenschaltfläche in der Werkzeugleiste, zeigt jede Operation mit Git-Befehlen, jeweiliger Dauer, Ergebnis und vollständiger Ausgabe; alles lässt sich kopieren. Ein Klick auf eine Meldung in der Werkzeugleiste öffnet dort die zugehörige Operation.

interaction-manual-feature-maintainers-06 = Fetch, Pull und Push lassen sich anklicken, während eine andere Operation läuft. Sie starten danach, höchstens drei warten in Reihenfolge. Abbrechen oder ein Fehler verwirft die Warteschlange. Die anderen Schaltflächen erklären, worauf sie warten.

interaction-manual-feature-maintainers-07 = Der Offline-Modus (Einstellungen → Git oder „Offline gehen“ in der Befehlspalette) verhindert selbstständige Netzwerkzugriffe von gitcraft: kein Hintergrund-Fetch, keine Avatare, Update-Prüfungen oder Build- und Pull-Request-Abfragen. Fetch, Pull, Push und andere Remote-Befehle erklären, warum sie nicht laufen. Bei getakteter oder fehlender Verbindung pausiert nur die Hintergrundarbeit. Die Werkzeugleiste zeigt dies und den letzten Fetch des Repositorys an.

# Coding agents
interaction-manual-feature-agent-tasks-title = Coding-Agenten

interaction-manual-feature-agent-tasks-01 = „Agentenaufgabe starten…“ über + bei AGENTEN oder die Befehlspalette erstellt Branch und Worktree, öffnet ihn und startet dort ein Terminal mit dem gewählten Agenten: Claude Code, Codex, Gemini CLI, Cursor Agent, Aider oder eigener Befehl. Sofern der Agent eine erste Nachricht annimmt, wird die Aufgabe dafür verwendet.

interaction-manual-feature-agent-tasks-02 = AGENTEN links listet Aufgaben mit den nicht committeten Änderungen ihrer Worktrees. Ein Klick öffnet eine Aufgabe zur Prüfung im normalen Diff. Ihr Menü startet den Agenten erneut oder öffnet dort ein Terminal.

interaction-manual-feature-agent-tasks-03 = Agentenrunden als Review-Schritte: „Agentenrunden: Jede Runde in diesem Worktree speichern“ in der Befehlspalette oder „Jede Runde einzeln zum Review speichern“ beim Aufgabenstart richtet die Hooks von Claude Code und Codex ein, die ab dem nächsten Agentenstart nach jeder Runde gitcraft checkpoint ausführen. Unter nicht committeten Änderungen erscheinen dann AGENTENRUNDEN, nach ihren Prompts benannt. Ein Klick zeigt die Änderung, das Menü vergleicht sie mit den aktuellen Dateien. Frühere Sitzungen werden eingeklappt; das Sitzungsmenü löscht ihre Runden.

interaction-manual-feature-agent-tasks-04 = An den Agenten übergeben: Klicke im Diff auf eine Zeilennummer, um eine Notiz zu schreiben; bei nicht committeten Änderungen Alt+click, da ein einfacher Klick dort die Zeile auswählt. Notizen bleiben bis zum Senden in diesem Worktree und sammeln sich unter dem großen Bereich. „An Agenten senden“ tippt sie als eine Nachricht in das nach dem Agenten benannte oder ausgewählte Terminal; du sendest sie mit Enter. Dies gibt es auch bei Konflikten mit allen drei Seiten, im Log eines fehlgeschlagenen CI-Laufs und im Thread eines Pull Requests.

interaction-manual-feature-agent-tasks-05 = „Versuche vergleichen…“ in der Befehlspalette oder „Vergleichen“ unter AGENTEN stellt 2–4 Worktrees oder Branches derselben Aufgabe nebeneinander: betroffene Dateien, Änderungsumfang, neue Abhängigkeiten und ob der Build funktioniert. Dafür läuft dein für das Repository gespeicherter Befehl im Terminal jedes Worktrees; nicht committete Arbeit zählt mit. Ein Klick auf eine Datei zeigt sie über die Versuche hinweg. Übernimm sie ganz, mit erneutem Klick rückgängig, oder wähle Hunks aus beliebigen Versuchen. „Zusammenstellen“ erstellt damit einen neuen Branch von der Basis, ohne etwas auszuchecken. Hunks zweier Versuche auf denselben Zeilen werden markiert und abgelehnt.

interaction-manual-feature-agent-tasks-06 = „Worktree-Einrichtung…“ in der Befehlspalette ist ein Rezept für jeden von gitcraft angelegten Worktree dieses Repositorys, mit oder ohne Agenten: Dateien vom Haupt-Worktree kopieren oder verknüpfen (.env.local, node_modules), Variablen setzen (PORT=$((3000 + GITCRAFT_WORKTREE_INDEX)): Der Haupt-Worktree ist 0, jeder andere behält seine zugeteilte Nummer) und einen Einrichtungsbefehl im Terminal mit sichtbarer Ausgabe ausführen. Es läuft mit sh auf dem Rechner des Repositorys, einschließlich WSL und SSH. Ein fehlgeschlagener Schritt wird gemeldet; anschließend starten weder Einrichtungsbefehl noch Agent.

# Packages
interaction-manual-feature-packages-title = Pakete

interaction-manual-feature-packages-01 = „Was ein Paket veröffentlicht“ aus der Befehlspalette (Ctrl+P) listet die Dateien auf, die cargo package --list oder npm pack --dry-run --ignore-scripts für jedes Crate des Workspace und die oberste package.json ausliefern würden: vermutlich versehentlich enthaltene Dateien (.env, Schlüssel, Logs, Build-Ausgabe), üblicherweise benötigte, aber fehlende Dateien (README, LICENSE, Hauptdatei) sowie neue und entfernte Dateien gegenüber der Version auf crates.io oder npm.
