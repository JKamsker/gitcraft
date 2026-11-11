//! Publishing a stack's pull requests against a fake forge (what it was asked, kept in memory)
//! and a real repository with a real remote to push to.

use std::path::Path;
use std::sync::Mutex;

use super::stack::{Input, LayerPlan, PR_KEY, Step, Target, number_in, plan, publish};
use super::{Forge, Found, Hosting, NewPullRequest, PrState, Provider, PullRequest};
use crate::avatars::Hint;
use crate::git::stack::test_repo::TestRepo;
use crate::git::stack::{config_key, recorded};

/// A forge that keeps its pull requests (and their states) in memory, and can be told to
/// refuse a branch.
#[derive(Default)]
struct FakeForge {
    all: Mutex<Vec<(PullRequest, PrState)>>,
    created: Mutex<Vec<String>>,
    retargeted: Mutex<Vec<(u64, String)>>,
    refuse: Mutex<Option<String>>,
}

impl FakeForge {
    fn open(&self) -> Vec<PullRequest> {
        self.all
            .lock()
            .map(|l| {
                l.iter()
                    .filter(|(_, s)| *s == PrState::Open)
                    .map(|(p, _)| p.clone())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The open pull request of `branch` merged, or closed without merging.
    fn end(&self, branch: &str, state: PrState) {
        if let Ok(mut all) = self.all.lock() {
            for (p, s) in all.iter_mut() {
                if p.branch == branch && *s == PrState::Open {
                    *s = state;
                }
            }
        }
    }

    /// A pull request made elsewhere (the web, a fork), in `state`.
    fn add(&self, number: u64, branch: &str, base: &str, from_fork: bool, state: PrState) {
        if let Ok(mut all) = self.all.lock() {
            all.push((
                PullRequest {
                    number,
                    title: branch.to_owned(),
                    branch: branch.to_owned(),
                    base: base.to_owned(),
                    base_oid: String::new(),
                    from_fork,
                    sha: String::new(),
                    author: "someone".to_owned(),
                    url: format!("https://fake.example/o/r/pull/{number}"),
                    draft: false,
                },
                state,
            ));
        }
    }
}

impl Provider for FakeForge {
    fn name(&self) -> &'static str {
        "Fake"
    }

    fn claims(&self, _: &str) -> bool {
        false
    }

    fn list(&self, _: &Path, _: &Hosting) -> Result<Vec<PullRequest>, String> {
        Ok(self.open())
    }

    fn find(&self, _: &Path, _: &Hosting, branch: &str) -> Result<Option<Found>, String> {
        let all = self.all.lock().map_err(|e| e.to_string())?;
        // Newest first, as the forges answer.
        let found = all
            .iter()
            .rev()
            .filter(|(p, _)| p.branch == branch)
            .map(|(pr, state)| Found {
                pr: pr.clone(),
                state: *state,
            })
            .collect();
        Ok(super::pick_found(found))
    }

    fn create(&self, _: &Path, _: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        if self.refuse.lock().ok().and_then(|r| r.clone()).as_deref() == Some(new.branch) {
            return Err(format!("{} refused", new.branch));
        }
        let mut all = self.all.lock().map_err(|e| e.to_string())?;
        let number = 10 + self.created.lock().map(|c| c.len()).unwrap_or(0) as u64;
        let url = format!("https://fake.example/o/r/pull/{number}");
        all.push((
            PullRequest {
                number,
                title: new.title.to_owned(),
                branch: new.branch.to_owned(),
                base: new.base.to_owned(),
                base_oid: String::new(),
                from_fork: false,
                sha: String::new(),
                author: "me".to_owned(),
                url: url.clone(),
                draft: new.draft,
            },
            PrState::Open,
        ));
        if let Ok(mut c) = self.created.lock() {
            c.push(format!("{}→{}:{}", new.branch, new.base, new.body));
        }
        Ok(format!("Creating pull request…\n{url}\n"))
    }

    fn retarget(&self, _: &Path, _: &Hosting, number: u64, base: &str) -> Result<(), String> {
        let mut all = self.all.lock().map_err(|e| e.to_string())?;
        let (pr, _) = all
            .iter_mut()
            .find(|(p, s)| p.number == number && *s == PrState::Open)
            .ok_or(format!("no open pull request #{number}"))?;
        pr.base = base.to_owned();
        if let Ok(mut r) = self.retargeted.lock() {
            r.push((number, base.to_owned()));
        }
        Ok(())
    }

    fn avatars(&self, _: &Path, _: &Hosting, _: &[String]) -> Vec<(String, Hint)> {
        Vec::new()
    }
}

fn hosting() -> Hosting {
    Hosting {
        forge: Forge::GitHub,
        host: "fake.example".to_owned(),
        path: "o/r".to_owned(),
        web: "https://fake.example".to_owned(),
    }
}

/// `main → schema → api → ui`, with a bare `origin` holding main.
fn stacked(name: &str) -> Result<(TestRepo, std::path::PathBuf), String> {
    let repo = TestRepo::new(name)?;
    let origin = repo.dir.with_file_name(format!(
        "{} origin.git",
        repo.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&origin);
    crate::git::run(None, &["init", "-q", "--bare", &origin.to_string_lossy()])?;
    repo.git(&["remote", "add", "origin", &origin.to_string_lossy()])?;
    repo.git(&["push", "-q", "origin", "main"])?;
    repo.stack(&["schema", "api", "ui"])?;
    Ok((repo, origin))
}

fn inputs(repo: &TestRepo) -> Result<Vec<Input>, String> {
    let rec = recorded(repo.path())?;
    Ok(["schema", "api", "ui"]
        .iter()
        .filter_map(|b| {
            Some(Input {
                branch: (*b).to_owned(),
                parent: rec.get(*b)?.parent.clone(),
            })
        })
        .collect())
}

fn run(
    repo: &TestRepo,
    forge: &FakeForge,
    draft: bool,
) -> Result<(Vec<LayerPlan>, super::stack::Report), String> {
    let hosting = hosting();
    let find = |branch: &str| forge.find(repo.path(), &hosting, branch);
    let plans = plan(repo.path(), &inputs(repo)?, &find)?;
    let report = publish(
        repo.path(),
        &plans,
        &Target {
            provider: forge,
            hosting: &hosting,
            remote: "origin",
            draft,
            creds: None,
        },
    );
    Ok((plans, report))
}

fn bases(forge: &FakeForge) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = forge
        .open()
        .into_iter()
        .map(|p| (p.branch, p.base))
        .collect();
    out.sort();
    out
}

fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect()
}

#[test]
fn a_stack_is_published_bottom_up_each_into_the_one_below() -> Result<(), String> {
    let (repo, origin) = stacked("publish ü")?;
    let forge = FakeForge::default();
    let (plans, report) = run(&repo, &forge, true)?;
    let _ = std::fs::remove_dir_all(&origin);
    assert!(
        plans
            .iter()
            .all(|p| p.step() == Step::Create { replaces: None })
    );
    assert_eq!(plans[1].title, "api");
    assert_eq!(plans[1].commits, ["api"]);
    assert!(
        plans[1].stat.contains("1 file changed"),
        "{}",
        plans[1].stat
    );
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(
        bases(&forge),
        pairs(&[("api", "schema"), ("schema", "main"), ("ui", "api")])
    );
    // Bottom-up, each above saying what it depends on, and drafts as asked.
    let created = forge.created.lock().map(|c| c.clone()).unwrap_or_default();
    assert_eq!(
        created,
        [
            "schema→main:",
            "api→schema:Depends on #10 (schema).",
            "ui→api:Depends on #11 (api)."
        ]
    );
    assert!(forge.open().iter().all(|p| p.draft));
    let n = repo.git(&["config", "--get", &config_key("api", PR_KEY)])?;
    assert_eq!(n.trim(), "11");
    // Pushed, tracking the remote.
    assert_eq!(
        repo.git(&["rev-parse", "--abbrev-ref", "ui@{upstream}"])?
            .trim(),
        "origin/ui"
    );
    Ok(())
}

#[test]
fn a_publish_that_stopped_is_run_again_without_duplicates() -> Result<(), String> {
    let (repo, origin) = stacked("publish retry")?;
    let forge = FakeForge::default();
    if let Ok(mut r) = forge.refuse.lock() {
        *r = Some("api".to_owned());
    }
    let (_, report) = run(&repo, &forge, false)?;
    assert_eq!(report.done.len(), 1);
    assert_eq!(report.failed.as_ref().map(|f| f.0.as_str()), Some("api"));
    if let Ok(mut r) = forge.refuse.lock() {
        *r = None;
    }
    let (plans, report) = run(&repo, &forge, false)?;
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None);
    assert_eq!(plans[0].step(), Step::Push(10));
    assert_eq!(forge.open().len(), 3);
    assert_eq!(forge.created.lock().map(|c| c.len()).unwrap_or(0), 3);
    Ok(())
}

#[test]
fn updating_pushes_restacked_layers_and_fixes_their_bases() -> Result<(), String> {
    let (repo, origin) = stacked("publish update")?;
    let forge = FakeForge::default();
    run(&repo, &forge, false)?;
    // api's request pointed elsewhere by hand, and schema amended with api rebased onto it.
    if let Ok(mut all) = forge.all.lock()
        && let Some((api, _)) = all.iter_mut().find(|(p, _)| p.branch == "api")
    {
        api.base = "main".to_owned();
    }
    repo.checkout("schema")?;
    repo.git(&["commit", "-q", "--amend", "-m", "schema amended"])?;
    repo.git(&["rebase", "-q", "--onto", "schema", "schema@{1}", "api"])?;
    repo.checkout("main")?;
    let (plans, report) = run(&repo, &forge, false)?;
    let pushed_api = repo.git(&["--git-dir", &origin.to_string_lossy(), "rev-parse", "api"]);
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(
        plans[1].step(),
        Step::Retarget {
            number: 11,
            from: "main".into()
        }
    );
    assert_eq!(
        forge
            .retargeted
            .lock()
            .map(|r| r.clone())
            .unwrap_or_default(),
        [(11, "schema".to_owned())]
    );
    assert_eq!(pushed_api?.trim(), repo.tip("api")?);
    assert_eq!(forge.created.lock().map(|c| c.len()).unwrap_or(0), 3);
    Ok(())
}

#[test]
fn when_a_lower_one_merges_the_one_above_is_retargeted() -> Result<(), String> {
    let (repo, origin) = stacked("publish merged")?;
    let forge = FakeForge::default();
    run(&repo, &forge, false)?;
    forge.end("schema", PrState::Merged);
    let (plans, report) = run(&repo, &forge, false)?;
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(plans[0].step(), Step::Merged(10));
    assert_eq!(plans[1].reparent_from.as_deref(), Some("schema"));
    assert_eq!(bases(&forge), pairs(&[("api", "main"), ("ui", "api")]));
    // Recorded too: api is based on main now (its base stays schema's tip, for the restack).
    let rec = recorded(repo.path())?;
    assert_eq!(rec.get("api").map(|r| r.parent.as_str()), Some("main"));
    assert_eq!(
        rec.get("api").and_then(|r| r.base.clone()),
        Some(repo.tip("schema")?)
    );
    // Nothing opened again for schema.
    assert_eq!(forge.created.lock().map(|c| c.len()).unwrap_or(0), 3);
    Ok(())
}

#[test]
fn reads_the_number_a_forge_answers_with() {
    assert_eq!(number_in("https://github.com/o/r/pull/12\n"), Some(12));
    assert_eq!(
        number_in("Creating…\nhttps://gitlab.com/g/r/-/merge_requests/7/"),
        Some(7)
    );
    assert_eq!(number_in("https://codeberg.org/o/r/pulls/3"), Some(3));
    assert_eq!(number_in("done"), None);
    assert_eq!(
        number_in("https://github.com/o/r/pull/12 (draft)"),
        Some(12)
    );
    assert_eq!(
        number_in("https://ghe.example:8443/o/r/pull/5\nwarning: x"),
        Some(5)
    );
    assert_eq!(number_in(""), None);
}

#[test]
fn a_lower_one_closed_without_merging_is_opened_anew_and_the_ones_above_stay() -> Result<(), String>
{
    let (repo, origin) = stacked("publish closed")?;
    let forge = FakeForge::default();
    run(&repo, &forge, false)?;
    forge.end("schema", PrState::Closed);
    let (plans, report) = run(&repo, &forge, false)?;
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(plans[0].step(), Step::Create { replaces: Some(10) });
    assert!(plans[1].reparent_from.is_none());
    assert_eq!(
        bases(&forge),
        pairs(&[("api", "schema"), ("schema", "main"), ("ui", "api")])
    );
    let rec = recorded(repo.path())?;
    assert_eq!(rec.get("api").map(|r| r.parent.as_str()), Some("schema"));
    Ok(())
}

#[test]
fn a_merge_seen_only_on_the_forge_is_taken_in_and_forks_are_ignored() -> Result<(), String> {
    let (repo, origin) = stacked("publish unrecorded")?;
    let forge = FakeForge::default();
    // schema's request was opened and merged on the web; someone's fork proposes a branch
    // called api too.
    forge.add(3, "schema", "main", false, PrState::Merged);
    forge.add(4, "api", "main", true, PrState::Open);
    let (plans, report) = run(&repo, &forge, false)?;
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(plans[0].step(), Step::Merged(3));
    assert_eq!(plans[1].step(), Step::Create { replaces: None });
    assert_eq!(plans[1].base, "main");
    // The fork's request is left as it was; ours is opened beside it.
    let open = forge.open();
    assert!(
        open.iter()
            .any(|p| p.number == 4 && p.from_fork && p.base == "main")
    );
    assert!(
        open.iter()
            .any(|p| !p.from_fork && p.branch == "api" && p.base == "main")
    );
    assert_eq!(forge.created.lock().map(|c| c.len()).unwrap_or(0), 2);
    Ok(())
}

#[test]
fn a_layer_with_nothing_of_its_own_is_pushed_and_opens_nothing() -> Result<(), String> {
    let (repo, origin) = stacked("publish empty")?;
    // ui on api with no commit of its own.
    repo.git(&["branch", "-f", "ui", "api"])?;
    let forge = FakeForge::default();
    let (plans, report) = run(&repo, &forge, false)?;
    let pushed = repo.git(&["--git-dir", &origin.to_string_lossy(), "rev-parse", "ui"]);
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(plans[2].step(), Step::Empty);
    assert_eq!(pushed?.trim(), repo.tip("ui")?);
    assert_eq!(forge.open().len(), 2);
    Ok(())
}

#[test]
fn a_teammates_commits_fetched_but_not_taken_in_are_never_pushed_over() -> Result<(), String> {
    let (repo, origin) = stacked("publish lease")?;
    let forge = FakeForge::default();
    run(&repo, &forge, false)?;
    // A teammate pushes onto api; the background fetch brings it in.
    let mate = repo.dir.with_file_name(format!(
        "{} mate",
        repo.dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&mate);
    let result = (|| -> Result<(), String> {
        crate::git::run(
            None,
            &[
                "clone",
                "-q",
                "-b",
                "api",
                &origin.to_string_lossy(),
                &mate.to_string_lossy(),
            ],
        )?;
        let in_mate = |args: &[&str]| crate::git::run(Some(&mate), args);
        in_mate(&["config", "user.name", "m"])?;
        in_mate(&["config", "user.email", "m@m"])?;
        in_mate(&["commit", "-q", "--allow-empty", "-m", "teammate's"])?;
        in_mate(&["push", "-q", "origin", "api"])?;
        let theirs = in_mate(&["rev-parse", "HEAD"])?.trim().to_owned();
        repo.git(&["fetch", "-q", "origin"])?;
        // Here, api is amended without taking theirs in.
        repo.checkout("api")?;
        repo.git(&["commit", "-q", "--amend", "-m", "api amended"])?;
        repo.checkout("main")?;
        let (_, report) = run(&repo, &forge, false)?;
        assert_eq!(
            report.failed.as_ref().map(|f| f.0.as_str()),
            Some("api"),
            "{report:?}"
        );
        let remote_api = repo.git(&["--git-dir", &origin.to_string_lossy(), "rev-parse", "api"])?;
        assert_eq!(remote_api.trim(), theirs);
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&mate);
    let _ = std::fs::remove_dir_all(&origin);
    result
}
