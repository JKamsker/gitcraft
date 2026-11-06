//! The issues assigned to you, against real servers: a Forgejo and a Gitea started by
//! `ci/forge-server.sh`, as for [`super::live`] (which says how to run these). Ignored by a
//! plain `cargo test`. Each run makes a new repository on the server.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;

use super::{Api, FORGEJO, GITEA, GiteaFamily, Login, issues};
use crate::forge::Hosting;

macro_rules! ensure {
    ($cond:expr, $($why:tt)+) => {
        if !$cond {
            return Err(format!($($why)+));
        }
    };
}

#[test]
#[ignore = "needs a Forgejo: see ci/forge-server.sh"]
fn live_issues_forgejo() -> Result<(), String> {
    assigned_to_each(&FORGEJO)
}

#[test]
#[ignore = "needs a Gitea: see ci/forge-server.sh"]
fn live_issues_gitea() -> Result<(), String> {
    assigned_to_each(&GITEA)
}

/// An issue assigned to `author` and one to `reviewer`: each sees their own in their list, and
/// not the other's; one read again by its number is the same.
fn assigned_to_each(family: &'static GiteaFamily) -> Result<(), String> {
    let upper = family.name.to_uppercase();
    let var = |name: &str| {
        let key = format!("GITCRAFT_{upper}_{name}");
        std::env::var(&key).map_err(|_| {
            format!("{key} isn't set: start a server with ci/forge-server.sh (see its header)")
        })
    };
    let url = var("URL")?.trim_end_matches('/').to_owned();
    let api = |token: String| Api::new(family.name, &url, "127.0.0.1", Some(Login::Token(token)));
    let (author, reviewer) = (api(var("AUTHOR_TOKEN")?), api(var("REVIEWER_TOKEN")?));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let name = format!("gitcraft-issues-{stamp}");
    let path = format!("author/{name}");
    let hosting = Hosting {
        forge: family.forge,
        host: "127.0.0.1".to_owned(),
        path: path.clone(),
        web: url.clone(),
    };

    eprintln!(
        "{}: A repository, with the reviewer a collaborator",
        family.name
    );
    author.send(
        "POST",
        "user/repos",
        Some(&json!({ "name": name, "default_branch": "main", "auto_init": true })),
    )?;
    author.send(
        "PUT",
        &format!("repos/{path}/collaborators/reviewer"),
        Some(&json!({ "permission": "write" })),
    )?;

    eprintln!(
        "{}: An issue for each of them, and one for nobody",
        family.name
    );
    let open = |title: &str, body: &str, assignees: &[&str]| -> Result<u64, String> {
        let made = author.send(
            "POST",
            &format!("repos/{path}/issues"),
            Some(&json!({ "title": title, "body": body, "assignees": assignees })),
        )?;
        made.get("number")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| format!("{title}: no number in {made}"))
    };
    let mine = open(
        "Fix login crash",
        "## Acceptance criteria\n- [ ] no crash",
        &["author"],
    )?;
    let theirs = open("Review the docs", "", &["reviewer"])?;
    let nobodys = open("Someday", "", &[])?;

    eprintln!("{}: Each one's list", family.name);
    let numbers = |api: &Api| -> Result<Vec<u64>, String> {
        let mut found: Vec<u64> = issues::assigned(api, &hosting)?
            .iter()
            .map(|i| i.number)
            .collect();
        found.sort_unstable();
        Ok(found)
    };
    let (for_author, for_reviewer) = (numbers(&author)?, numbers(&reviewer)?);
    ensure!(
        for_author == [mine],
        "author's: {for_author:?}, not [{mine}]"
    );
    ensure!(
        for_reviewer == [theirs],
        "reviewer's: {for_reviewer:?}, not [{theirs}] (nobody's is {nobodys})"
    );
    let listed = issues::assigned(&author, &hosting)?;
    let issue = listed.first().ok_or("the author's list is empty")?;
    ensure!(
        issue.title == "Fix login crash"
            && issue.body.contains("- [ ] no crash")
            && issue.url.ends_with(&format!("/{path}/issues/{mine}")),
        "listed {issue:?}"
    );

    eprintln!("{}: One read again by its number", family.name);
    let again = issues::one(&reviewer, &hosting, mine)?;
    ensure!(&again == issue, "read again {again:?}, listed {issue:?}");
    Ok(())
}
