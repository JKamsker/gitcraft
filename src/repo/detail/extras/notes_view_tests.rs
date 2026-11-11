use super::*;

#[test]
fn the_header_shows_the_first_line_with_text() {
    assert_eq!(first_line("\n  \n  Reviewed ✓ \nmore"), "Reviewed ✓");
    assert_eq!(first_line(""), "");
}
