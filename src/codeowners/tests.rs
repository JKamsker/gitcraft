//! Each forge's CODEOWNERS read its own way, and found in a real repository.

use std::path::PathBuf;

use super::*;

fn owners(flavor: Flavor, text: &str, path: &str) -> Vec<String> {
    CodeOwners::parse("CODEOWNERS", flavor, text).owners(path)
}

const GITHUB: &str = "\
# Everything, unless a later line says otherwise.
*       @acme/everyone
*.js    @ann
/build/logs/ @acme/infra
docs/*  docs@example.org
/apps/  @acme/apps
/apps/github
/my\\ dir/ @bob
";

#[test]
fn github_the_last_matching_line_wins() {
    assert_eq!(
        owners(Flavor::GitHub, GITHUB, "README.md"),
        ["@acme/everyone"]
    );
    assert_eq!(owners(Flavor::GitHub, GITHUB, "src/app.js"), ["@ann"]);
    assert_eq!(
        owners(Flavor::GitHub, GITHUB, "build/logs/x.log"),
        ["@acme/infra"]
    );
    assert_eq!(
        owners(Flavor::GitHub, GITHUB, "docs/a.md"),
        ["docs@example.org"]
    );
    assert_eq!(
        owners(Flavor::GitHub, GITHUB, "docs/deep/a.md"),
        ["@acme/everyone"]
    );
    assert_eq!(
        owners(Flavor::GitHub, GITHUB, "apps/web/x.ts"),
        ["@acme/apps"]
    );
    // A line without owners: nobody owns these.
    assert!(owners(Flavor::GitHub, GITHUB, "apps/github/x.rb").is_empty());
    assert_eq!(owners(Flavor::GitHub, GITHUB, "my dir/ü.txt"), ["@bob"]);
}

#[test]
fn github_skips_negations_and_says_so() {
    let file = CodeOwners::parse("CODEOWNERS", Flavor::GitHub, "* @a\n!*.md @b\n");
    assert_eq!(file.owners("x.md"), ["@a"]);
    assert_eq!(file.problems.len(), 1);
    assert_eq!(file.problems[0].line, 2);
}

const GITLAB: &str = "\
* @default-owner
[Docs] @acme/writers
*.md
/docs/internal/ @ann
!/docs/internal/public.md
^[Database][2] @dba
db/
[Backend]
*.rb @bob @ann
";

#[test]
fn gitlab_sections_add_up_and_use_their_default_owners() {
    assert_eq!(
        owners(Flavor::GitLab, GITLAB, "README.md"),
        ["@default-owner", "@acme/writers"]
    );
    assert_eq!(
        owners(Flavor::GitLab, GITLAB, "docs/internal/a.md"),
        ["@default-owner", "@ann"]
    );
    // Taken out of the Docs section: only the default owner is left.
    assert_eq!(
        owners(Flavor::GitLab, GITLAB, "docs/internal/public.md"),
        ["@default-owner"]
    );
    assert_eq!(
        owners(Flavor::GitLab, GITLAB, "db/schema.rb"),
        ["@default-owner", "@dba", "@bob", "@ann"]
    );
}

#[test]
fn a_file_with_sections_found_where_github_looks_is_read_as_gitlabs() {
    let file = CodeOwners::parse("CODEOWNERS", Flavor::GitHub, GITLAB);
    assert_eq!(file.flavor, Flavor::GitLab);
    assert_eq!(
        file.section_labels(),
        ["Docs", "Database (optional, 2 approvals)", "Backend"]
    );
}

#[test]
fn gitlab_lines_without_owners_in_a_section_without_defaults_are_noted() {
    let file = CodeOwners::parse("CODEOWNERS", Flavor::GitLab, "[Ops]\n/ops/\n");
    assert!(file.owners("ops/a").is_empty());
    assert_eq!(file.problems.len(), 1);
}

const GITEA: &str = "\
.*\\.go @gopher
!.*\\.go @acme/not-go
docs/.* @writer @gopher
[ @broken
";

#[test]
fn gitea_every_matching_line_adds_its_owners_and_negations_stand_for_the_rest() {
    assert_eq!(owners(Flavor::Gitea, GITEA, "main.go"), ["@gopher"]);
    assert_eq!(owners(Flavor::Gitea, GITEA, "cmd/x/main.go"), ["@gopher"]);
    assert_eq!(owners(Flavor::Gitea, GITEA, "README.md"), ["@acme/not-go"]);
    assert_eq!(
        owners(Flavor::Gitea, GITEA, "docs/a.md"),
        ["@acme/not-go", "@writer", "@gopher"]
    );
    // The whole path, not a part of it.
    assert_eq!(owners(Flavor::Gitea, GITEA, "main.gox"), ["@acme/not-go"]);
    let file = CodeOwners::parse("CODEOWNERS", Flavor::Gitea, GITEA);
    assert_eq!(file.problems.len(), 1, "{:?}", file.problems);
    assert_eq!(file.problems[0].line, 4);
}

#[test]
fn lint_finds_patterns_matching_nothing_and_files_without_owners() {
    let file = CodeOwners::parse(
        "CODEOWNERS",
        Flavor::GitHub,
        "/src/ @dev\n/gone/ @old\n*.rs @rust\n",
    );
    let files: Vec<String> = ["src/main.rs", "README.md", "build.rs"]
        .map(String::from)
        .to_vec();
    let lint = file.lint(&files);
    assert_eq!(lint.unmatched, [(2, "/gone/".to_owned())]);
    assert_eq!(lint.unowned, ["README.md"]);
    assert_eq!(lint.files, 3);
    let empty = file.lint(&[]);
    assert_eq!(empty.unmatched.len(), 3);
    assert!(empty.unowned.is_empty());
}

#[test]
fn owners_of_many_files_and_reviewers_to_ask() {
    let file = CodeOwners::parse(
        "CODEOWNERS",
        Flavor::GitHub,
        "/pay/ @acme/payments\n/infra/ @acme/infra me@example.org\n/x/ @@maintainer\n",
    );
    let (all, unowned) = owners_of(&file, ["pay/a", "infra/b", "pay/c", "other"]);
    assert_eq!(all, ["@acme/payments", "@acme/infra", "me@example.org"]);
    assert_eq!(unowned, 1);
    let names: Vec<String> = all.iter().map(|o| short_owner(o).to_owned()).collect();
    assert_eq!(and_list(&names), "payments, infra and me@example.org");
    let reviewers = reviewers_for(&file, ["pay/a", "infra/b", "x/y"]);
    assert_eq!(
        reviewers,
        [
            Reviewer {
                name: "acme/payments".into(),
                team: true
            },
            Reviewer {
                name: "acme/infra".into(),
                team: true
            },
        ]
    );
    assert_eq!(short_owner("@ann"), "ann");
    assert_eq!(short_owner("@@maintainer"), "@@maintainer");
    assert_eq!(Reviewer::of("@ann").map(|r| r.team), Some(false));
}

#[test]
fn a_huge_file_list_is_read_in_reasonable_time() {
    let mut text = String::new();
    for i in 0..300 {
        text.push_str(&format!("/area{i}/**/*.rs @team{i}\n"));
    }
    let file = CodeOwners::parse("CODEOWNERS", Flavor::GitHub, &text);
    let files: Vec<String> = (0..3000)
        .map(|i| format!("area{}/deep/er/file{i}.rs", i % 400))
        .collect();
    let started = std::time::Instant::now();
    let lint = file.lint(&files);
    assert!(started.elapsed() < std::time::Duration::from_secs(60));
    assert_eq!(lint.unowned.len(), 700);
}

fn repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gitcraft owners-ü {name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::create_dir_all(&dir);
    for args in [
        &["init", "-q", "-b", "main"][..],
        &["config", "user.name", "t"],
        &["config", "user.email", "t@t"],
    ] {
        assert!(git::run(Some(&dir), args).is_ok());
    }
    dir
}

fn commit_file(dir: &std::path::Path, path: &str, text: &str) {
    let file = dir.join(path);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&file, text);
    assert!(git::run(Some(dir), &["add", "--", path]).is_ok());
    assert!(git::run(Some(dir), &["commit", "-q", "-m", path]).is_ok());
}

#[test]
fn each_forge_finds_the_file_in_its_own_places_in_a_real_repository() {
    let dir = repo("find");
    let before = read(&dir, "HEAD", Some(Forge::GitHub), None);
    commit_file(&dir, "docs/CODEOWNERS", "* @docs-owner\n");
    commit_file(&dir, ".gitlab/CODEOWNERS", "* @gitlab-owner\n");
    let github = read(&dir, "HEAD", Some(Forge::GitHub), None);
    let gitlab = read(&dir, "HEAD", Some(Forge::GitLab), None);
    commit_file(&dir, ".github/CODEOWNERS", "* @github-owner\n");
    let github_first = read(&dir, "HEAD", Some(Forge::GitHub), None);
    commit_file(&dir, ".forgejo/CODEOWNERS", ".* @forgejo-owner\n");
    let forgejo = read(&dir, "HEAD", Some(Forge::Forgejo), None);
    let gitea = read(&dir, "HEAD", Some(Forge::Gitea), None);
    let unknown = read(&dir, "HEAD", None, None);
    // The same blob again: kept, not read again.
    let kept = read(
        &dir,
        "HEAD",
        Some(Forge::GitHub),
        github_first.as_ref().ok().and_then(|o| o.as_ref()),
    );
    let _ = std::fs::remove_dir_all(&dir);
    let found = |r: &Result<Option<CodeOwners>, String>| {
        r.as_ref()
            .ok()
            .and_then(|o| o.as_ref())
            .map(|o| (o.path.clone(), o.owners("x")))
    };
    assert!(matches!(before, Ok(None)), "no commits yet");
    assert_eq!(
        found(&github),
        Some(("docs/CODEOWNERS".into(), vec!["@docs-owner".into()]))
    );
    assert_eq!(
        found(&gitlab),
        Some(("docs/CODEOWNERS".into(), vec!["@docs-owner".into()]))
    );
    assert_eq!(
        found(&github_first).map(|f| f.0),
        Some(".github/CODEOWNERS".into())
    );
    assert_eq!(
        found(&forgejo),
        Some((".forgejo/CODEOWNERS".into(), vec!["@forgejo-owner".into()]))
    );
    assert_eq!(found(&gitea).map(|f| f.0), Some("docs/CODEOWNERS".into()));
    assert_eq!(
        found(&unknown).map(|f| f.0),
        Some(".github/CODEOWNERS".into())
    );
    assert_eq!(found(&kept).map(|f| f.0), Some(".github/CODEOWNERS".into()));
}
