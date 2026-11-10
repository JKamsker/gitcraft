use super::*;
#[test]
fn fluent_partition_actual_month_heading_preserves_iso_calendar_identity() {
    let day = Day {
        year: 2026,
        month: 3,
        day: 12,
    };
    for (lang, expected) in [("en-US", "March 2026"), ("de", "März 2026")] {
        crate::i18n::set_language(lang);
        assert_eq!(month_heading(day), expected);
        assert_eq!(day.to_string(), "2026-03-12");
        assert_eq!(
            crate::i18n::number(1234usize),
            if lang == "de" { "1.234" } else { "1,234" }
        );
    }
    crate::i18n::set_language("en-US");
}
