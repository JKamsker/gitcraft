use super::*;
#[test]
fn actual_completion_text_uses_typed_payload_and_keeps_landed_paths_notes_and_bytes_unchanged() {
    let raw_lfs = "Files kept in Git LFS arrived as their pointers; the palette's LFS: Pull fetches those that were pushed.";
    let landed = crate::teleport::Landed {
        folder: "/opaque/path $() 日本語".into(),
        size: 1536,
        notes: vec![
            raw_lfs.into(),
            "External detail { $count }\nopaque second line".into(),
        ],
    };
    crate::i18n::set_language("de");
    let de = completion_message(&landed, &Machine::Here);
    assert!(
        de.starts_with("Teleportiert. Ziel: dieser Rechner (1,5 KB): /opaque/path $() 日本語\n")
    );
    assert!(de.contains("In Git LFS gespeicherte Dateien sind als Zeigerdateien angekommen;"));
    assert!(de.ends_with("External detail { $count }\nopaque second line"));
    assert_eq!(landed.notes[0], raw_lfs);
    assert_eq!(landed.size, 1536);
    assert_eq!(
        landed.folder,
        std::path::PathBuf::from("/opaque/path $() 日本語")
    );
    crate::i18n::set_language("en-US");
    assert_eq!(
        completion_message(&landed, &Machine::Here),
        format!(
            "Teleported to this machine (1.5 KB): /opaque/path $() 日本語\n{raw_lfs}\nExternal detail {{ $count }}\nopaque second line"
        )
    );
}
