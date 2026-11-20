//! A post's follow-up read settles the draft that sent it (#195), whichever pull request is
//! shown by the time it answers, and whether it answers, fails or is refused. Only a post
//! still being sent keeps the primary forge from being changed.

use super::Posted;
use crate::pull_request::{PrDetails, PrFiles};
use crate::repo::messages::{record_jobs, record_rejected_jobs};
use crate::repo::{Msg, RepoTab};

fn tab(ctx: &egui::Context) -> RepoTab {
    RepoTab::new_deferred("/nonexistent/pr-settling".into(), ctx)
}

fn head() -> String {
    "b".repeat(40)
}

fn details() -> PrDetails {
    PrDetails {
        number: 7,
        state: "OPEN".into(),
        head_oid: head(),
        ..Default::default()
    }
}

fn files() -> PrFiles {
    PrFiles {
        from: "a".repeat(40),
        to: head(),
        files: Vec::new(),
    }
}

type Job = Box<dyn FnOnce() -> Msg + Send>;

/// The one read `jobs` hold, run: with no forge selected it answers at once, off the network.
fn read_request(jobs: Vec<Job>) -> Result<u64, String> {
    let [job] = <[Job; 1]>::try_from(jobs).map_err(|j| format!("{} jobs", j.len()))?;
    match job() {
        Msg::PrDetails(request, _) | Msg::PrFiles(request, _) => Ok(request),
        _ => Err("not a pull request read".into()),
    }
}

/// Opens pull request 7 and has a comment on it sent; the page is then read again. Returns
/// that read's request.
fn posted(tab: &mut RepoTab, ctx: &egui::Context) -> Result<u64, String> {
    record_jobs(|| tab.open_pr(7, ctx));
    tab.pr_drafts.entry(7).or_default().posting = true;
    let (_, jobs) = record_jobs(|| tab.pr_posted(7, Posted::Comment, Ok(()), ctx));
    assert!(tab.pr_drafts[&7].posting, "the page is read again first");
    read_request(jobs)
}

fn settled(tab: &RepoTab) -> bool {
    !tab.pr_drafts[&7].posting && !tab.forge_change_blocked()
}

#[test]
fn a_follow_up_read_answered_after_the_page_changed_settles_its_draft() -> Result<(), String> {
    for leave in ["closed", "other"] {
        for ok in [true, false] {
            let ctx = egui::Context::default();
            let mut tab = tab(&ctx);
            let request = posted(&mut tab, &ctx)?;
            match leave {
                "closed" => tab.close_pr(),
                _ => drop(record_jobs(|| tab.open_pr(8, &ctx))),
            }
            let answer = if ok {
                Ok(details())
            } else {
                Err("gone".into())
            };
            let (_, jobs) =
                record_jobs(|| tab.handle(Msg::PrDetails(request, Box::new(answer)), &ctx));
            assert!(jobs.is_empty(), "no files are read for a page not shown");
            assert!(settled(&tab), "{leave} ok={ok}");
        }
    }
    Ok(())
}

#[test]
fn files_answered_after_the_page_closed_settle_their_draft() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let request = posted(&mut tab, &ctx)?;
    let (_, jobs) =
        record_jobs(|| tab.handle(Msg::PrDetails(request, Box::new(Ok(details()))), &ctx));
    assert_eq!(read_request(jobs)?, request, "the files follow the details");
    assert!(
        tab.pr_drafts[&7].posting,
        "settled only once the files come"
    );
    tab.close_pr();
    tab.handle(Msg::PrFiles(request, Ok(files())), &ctx);
    assert!(settled(&tab));
    Ok(())
}

#[test]
fn a_refused_follow_up_read_settles_its_draft() -> Result<(), String> {
    for files_refused in [false, true] {
        let ctx = egui::Context::default();
        let mut tab = tab(&ctx);
        let jobs = if files_refused {
            let request = posted(&mut tab, &ctx)?;
            let msg = Msg::PrDetails(request, Box::new(Ok(details())));
            record_rejected_jobs(|| tab.handle(msg, &ctx)).1
        } else {
            record_jobs(|| tab.open_pr(7, &ctx));
            tab.pr_drafts.entry(7).or_default().posting = true;
            record_rejected_jobs(|| tab.pr_posted(7, Posted::Comment, Ok(()), &ctx)).1
        };
        assert_eq!(jobs.len(), 1);
        for job in jobs {
            let msg = job();
            record_jobs(|| tab.handle(msg, &ctx));
        }
        assert!(settled(&tab), "files_refused={files_refused}");
    }
    Ok(())
}

#[test]
fn only_a_post_still_being_sent_blocks_changing_the_primary_forge() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    record_jobs(|| tab.open_pr(7, &ctx));
    // What `pr_send` does: the post is on its way, and its answer may not be dropped.
    tab.pr_drafts.entry(7).or_default().posting = true;
    assert!(tab.forge_change_blocked());
    tab.close_pr();
    assert!(
        tab.forge_change_blocked(),
        "leaving the page does not end the post"
    );
    tab.pr_posted(7, Posted::Comment, Ok(()), &ctx);
    assert!(
        settled(&tab),
        "answered with the page closed, nothing is read again"
    );
    // Sent with the page open: what is left is reading it again, which a forge change ends.
    posted(&mut tab, &ctx)?;
    assert!(tab.pr_drafts[&7].posting, "nothing is sent by the old page");
    assert!(!tab.forge_change_blocked());
    Ok(())
}

#[test]
fn a_newer_read_of_the_same_page_takes_over_settling() -> Result<(), String> {
    let ctx = egui::Context::default();
    let mut tab = tab(&ctx);
    let old = posted(&mut tab, &ctx)?;
    let new = read_request(record_jobs(|| tab.load_pr(&ctx)).1)?;
    assert_ne!(old, new);
    // The superseded read's answer is not the page shown, so the draft still waits.
    for result in [Ok(details()), Err("old".into())] {
        tab.handle(Msg::PrDetails(old, Box::new(result)), &ctx);
        assert!(tab.pr_drafts[&7].posting);
    }
    let (_, jobs) = record_jobs(|| tab.handle(Msg::PrDetails(new, Box::new(Ok(details()))), &ctx));
    assert_eq!(read_request(jobs)?, new);
    assert!(tab.pr_drafts[&7].posting);
    record_jobs(|| tab.handle(Msg::PrFiles(new, Ok(files())), &ctx));
    assert!(settled(&tab));
    Ok(())
}
