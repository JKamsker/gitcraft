use super::*;

#[test]
fn reads_check_ignore_lines() {
    assert_eq!(
        parse_check_ignore("sub/.gitignore:2:!keep.log\tsub/keep.log"),
        Some(Rule {
            source: "sub/.gitignore".into(),
            line: 2,
            pattern: "!keep.log".into(),
        })
    );
    assert!(parse_check_ignore("::\ta.log").is_none());
    // A Windows path, a pattern with colons and a quoted path with a tab in it.
    let rule = parse_check_ignore("C:/Users/me/.config/git/ignore:12:a:b:*.x\t\"we\\tird\"");
    assert_eq!(
        rule,
        Some(Rule {
            source: "C:/Users/me/.config/git/ignore".into(),
            line: 12,
            pattern: "a:b:*.x".into(),
        })
    );
    assert!(parse_check_ignore("").is_none());
    assert!(parse_check_ignore("no tab here").is_none());
}

#[test]
fn reads_attributes_and_line_endings() {
    let attrs = parse_check_attr("a.png\0binary\0set\0a.png\0diff\0unset\0a.png\0text\0unset\0");
    assert_eq!(attrs.len(), 3);
    assert_eq!(attrs[0].name, "binary");
    assert!(parse_check_attr("").is_empty());
    let eol = parse_eol("i/lf    w/crlf  attr/text=auto eol=lf \tdir/é b.txt");
    assert_eq!(
        eol,
        Some(Eol {
            index: "lf".into(),
            worktree: "crlf".into(),
            attr: "text=auto eol=lf".into(),
        })
    );
    let untracked = parse_eol("        w/lf    attr/                 \tnew.txt");
    assert_eq!(
        untracked.map(|e| (e.index, e.worktree)),
        Some((String::new(), "lf".into()))
    );
}

#[test]
fn typed_paths_are_cleaned_or_refused() {
    let top = "/home/me/my repo";
    assert_eq!(
        clean_path("./src\\a b.rs\n", top),
        Ok("src/a b.rs".to_owned())
    );
    assert_eq!(clean_path("src/", top), Ok("src".to_owned()));
    // Absolute inside the repository: made relative; elsewhere: refused.
    assert_eq!(
        clean_path("/home/me/my repo/é.txt", top),
        Ok("é.txt".to_owned())
    );
    assert!(clean_path("/home/me/other/x", top).is_err());
    assert!(clean_path("/home/me/my repository/x", top).is_err());
    assert!(clean_path("C:\\x\\y", top).is_err());
    assert_eq!(
        clean_path("C:\\code\\r\\a.txt", "C:/code/r"),
        Ok("a.txt".to_owned())
    );
    // On Windows letter case doesn't count; elsewhere it does.
    assert_eq!(
        clean_path("c:\\Code\\R\\a.txt", "C:/code/r"),
        Ok("a.txt".to_owned())
    );
    assert!(clean_path("/home/me/My Repo/a.txt", top).is_err());
    // A name with characters wider than a byte where the top would end is no panic.
    assert!(clean_path("/home/me/my rép/x", top).is_err());
    for refused in ["", "  ", ".", "./", "a\nb", "../outside", "a/../../b"] {
        assert!(clean_path(refused, top).is_err(), "{refused:?}");
    }
    // A name that only starts with dots is a name; one starting with `:` too.
    assert_eq!(clean_path("..hidden", top), Ok("..hidden".to_owned()));
    assert_eq!(clean_path(":x", top), Ok(":x".to_owned()));
}

#[test]
fn drivers_are_checked_against_the_configuration() {
    let attr = |n: &str, v: &str| Attr {
        name: n.into(),
        value: v.into(),
    };
    let named = [
        attr("merge", "ours-driver"),
        attr("diff", "word"),
        attr("filter", "crypt"),
        attr("filter", "lfs"),
    ];
    let refs: Vec<&Attr> = named.iter().collect();
    let config = "merge.ours-driver.name\nours\0diff.word.wordregex\n.\0\
                  filter.crypt.required\ntrue\0filter.lfs.clean\ngit-lfs clean -- %f\0";
    let missing = drivers_missing(&refs, config);
    // The merge driver has a name but no driver command; the diff driver is defined by
    // any key; the required filter has nothing to run; LFS is set up.
    assert_eq!(
        missing,
        [
            MissingDriver {
                attr: "merge".into(),
                name: "ours-driver".into(),
                required: false,
            },
            MissingDriver {
                attr: "filter".into(),
                name: "crypt".into(),
                required: true,
            },
        ]
    );
    assert_eq!(drivers_missing(&refs, "").len(), 4);
}

#[test]
fn folders_listed_whole_hide_their_files() {
    let listed = [
        "e/",
        "e/only.log",
        "a.log",
        "build/",
        "build/x/",
        "buildx.log",
        "a.log",
    ];
    assert_eq!(
        collapse(listed.into_iter()),
        ["a.log", "build/", "buildx.log", "e/"]
    );
    assert!(collapse(std::iter::empty()).is_empty());
}

#[test]
fn meanings_in_words() {
    let attr = |n: &str, v: &str| Attr {
        name: n.into(),
        value: v.into(),
    };
    let words = meaning(&[
        attr("filter", "lfs"),
        attr("merge", "unset"),
        attr("text", "unset"),
    ]);
    assert!(words.iter().any(|w| w.contains("LFS")));
    assert!(words.iter().any(|w| w.starts_with("-merge")));
    assert!(words.iter().any(|w| w.starts_with("-text")));
    // binary sets -diff -merge -text too: said once.
    let binary = meaning(&[
        attr("binary", "set"),
        attr("diff", "unset"),
        attr("merge", "unset"),
        attr("text", "unset"),
    ]);
    assert_eq!(binary.len(), 1, "{binary:?}");
    assert!(meaning(&[attr("diff", "rust")])[0].contains("git's own rust"));
    assert!(meaning(&[]).is_empty());
}
