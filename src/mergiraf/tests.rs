use super::*;

/// A conflicted file in WSL is solved by Mergiraf here, from a copy (run with
/// `GITCRAFT_TEST_MERGIRAF=<mergiraf program>` and `GITCRAFT_TEST_WSL=<distro>`; skipped
/// otherwise).
#[test]
fn a_file_elsewhere_is_solved_from_a_copy() -> Result<(), String> {
    let (Ok(program), Ok(distro)) = (
        std::env::var("GITCRAFT_TEST_MERGIRAF"),
        std::env::var("GITCRAFT_TEST_WSL"),
    ) else {
        return Ok(());
    };
    if !cfg!(windows) {
        return Ok(());
    }
    let host = crate::remote::Host::Wsl(distro);
    let dir = format!("/tmp/gitcraft-mergiraf-{}", std::process::id());
    let conflicted = concat!(
        "fn main() {}\n",
        "<<<<<<< ours\n",
        "fn a() {}\n",
        "||||||| base\n",
        "=======\n",
        "fn b() {}\n",
        ">>>>>>> theirs\n",
    );
    let made = crate::remote::exec(
        &host,
        &crate::remote::Request {
            script: format!("mkdir -p {dir} && cat > {dir}/both.rs"),
            stdin: Some(conflicted.as_bytes().to_vec()),
            ..crate::remote::Request::default()
        },
    )?;
    assert_eq!(made.code, 0);
    let repo = crate::remote::encode(&host, &dir);
    let solved = solve_copy(Path::new(&program), &repo, "both.rs");
    let _ = crate::remote::exec(
        &host,
        &crate::remote::Request {
            script: format!("rm -rf {dir}"),
            ..crate::remote::Request::default()
        },
    );
    let solved = solved?;
    assert!(!solved.contains("<<<<<<<"), "{solved}");
    assert!(
        solved.contains("fn a()") && solved.contains("fn b()"),
        "{solved}"
    );
    Ok(())
}

#[test]
fn language_patterns_are_read_and_matched() {
    let out = "*.java merge=mergiraf\n*.rs merge=mergiraf\n*.js *.jsx merge=mergiraf\n\
                   go.mod merge=mergiraf\n# a comment\n";
    let patterns = parse_patterns(out);
    assert_eq!(patterns, ["*.java", "*.rs", "*.js", "*.jsx", "go.mod"]);
    for (path, known) in [
        ("src/main.rs", true),
        ("a b/ü.jsx", true),
        ("go.mod", true),
        ("sub/go.mod", true),
        ("go.mod.bak", false),
        ("main.rs.orig", false),
        ("rs", false),
        ("image.png", false),
        ("Makefile", false),
    ] {
        assert_eq!(knows(&patterns, path), known, "{path}");
    }
    assert!(wildcard_match("a*b*c", "a-x-b-y-c"));
    assert!(!wildcard_match("a*b*c", "a-x-c"));
    assert!(wildcard_match("*", "anything"));
    assert!(!wildcard_match("*.rs", ".r"));
}

#[test]
fn errors_lose_their_log_level() {
    assert_eq!(
        error_message(
            "WARN Mergiraf: Could not find a supported language for 'notes.txt'\n",
            "notes.txt"
        ),
        "Mergiraf: Could not find a supported language for 'notes.txt'"
    );
    assert_eq!(error_message("ERROR boom", "f"), "Mergiraf: boom");
    assert_eq!(error_message("  \n", "f"), "Mergiraf could not solve f");
}
