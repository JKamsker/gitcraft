use super::*;

const ANN: (&str, &str) = ("Ann Lee", "Ann@Example.org");

#[test]
fn a_sign_off_counts_with_the_name_and_email_in_any_case() {
    let msg = "fix: x\n\nbody\n\nSigned-off-by: Ann Lee <ann@example.org>\n";
    assert!(signed_off_by(msg, &[ANN]));
    assert!(signed_off_by(msg, &[("ANN LEE", "ann@EXAMPLE.org")]));
    assert!(!signed_off_by(msg, &[("Bob", "bob@example.org")]));
    assert!(signed_off_by(
        "x\n\nsigned-off-by: Ann Lee <ann@example.org>",
        &[ANN]
    ));
}

/// As the DCO app reads them: one space after the colon, the line ending with `>`, and the
/// name too.
#[test]
fn a_sign_off_is_read_as_the_dco_check_reads_it() {
    assert!(!signed_off_by(
        "x\n\nSigned-off-by:Ann Lee <ann@example.org>",
        &[ANN]
    ));
    assert!(!signed_off_by(
        "x\n\n  Signed-off-by: Ann Lee <ann@example.org>",
        &[ANN]
    ));
    assert!(!signed_off_by(
        "x\n\nSigned-off-by: Ann Lee <ann@example.org> ok",
        &[ANN]
    ));
    assert!(!signed_off_by(
        "x\n\nSigned-off-by: Ann <ann@example.org>",
        &[ANN]
    ));
    assert!(signed_off_by(
        "x\n\nSigned-off-by: Ann Lee <ann@example.org>   ",
        &[ANN]
    ));
    assert!(!signed_off_by("x\n\nSigned-off-by: Ann Lee", &[ANN]));
    assert!(!signed_off_by(
        "x\n\nCo-authored-by: Ann Lee <ann@example.org>",
        &[ANN]
    ));
    assert!(!signed_off_by("", &[ANN]));
    assert!(!signed_off_by(
        "x\n\nSigned-off-by: Ann Lee <>",
        &[("Ann Lee", "")]
    ));
}

#[test]
fn unicode_names_and_emails_are_read() {
    let msg = "feat: ü\n\nSigned-off-by: Zoë Ångström <zoë@exämple.org>";
    assert!(signed_off_by(msg, &[("Zoë Ångström", "zoë@exämple.org")]));
}

#[test]
fn the_log_is_read_into_commits() {
    let out = "\
aaa\u{1f}p1\u{1f}Ann\u{1f}ann@e\u{1f}Ann\u{1f}ann@e\u{1f}one\u{1f}one\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
bbb\u{1f}p1 p2\u{1f}Bob\u{1f}bob@e\u{1f}Bob\u{1f}bob@e\u{1f}Merge\u{1f}Merge\n\u{1e}\n\
ccc\u{1f}p1\u{1f}Bob\u{1f}bob@e\u{1f}Bob\u{1f}bob@e\u{1f}two\u{1f}two\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
ddd\u{1f}p1\u{1f}Bob\u{1f}bob@e\u{1f}Ann\u{1f}ann@e\u{1f}three\u{1f}three\n\nSigned-off-by: Ann <ann@e>\n\u{1e}\n\
eee\u{1f}p1\u{1f}dependabot[bot]\u{1f}b@e\u{1f}x\u{1f}x@e\u{1f}bump\u{1f}bump\n\u{1e}";
    let list = parse_commits(out);
    assert_eq!(list.len(), 5);
    assert!(list[0].signed && !list[0].merge);
    assert!(list[1].merge);
    assert!(
        !list[2].signed,
        "signed by someone else than its author and committer"
    );
    assert!(list[3].signed, "signed by its committer");
    assert!(list[4].bot);
    let missing: Vec<&str> = missing(&list).map(|c| c.hash.as_str()).collect();
    assert_eq!(missing, ["ccc"]);
}

#[test]
fn dco_checks_are_known_by_name() {
    assert!(is_dco_check("DCO"));
    assert!(is_dco_check("dco"));
    assert!(is_dco_check("probot/dco"));
    assert!(is_dco_check("DCO check"));
    assert!(is_dco_check("ci / DCO"));
    assert!(!is_dco_check("deco"));
    assert!(!is_dco_check("build"));
}

#[test]
fn the_choice_is_kept_or_taken_out() {
    assert_eq!(
        choice_command(Some(true)),
        ["config", "--local", SIGN_OFF_KEY, "true"]
    );
    assert_eq!(
        choice_command(None),
        ["config", "--local", "--unset-all", SIGN_OFF_KEY]
    );
}
