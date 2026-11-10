#[test]
fn backend_fluent_relative_time_uses_numeric_plurals_and_preserves_time_boundaries() {
    let units = [60, 3_600, 86_400, 604_800, 2_629_800, 31_557_600];
    for (locale, singular, plural) in [
        (
            "en-US",
            [
                "1 minute ago",
                "1 hour ago",
                "1 day ago",
                "1 week ago",
                "1 month ago",
                "1 year ago",
            ],
            [
                "2 minutes ago",
                "2 hours ago",
                "2 days ago",
                "2 weeks ago",
                "2 months ago",
                "2 years ago",
            ],
        ),
        (
            "de",
            [
                "vor 1 Minute",
                "vor 1 Stunde",
                "vor 1 Tag",
                "vor 1 Woche",
                "vor 1 Monat",
                "vor 1 Jahr",
            ],
            [
                "vor 2 Minuten",
                "vor 2 Stunden",
                "vor 2 Tagen",
                "vor 2 Wochen",
                "vor 2 Monaten",
                "vor 2 Jahren",
            ],
        ),
    ] {
        crate::i18n::set_language(locale);
        for (i, seconds) in units.into_iter().enumerate() {
            assert_eq!(super::relative_time(0, seconds), singular[i]);
            assert_eq!(super::relative_time(0, 2 * seconds), plural[i]);
        }
        let just_now = if locale == "de" {
            "Gerade eben"
        } else {
            "just now"
        };
        assert_eq!(super::relative_time(0, 59), just_now);
        assert_eq!(super::relative_time(100, 0), just_now);
        assert_eq!(super::relative_time(i64::MAX, i64::MIN), just_now);
        assert!(!super::relative_time(i64::MIN, i64::MAX).is_empty());
    }
    crate::i18n::set_language("en-US");
}
