//! Delta debugging (Zeller's `ddmin`): from a set of changes that fails, a smallest subset
//! that still fails, every part of which passes once any one change is left out ("1-minimal").
//! Tests that can't decide (the changes don't apply together) count as neither.

use std::collections::HashMap;

/// What a test said of a subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    Pass,
    Fail,
    /// It couldn't be told: the subset doesn't apply, or the result didn't repeat.
    Unresolved,
}

/// Why `ddmin` stopped before the end.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Halt {
    /// It was stopped.
    Stopped,
    /// A test couldn't be run.
    Error(String),
    /// The test is out of budget (it counts tests of its own): what was found so far stands.
    OutOfBudget,
}

/// What `ddmin` found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reduced {
    /// The smallest failing subset found (indices into the changes, in order).
    pub failing: Vec<usize>,
    /// It is 1-minimal: none was cut short by the budget.
    pub minimal: bool,
    /// Some of it left out one at a time couldn't be told (didn't apply…): those may not pass.
    pub gaps: bool,
    /// How many tests were run (each subset once).
    pub tests: usize,
}

/// Reduces `n` changes (0..n), known to fail together, with `test` (given a subset in order),
/// running at most `budget` tests. Each subset is tested once; `known` are subsets tested
/// already, with what they said.
pub fn ddmin(
    n: usize,
    budget: usize,
    known: &[(Vec<usize>, Probe)],
    test: &mut dyn FnMut(&[usize]) -> Result<Probe, Halt>,
) -> Result<Reduced, Halt> {
    let mut seen: HashMap<Vec<usize>, Probe> = known.iter().cloned().collect();
    let mut tests = 0;
    let mut probe = |subset: &[usize], tests: &mut usize| -> Result<Option<Probe>, Halt> {
        if subset.is_empty() {
            // Nothing applied: what both branches start from, which passes.
            return Ok(Some(Probe::Pass));
        }
        if let Some(p) = seen.get(subset) {
            return Ok(Some(*p));
        }
        if *tests >= budget {
            return Ok(None);
        }
        *tests += 1;
        let p = match test(subset) {
            Err(Halt::OutOfBudget) => return Ok(None),
            other => other?,
        };
        seen.insert(subset.to_vec(), p);
        Ok(Some(p))
    };
    let mut current: Vec<usize> = (0..n).collect();
    let mut granularity = 2;
    while current.len() >= 2 {
        let chunks = split(&current, granularity);
        let mut reduced = false;
        let mut out_of_budget = false;
        // A chunk alone that fails.
        for chunk in &chunks {
            match probe(chunk, &mut tests)? {
                Some(Probe::Fail) => {
                    current = chunk.clone();
                    granularity = 2;
                    reduced = true;
                    break;
                }
                None => {
                    out_of_budget = true;
                    break;
                }
                _ => {}
            }
        }
        // All but a chunk that fails (with two chunks that is the other chunk, done above).
        if !reduced && !out_of_budget && chunks.len() > 2 {
            for chunk in &chunks {
                let rest: Vec<usize> = current
                    .iter()
                    .copied()
                    .filter(|i| !chunk.contains(i))
                    .collect();
                match probe(&rest, &mut tests)? {
                    Some(Probe::Fail) => {
                        current = rest;
                        granularity = (granularity - 1).max(2);
                        reduced = true;
                        break;
                    }
                    None => {
                        out_of_budget = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
        if out_of_budget {
            return Ok(Reduced {
                failing: current,
                minimal: false,
                gaps: false,
                tests,
            });
        }
        if reduced {
            continue;
        }
        if granularity >= current.len() {
            break;
        }
        granularity = (granularity * 2).min(current.len());
    }
    // Each change left out of the result was tested (the last round splits it one by one):
    // those that couldn't be told leave a gap in "leaving any out passes".
    let gaps = current.iter().any(|left_out| {
        let rest: Vec<usize> = current.iter().copied().filter(|i| i != left_out).collect();
        seen.get(&rest) == Some(&Probe::Unresolved)
    });
    Ok(Reduced {
        failing: current,
        minimal: true,
        gaps,
        tests,
    })
}

/// `items` in `parts` runs as even as can be, in order.
fn split(items: &[usize], parts: usize) -> Vec<Vec<usize>> {
    let parts = parts.clamp(1, items.len().max(1));
    let mut out = Vec::with_capacity(parts);
    let mut start = 0;
    for i in 0..parts {
        let end = start + (items.len() - start) / (parts - i);
        out.push(items[start..end].to_vec());
        start = end;
    }
    out.retain(|c| !c.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test that fails when every change of `culprits` is in the subset.
    fn oracle(culprits: &[usize]) -> impl FnMut(&[usize]) -> Result<Probe, Halt> + '_ {
        move |subset| {
            Ok(if culprits.iter().all(|c| subset.contains(c)) {
                Probe::Fail
            } else {
                Probe::Pass
            })
        }
    }

    #[test]
    fn splits_are_even_and_in_order() {
        assert_eq!(split(&[0, 1, 2, 3, 4], 2), [vec![0, 1], vec![2, 3, 4]]);
        assert_eq!(split(&[0, 1, 2], 3), [vec![0], vec![1], vec![2]]);
        assert_eq!(split(&[0, 1], 5), [vec![0], vec![1]]);
        assert!(split(&[], 2).is_empty());
    }

    #[test]
    fn finds_the_pair_that_fails_together() -> Result<(), Halt> {
        for (n, pair) in [
            (2, [0, 1]),
            (8, [1, 6]),
            (20, [3, 17]),
            (33, [0, 32]),
            (16, [7, 8]),
        ] {
            let reduced = ddmin(n, 500, &[], &mut oracle(&pair))?;
            assert_eq!(reduced.failing, pair, "n = {n}");
            assert!(reduced.minimal);
            assert!(reduced.tests <= 4 * n, "n = {n}: {} tests", reduced.tests);
        }
        Ok(())
    }

    #[test]
    fn finds_a_single_culprit_and_a_triple() -> Result<(), Halt> {
        assert_eq!(ddmin(10, 500, &[], &mut oracle(&[4]))?.failing, [4]);
        assert_eq!(
            ddmin(12, 500, &[], &mut oracle(&[1, 5, 11]))?.failing,
            [1, 5, 11]
        );
        Ok(())
    }

    /// Subsets that don't apply (the second change needs the first) are stepped around.
    #[test]
    fn unresolved_subsets_are_stepped_around() -> Result<(), Halt> {
        let mut test = |subset: &[usize]| -> Result<Probe, Halt> {
            if subset.contains(&3) && !subset.contains(&2) {
                return Ok(Probe::Unresolved);
            }
            Ok(if subset.contains(&3) && subset.contains(&6) {
                Probe::Fail
            } else {
                Probe::Pass
            })
        };
        let reduced = ddmin(8, 500, &[], &mut test)?;
        assert_eq!(reduced.failing, [2, 3, 6]);
        // Leaving 2 out doesn't apply: "leaving any out passes" isn't quite so.
        assert!(reduced.gaps);
        Ok(())
    }

    #[test]
    fn a_budget_stops_it_with_what_it_has() -> Result<(), Halt> {
        let reduced = ddmin(64, 3, &[], &mut oracle(&[5, 40]))?;
        assert!(!reduced.minimal);
        assert_eq!(reduced.tests, 3);
        assert!(reduced.failing.contains(&5) && reduced.failing.contains(&40));
        Ok(())
    }

    #[test]
    fn a_halt_comes_through() {
        let mut test = |_: &[usize]| Err(Halt::Stopped);
        assert_eq!(ddmin(4, 10, &[], &mut test), Err(Halt::Stopped));
    }

    /// Subsets tested before aren't tested again.
    #[test]
    fn known_results_are_not_tested_again() -> Result<(), Halt> {
        let known = [(vec![0, 1, 2], Probe::Pass), (vec![3, 4, 5], Probe::Pass)];
        let mut asked = Vec::new();
        let mut test = |subset: &[usize]| {
            asked.push(subset.to_vec());
            Ok(if subset.contains(&1) && subset.contains(&4) {
                Probe::Fail
            } else {
                Probe::Pass
            })
        };
        let reduced = ddmin(6, 100, &known, &mut test)?;
        assert_eq!(reduced.failing, [1, 4]);
        assert!(!reduced.gaps);
        assert!(!asked.contains(&vec![0, 1, 2]) && !asked.contains(&vec![3, 4, 5]));
        Ok(())
    }

    #[test]
    fn nothing_to_reduce() -> Result<(), Halt> {
        let reduced = ddmin(1, 10, &[], &mut oracle(&[0]))?;
        assert_eq!((reduced.failing, reduced.tests), (vec![0], 0));
        Ok(())
    }
}
