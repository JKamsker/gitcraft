use super::*;

#[test]
fn home_source_toast_keeps_delivery_text_error_flag_and_no_extra_actions() {
    crate::i18n::set_language("de");
    let error = super::super::AppError::init("opaque native 日本語".into());
    let raw = error.to_string();
    let toast = Toast::from_app_error(&error);
    let until = toast.until;
    assert_eq!(raw, "git init failed: opaque native 日本語");
    assert_eq!(toast.text, "git init fehlgeschlagen: opaque native 日本語");
    assert!(toast.error);
    assert!(toast.notice.is_none() && toast.update_go_back.is_none());
    crate::i18n::set_language("en-US");
    assert_eq!(
        toast.display_text(),
        "git init fehlgeschlagen: opaque native 日本語"
    );
    assert_eq!(toast.until, until);
    assert_eq!(error.to_string(), raw);
    let plain = super::super::AppError::from(raw.clone());
    let plain_toast = Toast::from_app_error(&plain);
    assert_eq!(plain_toast.text, raw);
    assert!(plain_toast.error);
    assert!(plain_toast.notice.is_none() && plain_toast.update_go_back.is_none());
}
