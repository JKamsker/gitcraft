use super::*;

#[test]
fn long_and_multiline_values_are_cut_in_rows() {
    assert_eq!(shortened("true"), "true");
    assert_eq!(shortened("a\nb"), "a…");
    let long = "é".repeat(60);
    assert_eq!(shortened(&long).chars().count(), ROW_VALUE + 1);
    assert_eq!(shortened(""), "");
}
