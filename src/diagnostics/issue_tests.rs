#[test]
fn issue_link_is_encoded_bounded_and_preserves_full_lines() {
    let url = super::issue_url("A & B?\n".repeat(5000).as_str());
    assert!(url.starts_with("https://github.com/JKamsker/gitcraft/issues/new?"));
    assert!(url.contains("%26") && url.len() < 7200 && !url.contains('\n'));
    assert!(url.contains("clipboard"));
}
