pub(super) const ROWS: &[(usize, (&str, &str, &str))] = &[
    (
        104,
        (
            "Source export cancelled",
            "Source export cancelled",
            "Quellexport abgebrochen",
        ),
    ),
    (
        105,
        (
            "Raw source export exceeded its 60s preparation budget",
            "Raw source export exceeded its 60s preparation budget",
            "Der direkte Quellexport hat seine Vorbereitungszeit von 60 Sekunden überschritten",
        ),
    ),
    (
        106,
        (
            "Sandbox source exceeds 20,000 files or 128MiB",
            "Sandbox source exceeds 20,000 files or 128MiB",
            "Die Sandbox-Quellen überschreiten 20.000 Dateien oder 128 MiB",
        ),
    ),
    (
        107,
        (
            "Submodule nesting exceeds the eight-level source-export limit",
            "Submodule nesting exceeds the eight-level source-export limit",
            "Die Submodulverschachtelung überschreitet das Quellexportlimit von acht Ebenen",
        ),
    ),
    (
        108,
        (
            "Malformed raw tree record",
            "Malformed raw tree record",
            "Fehlerhafter Datensatz im Git-Rohbaum",
        ),
    ),
    (
        109,
        (
            "Invalid raw tree metadata",
            "Invalid raw tree metadata",
            "Ungültige Metadaten im Git-Rohbaum",
        ),
    ),
    (
        110,
        (
            "Malformed raw tree metadata",
            "Malformed raw tree metadata",
            "Fehlerhafte Metadaten im Git-Rohbaum",
        ),
    ),
    (
        111,
        (
            "Raw tree contains an invalid object ID",
            "Raw tree contains an invalid object ID",
            "Der Git-Rohbaum enthält eine ungültige Objekt-ID",
        ),
    ),
    (
        112,
        (
            "Sandbox source paths must be UTF-8",
            "Sandbox source paths must be UTF-8",
            "Sandbox-Quellpfade müssen UTF-8 verwenden",
        ),
    ),
    (
        113,
        (
            "Unsupported raw tree object mode",
            "Unsupported raw tree object mode",
            "Nicht unterstützter Objektmodus im Git-Rohbaum",
        ),
    ),
    (
        114,
        (
            "Invalid raw blob size",
            "Invalid raw blob size",
            "Ungültige Größe des Git-Rohblobs",
        ),
    ),
    (
        115,
        (
            "Sandbox source exceeds 16MiB per blob or 128MiB total",
            "Sandbox source exceeds 16MiB per blob or 128MiB total",
            "Die Sandbox-Quellen überschreiten 16 MiB pro Blob oder insgesamt 128 MiB",
        ),
    ),
    (
        116,
        (
            "Raw blob size changed or output was truncated",
            "Raw blob size changed or output was truncated",
            "Die Größe des Git-Rohblobs wurde geändert oder die Ausgabe wurde abgeschnitten",
        ),
    ),
    (
        117,
        (
            "Duplicate raw source path",
            "Duplicate raw source path",
            "Doppelter Pfad im direkten Quellexport",
        ),
    ),
    (
        118,
        (
            "Sandbox symlink targets must be UTF-8",
            "Sandbox symlink targets must be UTF-8",
            "Sandbox-Symlink-Ziele müssen UTF-8 verwenden",
        ),
    ),
    (
        119,
        (
            "Symlink points into Git metadata",
            "Symlink points into Git metadata",
            "Symlink verweist auf Git-Metadaten",
        ),
    ),
    (
        120,
        (
            "Archive field overflow",
            "Archive field overflow",
            "Überlauf eines Archivfelds",
        ),
    ),
    (
        121,
        (
            "Archive header requires a PAX path record",
            "Archive header requires a PAX path record",
            "Der Archiv-Header erfordert einen PAX-Pfaddatensatz",
        ),
    ),
];
