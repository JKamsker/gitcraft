use super::*;

#[test]
fn actual_size_guard_and_failure_combinators_retain_raw_locale_without_archive_execution()
-> Result<(), String> {
    crate::i18n::set_language("de");
    assert!(check_size(max()).is_ok());
    let large = check_size(max() + 1).err().ok_or("source size guard")?;
    assert!(matches!(large, Failure::ArchiveTooBig { .. }));
    assert_eq!(
        large.raw(),
        "the archive is bigger than 200,0 MB: not compared"
    );
    let gzip = gzip_failure("a damaged archive; ; /1234.5".into());
    let damaged = damaged_failure();
    for locale in ["en-US", "de", "en-US"] {
        crate::i18n::set_language(locale);
        assert_eq!(
            large.display(),
            if locale == "de" {
                "das Archiv ist größer als 200,0 MB: kein Vergleich"
            } else {
                "the archive is bigger than 200.0 MB: not compared"
            }
        );
        assert_eq!(
            large.raw(),
            "the archive is bigger than 200,0 MB: not compared"
        );
        assert!(gzip.display().ends_with("a damaged archive; ; /1234.5"));
        assert_eq!(damaged.display() == damaged.raw(), locale == "en-US");
    }

    Ok(())
}
