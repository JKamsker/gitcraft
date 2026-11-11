use super::*;

#[test]
fn hex_colours_read_in_every_length() {
    assert_eq!(parse_hex("#fff"), Some(Color32::WHITE));
    assert_eq!(parse_hex("1e1e2e"), Some(hex(0x1e1e2e)));
    assert_eq!(
        parse_hex("#ff000080"),
        Some(Color32::from_rgba_unmultiplied(255, 0, 0, 128))
    );
    assert_eq!(parse_hex("#12"), None);
    assert_eq!(parse_hex("#gggggg"), None);
    assert_eq!(to_hex(hex(0x89b4fa)), "#89b4fa");
}

#[test]
fn accent_text_is_whichever_reads_better() {
    assert_eq!(readable_on(hex(0x1f6feb)), Color32::WHITE);
    assert_ne!(readable_on(hex(0xf1fa8c)), Color32::WHITE);
}

#[test]
fn translucent_colours_are_laid_over_their_background() {
    let half_white = Color32::from_rgba_unmultiplied(255, 255, 255, 128);
    let c = over(half_white, Color32::BLACK);
    assert!((127..=129).contains(&c.r()));
    assert_eq!(over(Color32::RED, Color32::BLACK), Color32::RED);
}
