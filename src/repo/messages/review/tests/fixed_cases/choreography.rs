pub(super) const ROWS: &[(usize, (&str, &str, &str))] = &[
    (
        0,
        (
            "Add 2–6 repositories to the release board",
            "Add 2–6 repositories to the release board",
            "Füge dem Release-Board 2–6 Repositorys hinzu",
        ),
    ),
    (
        1,
        (
            "Component names must be unique ASCII identifiers, ignoring case",
            "Component names must be unique ASCII identifiers, ignoring case",
            "Komponentennamen müssen eindeutige ASCII-Kennungen sein, unabhängig von Groß- und Kleinschreibung",
        ),
    ),
    (
        2,
        (
            "Use absolute repository paths or encoded SSH/WSL paths",
            "Use absolute repository paths or encoded SSH/WSL paths",
            "Verwende absolute Repository-Pfade oder kodierte SSH-/WSL-Pfade",
        ),
    ),
    (
        3,
        (
            "Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host",
            "Rehearse linked repositories on the same machine; choose local copies or paths on one shared SSH/WSL host",
            "Erprobe verknüpfte Repositorys auf demselben Rechner; wähle lokale Kopien oder Pfade auf einem gemeinsamen SSH-/WSL-Host",
        ),
    ),
    (
        4,
        (
            "Too many combinations",
            "Too many combinations",
            "Zu viele Kombinationen",
        ),
    ),
    (
        5,
        (
            "More than 256 in-between combinations: split this release into smaller boards",
            "More than 256 in-between combinations: split this release into smaller boards",
            "Mehr als 256 Zwischenkombinationen: Teile dieses Release in kleinere Boards auf",
        ),
    ),
    (
        6,
        (
            "A rule names an unknown dependent component",
            "A rule names an unknown dependent component",
            "Eine Regel nennt eine unbekannte abhängige Komponente",
        ),
    ),
    (
        7,
        (
            "A rule names an unknown prerequisite component",
            "A rule names an unknown prerequisite component",
            "Eine Regel nennt eine unbekannte vorausgesetzte Komponente",
        ),
    ),
    (
        8,
        (
            "Rule stage thresholds must name non-deployed stages present on their components",
            "Rule stage thresholds must name non-deployed stages present on their components",
            "Stufenschwellen der Regeln müssen vorhandene, noch nicht ausgelieferte Stufen der jeweiligen Komponenten bezeichnen",
        ),
    ),
    (
        9,
        (
            "Stopped before constructing the full combination",
            "Stopped before constructing the full combination",
            "Vor dem Aufbau der vollständigen Kombination gestoppt",
        ),
    ),
    (
        10,
        (
            "Enter and explicitly trust your compatibility command before rehearsal",
            "Enter and explicitly trust your compatibility command before rehearsal",
            "Gib vor der Probe deinen Kompatibilitätsbefehl ein und bestätige ausdrücklich dein Vertrauen",
        ),
    ),
    (
        11,
        (
            "Not tested: rehearsal stopped",
            "Not tested: rehearsal stopped",
            "Nicht getestet: Probe gestoppt",
        ),
    ),
];
