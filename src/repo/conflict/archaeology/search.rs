//! Finding the first pair of commits whose merge conflicts, as git-imerge does: with the
//! commits of each side since they diverged in order, `conflicts(i, j)` says whether the `i`th
//! commit of ours merged with the `j`th of theirs conflicts (1-based; 0 is where they
//! diverged, which merges with anything). Taking a later commit on either side is assumed to
//! keep a conflict, so two binary searches find the pair: the first of ours that conflicts with
//! all of theirs, then the first of theirs that conflicts with it.

/// Why the search stopped before it was done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Stop {
    Cancelled,
    Failed(String),
}

/// What the search found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Ours `i` and theirs `j` (1-based) conflict, while ours `i` merges cleanly with theirs
    /// `j - 1`; `checked`: theirs `j` merges cleanly with ours `i - 1` too. When it doesn't
    /// (a later commit didn't keep a conflict), an earlier pair conflicts as well.
    Pair {
        ours: usize,
        theirs: usize,
        checked: bool,
    },
    /// Only one side changed the file since they diverged: no pair of its commits conflicts.
    OneSided,
    /// The two tips merge without a conflict in the file: it comes from elsewhere (what was
    /// replayed before, a rename…).
    TipsMergeCleanly,
}

/// The pair, testing at most about `log2(ours) + log2(theirs) + 2` merges.
pub fn first_pair(
    ours: usize,
    theirs: usize,
    mut conflicts: impl FnMut(usize, usize) -> Result<bool, Stop>,
) -> Result<Outcome, Stop> {
    if ours == 0 || theirs == 0 {
        return Ok(Outcome::OneSided);
    }
    if !conflicts(ours, theirs)? {
        return Ok(Outcome::TipsMergeCleanly);
    }
    let i = first_true(ours, |i| conflicts(i, theirs))?;
    let j = first_true(theirs, |j| conflicts(i, j))?;
    let checked = i == 1 || !conflicts(i - 1, j)?;
    Ok(Outcome::Pair {
        ours: i,
        theirs: j,
        checked,
    })
}

/// The smallest `k` in `1..=last` for which `test` holds, `test(last)` known to hold and
/// `test(0)` taken not to.
fn first_true(
    last: usize,
    mut test: impl FnMut(usize) -> Result<bool, Stop>,
) -> Result<usize, Stop> {
    // `lo` fails (or is 0), `hi` holds.
    let (mut lo, mut hi) = (0, last);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if test(mid)? {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Conflicts from ours `a` and theirs `b` on: what a monotonic history gives.
    fn from(a: usize, b: usize) -> impl FnMut(usize, usize) -> Result<bool, Stop> {
        move |i, j| Ok(i >= a && j >= b)
    }

    #[test]
    fn finds_the_first_pair() {
        for (m, n) in [(1, 1), (1, 7), (9, 1), (10, 10), (1000, 3)] {
            for a in 1..=m.min(12) {
                for b in 1..=n.min(12) {
                    assert_eq!(
                        first_pair(m, n, from(a, b)),
                        Ok(Outcome::Pair {
                            ours: a,
                            theirs: b,
                            checked: true
                        }),
                        "{m}×{n} from {a},{b}"
                    );
                }
            }
        }
    }

    #[test]
    fn tests_few_merges() {
        let mut tested = 0;
        let found = first_pair(1000, 1000, |i, j| {
            tested += 1;
            Ok(i >= 700 && j >= 3)
        });
        assert_eq!(
            found,
            Ok(Outcome::Pair {
                ours: 700,
                theirs: 3,
                checked: true
            })
        );
        assert!(tested <= 22, "{tested}");
    }

    /// A history where a later commit doesn't keep a conflict: ours 1 conflicts with theirs 1
    /// only, ours 2 with both. The pair found (2, 1) is said not to hold: (1, 1) conflicts too.
    #[test]
    fn a_pair_that_does_not_hold_is_said_so() {
        let found = first_pair(2, 2, |i, j| Ok(i == 2 || j == 1));
        assert_eq!(
            found,
            Ok(Outcome::Pair {
                ours: 2,
                theirs: 1,
                checked: false
            })
        );
    }

    #[test]
    fn one_side_or_clean_tips() {
        assert_eq!(first_pair(0, 5, from(1, 1)), Ok(Outcome::OneSided));
        assert_eq!(first_pair(5, 0, from(1, 1)), Ok(Outcome::OneSided));
        assert_eq!(
            first_pair(3, 3, |_, _| Ok(false)),
            Ok(Outcome::TipsMergeCleanly)
        );
    }

    #[test]
    fn stops_when_asked_or_on_errors() {
        let mut n = 0;
        let cancelled = first_pair(50, 50, |i, j| {
            n += 1;
            if n > 2 {
                return Err(Stop::Cancelled);
            }
            Ok(i >= 1 && j >= 1)
        });
        assert_eq!(cancelled, Err(Stop::Cancelled));
        let failed = first_pair(4, 4, |_, _| Err(Stop::Failed("boom".into())));
        assert_eq!(failed, Err(Stop::Failed("boom".into())));
    }
}
