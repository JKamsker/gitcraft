use super::*;

#[test]
fn a_notification_shows_tags_as_text() {
    let body = notification_body("push failed: remote: <a href=\"https://x\">see</a> & more");
    if cfg!(all(unix, not(target_os = "macos"))) {
        assert_eq!(
            body,
            "push failed: remote: &lt;a href=\"https://x\"&gt;see&lt;/a&gt; &amp; more"
        );
    } else {
        assert!(body.contains("<a href"));
    }
}
