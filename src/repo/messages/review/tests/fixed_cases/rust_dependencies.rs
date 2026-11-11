pub(super) const ROWS: &[(usize, (&str, &str, &str))] = &[
    (
        122,
        (
            "Non-UTF-8 or newline-containing Rust paths were not analyzed",
            "Non-UTF-8 or newline-containing Rust paths were not analyzed",
            "Rust-Pfade ohne UTF-8 oder mit Zeilenumbrüchen wurden nicht analysiert",
        ),
    ),
    (
        123,
        (
            "More than 4096 Rust files; dependency analysis was not performed",
            "More than 4096 Rust files; dependency analysis was not performed",
            "Mehr als 4096 Rust-Dateien; Abhängigkeitsanalyse nicht durchgeführt",
        ),
    ),
    (
        124,
        (
            "Rust sources exceed the 32MiB analysis budget; dependencies unchecked",
            "Rust sources exceed the 32MiB analysis budget; dependencies unchecked",
            "Die Rust-Quellen überschreiten das Analyselimit von 32 MiB; Abhängigkeiten ungeprüft",
        ),
    ),
    (
        125,
        (
            "Incomplete Rust blob header",
            "Incomplete Rust blob header",
            "Unvollständiger Rust-Blob-Header",
        ),
    ),
    (
        126,
        (
            "Invalid Rust blob size",
            "Invalid Rust blob size",
            "Ungültige Rust-Blob-Größe",
        ),
    ),
    (
        127,
        (
            "A Rust source is not a blob",
            "A Rust source is not a blob",
            "Eine Rust-Quelle ist kein Blob",
        ),
    ),
    (
        128,
        (
            "Incomplete Rust blob",
            "Incomplete Rust blob",
            "Unvollständiger Rust-Blob",
        ),
    ),
    (
        129,
        (
            "Incomplete Rust blob separator",
            "Incomplete Rust blob separator",
            "Unvollständiges Rust-Blob-Trennzeichen",
        ),
    ),
    (
        130,
        (
            "Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types",
            "Rust syntax evidence does not resolve cfg features, custom #[path] modules, re-exports, or types",
            "Der Rust-Syntaxnachweis löst cfg-Features, benutzerdefinierte #[path]-Module, Re-Exporte oder Typen nicht auf",
        ),
    ),
    (
        131,
        (
            "Static call traversal exceeded 1024 symbols",
            "Static call traversal exceeded 1024 symbols",
            "Die Verfolgung statischer Aufrufe hat 1024 Symbole überschritten",
        ),
    ),
    (
        132,
        (
            "Dynamic, external, local-import, or macro-generated calls remain unchecked",
            "Dynamic, external, local-import, or macro-generated calls remain unchecked",
            "Dynamische, externe, lokal importierte oder durch Makros erzeugte Aufrufe bleiben ungeprüft",
        ),
    ),
    (
        133,
        (
            "Duplicate/cfg-dependent symbol identities were not resolved",
            "Duplicate/cfg-dependent symbol identities were not resolved",
            "Doppelte oder cfg-abhängige Symbolidentitäten wurden nicht aufgelöst",
        ),
    ),
    (
        134,
        (
            "No unchanged uniquely identified Rust functions were analyzed",
            "No unchanged uniquely identified Rust functions were analyzed",
            "Keine unveränderten eindeutig identifizierten Rust-Funktionen analysiert",
        ),
    ),
];
