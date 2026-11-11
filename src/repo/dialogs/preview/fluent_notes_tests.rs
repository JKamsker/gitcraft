use super::*;

#[test]
fn rewrite_note_uses_whole_plural_message_and_opaque_upstream() {
    crate::i18n::set_language("en-US");
    assert_eq!(
        pushed_note("origin/Commit 日本語", 1),
        "Already pushed: 1 commit this rewrites is on origin/Commit 日本語, so pushing it again will need a force push."
    );
    crate::i18n::set_language("de");
    assert_eq!(
        pushed_note("origin/Commit 日本語", 1),
        "Ein hiervon umgeschriebener Commit wurde bereits nach origin/Commit 日本語 gepusht; erneutes Pushen erfordert daher einen Force-Push."
    );
    assert_eq!(
        pushed_note("origin/Commit 日本語", 1234),
        "1.234 hiervon umgeschriebene Commits wurden bereits nach origin/Commit 日本語 gepusht; erneutes Pushen erfordert daher einen Force-Push."
    );
    crate::i18n::set_language("en-US");
}
