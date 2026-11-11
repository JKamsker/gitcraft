use super::*;

fn side(patch: &str) -> Side {
    Side {
        patch: patch.to_owned(),
        hunks: crate::agent::bakeoff::hunks::parse(patch),
    }
}

#[test]
fn hunks_are_named_by_their_lines() {
    let s = side(
        "@@ -2 +2 @@\n-b\n+B\n@@ -4,2 +4 @@\n-d\n-e\n+x\n@@ -5,0 +6 @@\n+y\n@@ -0,0 +1 @@\n+top\n",
    );
    let names: Vec<String> = s.hunks.iter().map(hunk_label).collect();
    assert_eq!(
        names,
        [
            "Line 2: −1 +1",
            "Lines 4–5: −2 +1",
            "After line 5: −0 +1",
            "At the top: −0 +1"
        ]
    );
}

#[test]
fn picked_hunks_of_two_attempts_on_the_same_lines_are_flagged() {
    let a = side("@@ -2,2 +2 @@\n-b\n-c\n+x\n");
    let b = side("@@ -3 +3 @@\n-c\n+y\n@@ -6,0 +7 @@\n+z\n");
    let sides = [a, b];
    assert_eq!(clashes(&sides, &[(0, 0), (1, 0)]).len(), 2);
    assert!(clashes(&sides, &[(0, 0), (1, 1)]).is_empty());
}

#[test]
fn the_next_name_counts_up() {
    assert_eq!(super::super::next_name("bakeoff/x"), "bakeoff/x-2");
    assert_eq!(super::super::next_name("bakeoff/x-2"), "bakeoff/x-3");
    assert_eq!(super::super::next_name("agent/fix-1"), "agent/fix-1-2");
}
