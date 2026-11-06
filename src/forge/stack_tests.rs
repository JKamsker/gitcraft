//! Publishing a stack's pull requests against a fake forge (what it was asked, kept in memory)
//! and a real repository with a real remote to push to.

use std::path::Path;
use std::sync::Mutex;

use super::stack::{Input, LayerPlan, PR_KEY, Step, Target, number_in, plan, publish};
use super::{Forge, Hosting, NewPullRequest, Provider, PullRequest};
use crate::avatars::Hint;
use crate::git::stack::test_repo::TestRepo;
use crate::git::stack::{config_key, recorded};

/// A forge that keeps its pull requests in memory, and can be told to refuse a branch.
#[derive(Default)]
struct FakeForge {
    open: Mutex<Vec<PullRequest>>,
    created: Mutex<Vec<String>>,
    retargeted: Mutex<Vec<(u64, String)>>,
    refuse: Mutex<Option<String>>,
}

impl FakeForge {
    fn open(&self) -> Vec<PullRequest> {
        self.open.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// The pull request of `branch` merged: no longer open.
    fn merge(&self, branch: &str) {
        if let Ok(mut open) = self.open.lock() {
            open.retain(|p| p.branch != branch);
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

    fn create(&self, _: &Path, _: &Hosting, new: &NewPullRequest) -> Result<String, String> {
        if self.refuse.lock().ok().and_then(|r| r.clone()).as_deref() == Some(new.branch) {
            return Err(format!("{} refused", new.branch));
        }
        let mut open = self.open.lock().map_err(|e| e.to_string())?;
        let number = 10 + self.created.lock().map(|c| c.len()).unwrap_or(0) as u64;
        let url = format!("https://fake.example/o/r/pull/{number}");
        open.push(PullRequest {
            number,
            title: new.title.to_owned(),
            branch: new.branch.to_owned(),
            base: new.base.to_owned(),
            sha: String::new(),
            author: "me".to_owned(),
            url: url.clone(),
            draft: new.draft,
        });
        if let Ok(mut c) = self.created.lock() {
            c.push(format!("{}→{}:{}", new.branch, new.base, new.body));
        }
        Ok(format!("Creating pull request…\n{url}\n"))
    }

    fn retarget(&self, _: &Path, _: &Hosting, number: u64, base: &str) -> Result<(), String> {
        let mut open = self.open.lock().map_err(|e| e.to_string())?;
        let pr = open
            .iter_mut()
            .find(|p| p.number == number)
            .ok_or(format!("no pull request #{number}"))?;
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
    let plans = plan(repo.path(), &inputs(repo)?, &forge.open())?;
    let hosting = hosting();
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
    assert!(plans.iter().all(|p| p.step() == Step::Create));
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
    assert_eq!(plans[0].step(), Step::Push);
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
    if let Ok(mut open) = forge.open.lock()
        && let Some(api) = open.iter_mut().find(|p| p.branch == "api")
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
    forge.merge("schema");
    let (plans, report) = run(&repo, &forge, false)?;
    let _ = std::fs::remove_dir_all(&origin);
    assert_eq!(report.failed, None, "{report:?}");
    assert_eq!(plans[0].step(), Step::Skip(10));
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
    assert_eq!(number_in(""), None);
}
