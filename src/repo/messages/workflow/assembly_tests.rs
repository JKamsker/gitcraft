use super::ordered;

#[test]
fn domain_rows_retain_original_precedence() {
    const ROWS: [u8; 4] = ordered(&[&[(2, 30), (0, 10)], &[(3, 40), (1, 20)]]);
    assert_eq!(ROWS, [10, 20, 30, 40]);
}

#[test]
#[should_panic]
fn duplicate_ordinals_are_rejected() {
    let _: [u8; 2] = ordered(&[&[(0, 10), (0, 20)]]);
}

#[test]
#[should_panic]
fn missing_rows_are_rejected() {
    let _: [u8; 2] = ordered(&[&[(0, 10)]]);
}
