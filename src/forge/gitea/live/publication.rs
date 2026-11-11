//! Fork checkout, stack publication and resumable release/asset scenarios.
use super::*;

/// The reviewer forks the repository and proposes a branch of the fork, letting maintainers
/// edit it: the list says so, and the author checks it out as gitcraft does and pushes to it.
pub(super) fn pull_request_from_a_fork(
    server: &Server,
    family: &'static GiteaFamily,
    dir: &Path,
    hosting: &Hosting,
    name: &str,
) -> Result<(), String> {
    eprintln!("{}: A pull request from a fork", family.name);
    let fork_name = format!("{name}-fork");
    server.reviewer().send(
        "POST",
        &format!("repos/{}/forks", hosting.path),
        Some(&json!({ "name": fork_name })),
    )?;
    let fork_path = format!("reviewer/{fork_name}");
    let as_reviewer =
        server
            .url
            .replacen("://", &format!("://reviewer:{}@", server.reviewer_token), 1)
            + &format!("/{fork_path}.git");
    run(dir, &["checkout", "-q", "-b", "patch", "main"])?;
    write(dir, "d.txt", "from the fork\n")?;
    run(dir, &["add", "."])?;
    run(dir, &["commit", "-q", "-m", "fix: from the fork"])?;
    eventually("the fork to be made", || {
        Ok(run(dir, &["push", "-q", &as_reviewer, "patch"])
            .ok()
            .map(|_| ()))
    })?;
    run(dir, &["checkout", "-q", "main"])?;
    run(dir, &["branch", "-D", "patch"])?;
    let opened = server.reviewer().send(
        "POST",
        &format!("repos/{}/pulls", hosting.path),
        Some(&json!({ "head": "reviewer:patch", "base": "main", "title": "From a fork" })),
    )?;
    let number = opened
        .get("number")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    server.reviewer().send(
        "PATCH",
        &format!("repos/{}/pulls/{number}", hosting.path),
        Some(&json!({ "allow_maintainer_edit": true })),
    )?;
    let forks = family.forks(dir, hosting)?;
    let info = forks
        .iter()
        .find(|f| f.number == number)
        .ok_or("the fork's pull request isn't listed")?;
    ensure!(
        info.cross
            && info.maintainers_can_edit
            && info.owner == "reviewer"
            && info.branch == "patch"
            && info.base == "main"
            && info.clone_https.ends_with(&format!("{fork_path}.git")),
        "{info:?}"
    );
    ensure!(
        forks.iter().any(|f| f.number != number && !f.cross),
        "the repository's own branch reads as a fork: {forks:?}"
    );
    // The owned fixture serves HTTP on loopback. Use a repository-local Git
    // credential helper for this exact fork URL: transient application sign-in
    // deliberately requires HTTPS. This still exercises the real checkout plan,
    // expectation checks and push through the ordinary credential-manager path.
    let helper = format!(
        "!f() {{ test \"$1\" = get || return 0; printf '%s\\n' 'username=author' {}; }}; f",
        crate::remote::sh_quote(&format!("password={}", server.author_token))
    );
    run(dir, &["config", "credential.helper", ""])?;
    run(dir, &["config", "credential.useHttpPath", "true"])?;
    // Neither the test wrapper nor Git's fallback command diagnostic may echo
    // the helper argument: it contains the fixture's author token.
    git::run(
        Some(dir),
        &[
            "config",
            &format!("credential.{}.helper", info.clone_https),
            &helper,
        ],
    )
    .map_err(|_| "Cannot install the loopback fixture credential helper".to_owned())?;
    let setup = crate::repo::forks::plan::read_setup(dir)?;
    let locals = vec!["main".to_owned(), "feat".to_owned(), "wip".to_owned()];
    let plan = crate::repo::forks::plan::checkout(
        info,
        &git::remotes(dir)?,
        &setup,
        &locals,
        "main",
        Some(hosting),
    )?;
    crate::repo::operations::run_live_checkout(dir, &plan.steps, None)?;
    write(dir, "d.txt", "fixed by the maintainer\n")?;
    run(dir, &["commit", "-q", "-am", "fix: by the maintainer"])?;
    git::run_with(Some(dir), &["push", "-q"], None)?;
    let mine = run(dir, &["rev-parse", "HEAD"])?.trim().to_owned();
    let branch = server
        .author()
        .get(&format!("repos/{fork_path}/branches/patch"))?;
    ensure!(
        at(&branch, "/commit/id") == mine,
        "the fork's branch is at {:?}, not {mine}",
        at(&branch, "/commit/id")
    );
    run(dir, &["checkout", "-q", "main"])?;
    Ok(())
}

/// The provider, logged in as the author: the forge's own calls, with the server's token
/// (a test can't hand the real provider a login without changing the process's environment).
struct AsAuthor<'a>(&'a Server);

impl Provider for AsAuthor<'_> {
    fn name(&self) -> &'static str {
        "live"
    }

    fn claims(&self, _: &str) -> bool {
        false
    }

    fn list(&self, _: &Path, hosting: &Hosting) -> Result<Vec<forge::PullRequest>, String> {
        list(&self.0.author(), hosting)
    }

    fn create(&self, _: &Path, hosting: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        create(&self.0.author(), hosting, new)
    }

    fn retarget(&self, _: &Path, hosting: &Hosting, number: u64, base: &str) -> Result<(), String> {
        retarget(&self.0.author(), hosting, number, base)
    }

    fn find(&self, _: &Path, hosting: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        super::super::find(&self.0.author(), hosting, branch)
    }

    fn avatars(&self, _: &Path, _: &Hosting, _: &[String]) -> Vec<(String, crate::avatars::Hint)> {
        Vec::new()
    }
}

/// A stack's pull requests on the server: published bottom-up, published again without
/// duplicates, and the upper one retargeted once the lower one is closed.
pub(super) fn stacked(
    server: &Server,
    hosting: &Hosting,
    dir: &Path,
    name: &str,
) -> Result<(), String> {
    use crate::forge::stack::{Input, Target, plan, publish};
    eprintln!("{name}: A stack of pull requests");
    run(dir, &["checkout", "-q", "-b", "stack-low", "main"])?;
    write(dir, "low.txt", "low\n")?;
    run(dir, &["add", "."])?;
    run(dir, &["commit", "-q", "-m", "low layer"])?;
    run(dir, &["checkout", "-q", "-b", "stack-high"])?;
    write(dir, "high.txt", "high\n")?;
    run(dir, &["add", "."])?;
    run(dir, &["commit", "-q", "-m", "high layer"])?;
    run(dir, &["checkout", "-q", "main"])?;
    let main = run(dir, &["rev-parse", "main"])?.trim().to_owned();
    let low = run(dir, &["rev-parse", "stack-low"])?.trim().to_owned();
    for step in [
        git::stack::set_parent_ops("stack-low", "main", &main),
        git::stack::set_parent_ops("stack-high", "stack-low", &low),
    ]
    .concat()
    {
        let args: Vec<&str> = step.iter().map(String::as_str).collect();
        run(dir, &args)?;
    }
    let inputs = [
        Input {
            branch: "stack-low".into(),
            parent: "main".into(),
        },
        Input {
            branch: "stack-high".into(),
            parent: "stack-low".into(),
        },
    ];
    let provider = AsAuthor(server);
    let publish_now = || -> Result<(), String> {
        let find = |branch: &str| provider.find(dir, hosting, branch);
        let plans = plan(dir, &inputs, &find)?;
        let report = publish(
            dir,
            &plans,
            &Target {
                provider: &provider,
                hosting,
                remote: "origin",
                draft: false,
                creds: None,
            },
        );
        match report.failed {
            Some((branch, e)) => Err(format!("publishing {branch}: {e}")),
            None => Ok(()),
        }
    };
    let bases = || -> Result<Vec<(String, String)>, String> {
        let mut out: Vec<(String, String)> = list(&server.author(), hosting)?
            .into_iter()
            .filter(|p| p.branch.starts_with("stack-"))
            .map(|p| (p.branch, p.base))
            .collect();
        out.sort();
        Ok(out)
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    publish_now()?;
    let first = bases()?;
    ensure!(
        first == [pair("stack-high", "stack-low"), pair("stack-low", "main")],
        "published as {first:?}"
    );
    // Again: nothing opened twice.
    publish_now()?;
    let again = bases()?;
    ensure!(again == first, "published again as {again:?}");
    // The lower one merged: the upper one goes into main.
    let low_pr = list(&server.author(), hosting)?
        .into_iter()
        .find(|p| p.branch == "stack-low")
        .ok_or("stack-low has no pull request")?;
    let merge = format!("repos/{}/pulls/{}/merge", hosting.path, low_pr.number);
    // The forge works out whether it can be merged in the background first.
    eventually("the lower pull request to merge", || {
        match server
            .author()
            .send("POST", &merge, Some(&json!({ "Do": "merge" })))
        {
            Ok(_) => Ok(Some(())),
            Err(f) if f.status == Some(405) => Ok(None),
            Err(f) => Err(String::from(f)),
        }
    })?;
    publish_now()?;
    let after = bases()?;
    ensure!(
        after == [pair("stack-high", "main")],
        "after the merge: {after:?}"
    );
    let parent = run(dir, &["config", "branch.stack-high.gitcraft-parent"])?;
    ensure!(parent.trim() == "main", "stack-high is based on {parent}");
    // Published again: the merged one isn't opened again.
    publish_now()?;
    ensure!(bases()? == after, "a merged layer was opened again");
    Ok(())
}

/// A tag pushed, its release made with notes, and a file attached to it.
pub(super) fn release_with_a_file(
    server: &Server,
    family: &'static GiteaFamily,
    dir: &Path,
    hosting: &Hosting,
) -> Result<(), String> {
    eprintln!("{}: A release with a file", family.name);
    run(dir, &["tag", "-a", "v1.0.0", "-m", "v1.0.0", "main"])?;
    run(dir, &["push", "-q", "origin", "v1.0.0"])?;
    let url = create_release(
        &server.author(),
        hosting,
        &forge::NewRelease {
            tag: "v1.0.0",
            title: "v1.0.0",
            notes: "### Features\n\n- ü things",
            prerelease: true,
        },
    )?;
    ensure!(url.contains("/releases/tag/v1.0.0"), "release at {url}");
    let file = dir.join("dist ü.txt");
    std::fs::write(&file, "attached\n").map_err(|e| e.to_string())?;
    upload_assets(
        &server.author(),
        hosting,
        "v1.0.0",
        std::slice::from_ref(&file),
    )?;
    // Resumed: the same release, the file not attached twice.
    let again = create_release(
        &server.author(),
        hosting,
        &forge::NewRelease {
            tag: "v1.0.0",
            title: "v1.0.0",
            notes: "other",
            prerelease: true,
        },
    )?;
    ensure!(again == url, "made again at {again}");
    upload_assets(
        &server.author(),
        hosting,
        "v1.0.0",
        std::slice::from_ref(&file),
    )?;
    let release = server
        .author()
        .get(&format!("repos/{}/releases/tags/v1.0.0", hosting.path))?;
    ensure!(
        release
            .get("assets")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
            == Some(1),
        "attached twice: {release}"
    );
    ensure!(
        at(&release, "/body").contains("ü things")
            && release
                .get("prerelease")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
            && at(&release, "/assets/0/name") == "dist ü.txt",
        "{release}"
    );
    Ok(())
}
