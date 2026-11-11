use super::*;

#[test]
fn anchored_details_keep_lookalike_cleanup_and_exit_phrases_opaque() {
    i18n::set_language("de");
    for detail in [
        "provider text; quarantine cleanup failed: provider detail",
        "provider text; workspace cleanup: provider detail",
        "provider text (exit 2345); stderr: provider detail",
        "provider/path on provider-host is not a git repository",
        "provider text; owned pack-01234567.keep retained",
        "provider text\ntrailing newline\n",
    ] {
        for prefix in [
            "could not read stopped test log: ",
            "Observation cleanup unknown: ",
        ] {
            let raw = format!("{prefix}{detail}");
            assert!(
                diagnostic(&raw).contains(detail),
                "{raw} -> {}",
                diagnostic(&raw)
            );
        }
    }
    let decoder = "provider (exit 7); stderr: decoder raw data";
    let raw = format!(
        "PNG frame could not be decoded (maximum 1024×1024): {decoder} (exit 2345); stderr: actual stderr"
    );
    assert!(diagnostic(&raw).contains(decoder));
    assert_eq!(
        observation_detail(&format!(
            "PNG frame could not be decoded (maximum 1024×1024): {decoder}"
        )),
        format!("Der PNG-Frame konnte nicht dekodiert werden (maximal 1024×1024): {decoder}")
    );
    i18n::set_language("en-US");
}
