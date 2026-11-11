pub(super) const ROWS: &[(usize, (&str, &str, &str))] = &[
    (
        43,
        (
            "Reviewed push already has execution evidence; resume its retained operation",
            "Reviewed push already has execution evidence; resume its retained operation",
            "Der geprüfte Push hat bereits einen Ausführungsnachweis; setze die gespeicherte Operation fort",
        ),
    ),
    (
        44,
        (
            "Push destination is not unambiguous; review its source manually",
            "Push destination is not unambiguous; review its source manually",
            "Das Push-Ziel ist nicht eindeutig; prüfe seine Quelle manuell",
        ),
    ),
    (
        45,
        (
            "This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation",
            "This operation changes state before pushing. Run the review check after those changes, or explicitly review and continue this operation",
            "Diese Operation ändert den Zustand vor dem Push. Führe die Review-Prüfung nach diesen Änderungen aus oder prüfe diese Operation ausdrücklich und setze sie fort",
        ),
    ),
    (
        46,
        (
            "Invalid wildcard source",
            "Invalid wildcard source",
            "Ungültige Platzhalterquelle",
        ),
    ),
    (
        47,
        (
            "Unsupported push wildcard; review it explicitly",
            "Unsupported push wildcard; review it explicitly",
            "Nicht unterstützter Push-Platzhalter; prüfe ihn ausdrücklich",
        ),
    ),
    (
        48,
        (
            "Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review",
            "Reviewer echo cannot inspect context-changing or unknown Git prefixes; this operation requires explicit manual review",
            "Das Reviewer-Echo kann kontextändernde oder unbekannte Git-Präfixe nicht untersuchen; diese Operation erfordert eine ausdrückliche manuelle Prüfung",
        ),
    ),
];
