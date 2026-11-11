use super::*;

#[test]
fn a_run_is_said_resolved_only_when_its_files_came_back_resolved() {
    let mut run = super::super::look_tests::run(Phase::Ready, Instant::now());
    // One of the four changed meanwhile.
    assert_eq!(title(&run), "Partly resolved 4 files");
    for (_, r) in &mut run.results {
        *r = FileResult::Resolved;
    }
    assert_eq!(title(&run), "Resolved 4 files");
    // The agent exited well, having resolved nothing.
    for (_, r) in &mut run.results {
        *r = FileResult::Untouched;
    }
    assert_eq!(title(&run), "Didn't resolve 4 files");
    run.stopping = true;
    assert_eq!(title(&run), "Stopped");
}
