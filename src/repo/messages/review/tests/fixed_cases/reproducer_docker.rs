pub(super) const ROWS: &[(usize, (&str, &str, &str))] = &[
    (
        81,
        (
            "Invalid immutable image/owner identity",
            "Invalid immutable image/owner identity",
            "Ungültige unveränderliche Image-/Eigentümeridentität",
        ),
    ),
    (
        82,
        (
            "Container ownership inspection was interrupted",
            "Container ownership inspection was interrupted",
            "Prüfung des Container-Eigentümers wurde unterbrochen",
        ),
    ),
    (
        83,
        (
            "Invalid container ownership response",
            "Invalid container ownership response",
            "Ungültige Antwort zum Container-Eigentümer",
        ),
    ),
    (
        84,
        (
            "Invalid immutable container ownership response",
            "Invalid immutable container ownership response",
            "Ungültige Antwort zum unveränderlichen Container-Eigentümer",
        ),
    ),
    (
        85,
        (
            "Name belongs to another owner; cleanup refused",
            "Name belongs to another owner; cleanup refused",
            "Der Name gehört einem anderen Eigentümer; Bereinigung verweigert",
        ),
    ),
    (
        86,
        (
            "Creation outcome is unknown; no owned ID was found. A stopped container may remain if creation finished after inspection; no removal by name was attempted.",
            "Creation outcome is unknown; no owned ID was found. A stopped container may remain if creation finished after inspection; no removal by name was attempted.",
            "Das Erstellungsergebnis ist unbekannt; keine eigene ID gefunden. Wenn die Erstellung nach der Prüfung abgeschlossen wurde, kann ein gestoppter Container verbleiben; eine Entfernung anhand des Namens wurde nicht versucht.",
        ),
    ),
    (
        87,
        (
            "No immutable container ID",
            "No immutable container ID",
            "Keine unveränderliche Container-ID",
        ),
    ),
    (
        88,
        (
            "Invalid cleanup ID",
            "Invalid cleanup ID",
            "Ungültige Bereinigungs-ID",
        ),
    ),
    (
        89,
        (
            "Container was already absent",
            "Container was already absent",
            "Container war bereits nicht vorhanden",
        ),
    ),
    (
        90,
        (
            "Immutable container owner/ID mismatch; cleanup refused",
            "Immutable container owner/ID mismatch; cleanup refused",
            "Unveränderlicher Container-Eigentümer und ID stimmen nicht überein; Bereinigung verweigert",
        ),
    ),
    (
        91,
        (
            "Container cleanup",
            "Container cleanup",
            "Container-Bereinigung",
        ),
    ),
    (
        92,
        (
            "Owned container removed by immutable ID",
            "Owned container removed by immutable ID",
            "Eigener Container anhand der unveränderlichen ID entfernt",
        ),
    ),
    (
        93,
        (
            "Choose a preloaded Rust image by name or digest",
            "Choose a preloaded Rust image by name or digest",
            "Wähle ein vorab geladenes Rust-Image nach Name oder Digest",
        ),
    ),
    (
        94,
        (
            "Inspect preloaded image",
            "Inspect preloaded image",
            "Vorab geladenes Image prüfen",
        ),
    ),
    (
        95,
        ("Invalid image ID", "Invalid image ID", "Ungültige Image-ID"),
    ),
    (
        96,
        (
            "Selected image is unavailable locally or did not return an immutable SHA256 ID; images are never pulled",
            "Selected image is unavailable locally or did not return an immutable SHA256 ID; images are never pulled",
            "Das ausgewählte Image ist lokal nicht verfügbar oder hat keine unveränderliche SHA256-ID geliefert; Images werden niemals heruntergeladen",
        ),
    ),
    (
        97,
        ("Create sandbox", "Create sandbox", "Sandbox erstellen"),
    ),
    (
        98,
        (
            "Invalid container creation response",
            "Invalid container creation response",
            "Ungültige Antwort bei der Container-Erstellung",
        ),
    ),
    (
        99,
        (
            "Docker did not return a complete immutable container ID",
            "Docker did not return a complete immutable container ID",
            "Docker hat keine vollständige unveränderliche Container-ID geliefert",
        ),
    ),
    (
        100,
        (
            "Created container ownership could not be established; execution refused",
            "Created container ownership could not be established; execution refused",
            "Der Eigentümer des erstellten Containers ließ sich nicht feststellen; Ausführung verweigert",
        ),
    ),
    (
        101,
        (
            "Read sandbox exit status",
            "Read sandbox exit status",
            "Sandbox-Exitstatus lesen",
        ),
    ),
    (
        102,
        (
            "Invalid sandbox exit status",
            "Invalid sandbox exit status",
            "Ungültiger Sandbox-Exitstatus",
        ),
    ),
    (
        103,
        (
            "Sandbox exit status is outside Docker's range",
            "Sandbox exit status is outside Docker's range",
            "Der Sandbox-Exitstatus liegt außerhalb des Docker-Wertebereichs",
        ),
    ),
];
