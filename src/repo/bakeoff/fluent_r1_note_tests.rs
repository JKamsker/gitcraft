use super::*;
#[test]
fn fluent_partition_r1_bakeoff_same_note_keeps_picks_request_and_opaque_branch()
-> Result<(), String> {
    for note in [
        BakeNote::Unassembled { count: 1234 },
        BakeNote::Changed,
        BakeNote::CommandMissing,
        BakeNote::Made {
            branch: "raw/{$x}:日本語".into(),
            commit: "1234abcd".into(),
        },
    ] {
        crate::i18n::set_language("de");
        let mut view = Bakeoff::new(vec![], "printf raw".into());
        view.remember_note(note, true);
        let raw = view.note.clone().ok_or("missing test value")?.0;
        let request = view.request;
        for lang in ["de", "en-US", "de"] {
            crate::i18n::set_language(lang);
            if lang == "de" {
                assert_ne!(view.display_note(&raw), raw);
            }
            assert_eq!(view.note.as_ref().ok_or("missing test value")?.0, raw);
            assert_eq!(view.request, request);
            assert_eq!(view.build_command, "printf raw");
            assert!(view.picks.is_empty());
            assert!(!view.assembling);
        }
        view.note_presentation = None;
        assert_eq!(view.display_note(&raw), raw);
    }
    crate::i18n::set_language("en-US");

    Ok(())
}
