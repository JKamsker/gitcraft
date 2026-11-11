use super::*;

#[test]
fn numbers_read_well() {
    assert_eq!(group(100_000), "100 000");
    assert_eq!(group(999), "999");
    assert_eq!(group(1_234_567), "1 234 567");
    assert_eq!(group(0), "0");
    assert_eq!(size(512), "512 KiB");
    assert_eq!(size(2048), "2.0 MiB");
    assert_eq!(size(3 * 1_048_576), "3.00 GiB");
}
