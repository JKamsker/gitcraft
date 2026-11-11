use super::*;

#[test]
fn where_a_run_goes_from_who_touched_its_lines() {
    // All by the second commit of the branch: clear.
    assert_eq!(classify(&[Some(1), Some(1)]), (Match::Clear(1), vec![1]));
    // Two of the branch's commits: the newer suggested.
    assert_eq!(
        classify(&[Some(2), Some(0)]),
        (Match::Several(0), vec![0, 2])
    );
    // One of the branch's, one older: suggested, not clear.
    assert_eq!(classify(&[Some(3), None]), (Match::Several(3), vec![3]));
    // None of the branch's.
    assert_eq!(classify(&[None, None]), (Match::None, vec![]));
    assert_eq!(classify(&[]), (Match::None, vec![]));
}

fn file(patch: &str) -> FilePatch {
    patchset::files_of(patch.as_bytes(), vec!["f".to_owned()])
        .unwrap_or_default()
        .remove(0)
}

#[test]
fn runs_and_the_lines_that_decide_them() {
    let f = file(
        "diff --git a/f b/f\nindex 1111111111111111111111111111111111111111..2222222222222222222222222222222222222222 100644\n--- a/f\n+++ b/f\n@@ -1,6 +1,7 @@\n one\n-two\n+zwei\n three\n+new\n four\n-five\n six\n",
    );
    let runs = runs_of(&f);
    // "two"→"zwei" decided by line 2; "new" by the lines around it (3 and 4); "five" by 5.
    let decided: Vec<Vec<u32>> = runs.iter().map(|(_, old)| old.clone()).collect();
    assert_eq!(decided, [vec![2], vec![3, 4], vec![5]]);
    assert_eq!(runs[0].0.len(), 2);
}
