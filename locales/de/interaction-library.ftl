interaction-scan-entry-limit = Angehalten nach { $entries } gelesenen Ordnereinträgen: Wähle einen tiefer liegenden Ordner, um den Rest zu sehen.

interaction-scan-time-limit = Angehalten: Es hat zu lange gedauert. Bisher { $found } gefunden.

interaction-library-save-root = Als Stammordner speichern

interaction-library-save-root-tip = Diesen Ordner als Repository-Stammordner behalten: Seine Repositorys bleiben auf der Startseite unter Alle Repositorys aufgeführt und werden bei späteren Scans erneut gefunden.

interaction-settings-label-repository-roots = Repository-Stammordner

interaction-library-settings-hint = Ordner, deren Repositorys auf der Startseite unter Alle Repositorys aufgeführt werden, für dieses Profil.

interaction-library-title = ALLE REPOSITORYS

interaction-library-manage = Repository-Stammordner…

interaction-library-empty = Füge einen Ordner hinzu, in dem du Repositorys aufbewahrst, und seine Repositorys werden hier aufgeführt. Repositorys neben denen, die du öffnest, werden ebenfalls aufgeführt.

interaction-library-add-root = Stammordner hinzufügen…

interaction-library-none-here = Hier sind noch keine Repositorys.

interaction-library-all = Alle

interaction-library-recent = Zuletzt

interaction-library-nearby = In der Nähe

interaction-library-stale = Veraltet

interaction-library-scanning = { $count ->
        [one] 1 Ordner wird durchsucht
       *[other] { $count } Ordner werden durchsucht
    }…

interaction-library-troubled = { $count ->
        [one] 1 Ordner konnte nicht vollständig durchsucht werden
       *[other] { $count } Ordner konnten nicht vollständig durchsucht werden
    }: siehe Repository-Stammordner…

interaction-library-hide = Aus der Bibliothek ausblenden

interaction-library-hide-tip = Aus der Bibliothek ausblenden: Es wird hier nicht mehr aufgeführt, egal was es findet. Es bleibt unter Zuletzt und kann weiterhin geöffnet werden.

interaction-library-pick = Wähle einen Ordner, in dem du Repositorys aufbewahrst

interaction-library-roots-title = Repository-Stammordner

interaction-library-roots-about = gitcraft führt die Repositorys in diesen Ordnern auf der Startseite unter Alle Repositorys auf und durchsucht sie ab und zu im Hintergrund. Es sieht sich nur Ordner an: Es führt für sie nie git aus, holt nichts ab und verbindet sich nirgendwohin.

interaction-library-no-roots = Noch keine Stammordner.

interaction-library-enabled = Diesen Ordner durchsuchen

interaction-library-listed = { $count } aufgeführt

interaction-library-depth-0 = Nur der Ordner

interaction-library-depth = { $depth ->
        [one] 1 Ebene tief
       *[other] { $depth } Ebenen tief
    }

interaction-library-move = Einen anderen Ordner für diesen Stammordner wählen

interaction-library-remove = Diesen Stammordner entfernen (seine Repositorys bleiben unter Zuletzt und in offenen Tabs)

interaction-library-rescan = Erneut durchsuchen

interaction-library-rescan-all = Alle erneut durchsuchen

interaction-library-nearby-switch = Repositorys neben denen aufführen, die ich öffne

interaction-library-nearby-tip = Wenn du ein Repository öffnest, werden die anderen Repositorys in seinem Ordner als In der Nähe aufgeführt (nur dieser Ordner, eine Ebene tief; nie dein Benutzerordner, ein Laufwerk oder ein anderer Rechner). Die letzten fünf Ordner werden behalten.

interaction-library-forget-nearby = Die Repositorys dieses Ordners nicht mehr aufführen

interaction-library-hidden = { $count ->
        [one] 1 Repository aus der Bibliothek ausgeblendet
       *[other] { $count } Repositorys aus der Bibliothek ausgeblendet
    }

interaction-library-unhide = Wieder anzeigen

interaction-library-slow = Warte auf einen Ordner, der langsam antwortet: Andere Suchen beginnen, sobald er antwortet.

interaction-library-off = Aus

interaction-library-waiting = Wartet auf die Suche

interaction-library-never = Noch nicht durchsucht

interaction-library-unavailable = Nicht verfügbar: { $reason } · { $when }

interaction-library-partial = Unvollständig: { $why } · { $found } gefunden · { $when }

interaction-library-done = { $found ->
        [one] 1 Repository
       *[other] { $found } Repositorys
    } · { $when }

interaction-library-limit-repos = Repository-Grenze erreicht

interaction-library-limit-folders = Ordner-Grenze erreicht

interaction-library-limit-time = hat zu lange gedauert

interaction-library-limit-asked = angehalten

interaction-library-limit-unreadable = manche Ordner waren nicht lesbar

interaction-library-limit-full = die Bibliothek ist voll
