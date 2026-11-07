use super::*;
#[test]
fn verify_pack_parser_keeps_actual_delta_numbers() -> Result<(), String> {
    let a = "a".repeat(40); let b = "b".repeat(40);
    let rows = pack_rows(&format!("{a} blob 9000 22 12 1 {b}\n{b} blob 8990 38 34\nnon delta: 1 object\n"))?;
    assert_eq!(rows[&a].disk, 22);
    assert_eq!(rows[&a].base.as_ref(), Some(&b));
    assert_eq!(rows[&a].depth, 1);
    assert_eq!(rows[&b].offset, 34);
    assert!(pack_rows(&format!("{a} blob 2 3 4 1 bad\n")).is_err());
    Ok(())
}
