#[test]
fn actual_app_message_paint_uses_canonical_remote_adapter_after_own_classifiers() {
    crate::i18n::set_language("de");
    let raw = "it is a symbolic link";
    assert_eq!(
        super::display_app_message(raw),
        "Dies ist ein symbolischer Link."
    );
    assert_eq!(raw, "it is a symbolic link");
    assert_eq!(
        super::display_app_message("gitcraft isn't allowed to look in /opaque/日本語."),
        "gitcraft darf /opaque/日本語 nicht durchsuchen."
    );
    let unknown = "opaque external 日本語\n: unchanged";
    assert_eq!(super::display_app_message(unknown), unknown);
    // No command/Host provenance is available at this generic paint boundary.
    let command = "opaque command on SSH · opaque: invalid native request";
    assert_eq!(super::display_app_message(command), command);
    crate::i18n::set_language("en-US");
    assert_eq!(super::display_app_message(raw), raw);
}
