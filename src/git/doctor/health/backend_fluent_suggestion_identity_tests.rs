#[test]
fn backend_fluent_doctor_titles_keep_control_identity() {
    use super::{Health, Objects, State, Suggestion, suggestions};
    let health = Health {
        objects: Objects {
            loose: 5000,
            ..Default::default()
        },
        fsmonitor: Some(State::Unsupported),
        ..Default::default()
    };
    let raw = [
        "Commit-graph",
        "Scheduled maintenance",
        "File system monitor",
        "Untracked cache",
        "Pack the objects",
    ];
    let german = [
        "Commit-Graph",
        "Geplante Wartung",
        "Dateisystemmonitor",
        "Cache unversionierter Dateien",
        "Objekte packen",
    ];
    crate::i18n::set_language("en-US");
    let operations: Vec<_> = suggestions(&health).into_iter().map(|s| s.ops).collect();
    for language in ["en-US", "de"] {
        crate::i18n::set_language(language);
        for (index, suggestion) in suggestions(&health).iter().enumerate() {
            assert_eq!(suggestion.title, raw[index]);
            assert_eq!(suggestion.ops, operations[index]);
            assert_eq!(
                suggestion.display_title(),
                if language == "de" {
                    german[index]
                } else {
                    raw[index]
                }
            );
        }
        let unknown = Suggestion {
            title: "opaque external title",
            why: String::new(),
            state: State::Off,
            ops: Vec::new(),
        };
        assert_eq!(unknown.display_title(), unknown.title);
    }
    crate::i18n::set_language("en-US");
}
