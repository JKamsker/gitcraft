use super::*;

#[test]
fn lists_cut_with_how_many_more() {
    let items: Vec<String> = (1..=7).map(|n| n.to_string()).collect();
    assert_eq!(listed(&items, String::clone), "1, 2, 3, 4, 5, and 2 more");
    assert_eq!(listed(&items[..2], String::clone), "1, 2");
    crate::i18n::set_language("en-US");
    assert_eq!(ghost_count(1, false), "one commit");
    assert_eq!(ghost_count(3, false), "3 commits");
    crate::i18n::set_language("de");
    assert_eq!(ghost_count(1234, true), "mehr als 1.234 Commits");
    assert_eq!(sentence("x isn't merged"), "X isn't merged");
}
