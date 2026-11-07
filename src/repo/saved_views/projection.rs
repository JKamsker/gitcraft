//! A display projection only: original commit identities and graph rows never change.
use crate::{git::Commit, graph::Graph};
use std::{collections::BTreeSet, ops::Range, sync::Arc};
const LANES: usize = 24;
#[derive(Clone, Debug)]
pub(in crate::repo) enum Row {
    Commit(usize),
    Gap {
        range: Range<usize>,
        top: Box<[Option<usize>; LANES]>,
        bottom: Box<[Option<usize>; LANES]>,
    },
}
#[derive(Default)]
pub(super) struct Projection {
    pub rows: Vec<Row>,
}
impl Projection {
    pub fn build(
        commits: &[Arc<Commit>],
        graph: &Graph,
        matches: &BTreeSet<String>,
        expanded: &BTreeSet<String>,
        context: Option<usize>,
        stashes: &BTreeSet<String>,
    ) -> Self {
        let keep = |i: usize| {
            context == Some(i)
                || !crate::git::investigations::oid(&commits[i].hash)
                || stashes.contains(&commits[i].hash)
                || matches.contains(&commits[i].hash)
                || expanded.contains(&commits[i].hash)
        };
        let mut rows = Vec::new();
        let mut i = 0;
        while i < commits.len() {
            if keep(i) {
                rows.push(Row::Commit(i));
                i += 1;
                continue;
            }
            let start = i;
            while i < commits.len() && !keep(i) {
                i += 1;
            }
            // Short gaps remain readable context, rather than making the view taller.
            if i - start < 3 {
                rows.extend((start..i).map(Row::Commit));
            } else {
                rows.push(Row::Gap {
                    range: start..i,
                    top: Box::new([None; LANES]),
                    bottom: Box::new([None; LANES]),
                });
            }
        }
        let gaps: Vec<_> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| matches!(r, Row::Gap { .. }).then_some(i))
            .collect();
        let mut active = [None; LANES];
        let mut next_origin = 0;
        let mut gap = 0;
        for (i, row) in graph.rows.iter().enumerate() {
            if let Some(index) = gaps.get(gap)
                && let Row::Gap { range, top, .. } = &mut rows[*index]
                && range.start == i
            {
                **top = active;
            }
            next_origin = next_origin.max(row.origin + 1);
            for &col in &row.incoming {
                if col < LANES {
                    active[col] = None;
                }
            }
            for (n, &col) in row.outgoing.iter().enumerate() {
                let origin = if n == 0 {
                    Some(row.origin)
                } else if row.through.contains(&col) || row.outgoing[..n].contains(&col) {
                    None
                } else {
                    let origin = next_origin;
                    next_origin += 1;
                    Some(origin)
                };
                if col < LANES
                    && let Some(origin) = origin
                {
                    active[col] = Some(origin);
                }
            }
            if let Some(index) = gaps.get(gap)
                && let Row::Gap { range, bottom, .. } = &mut rows[*index]
                && range.end == i + 1
            {
                **bottom = active;
                gap += 1;
            }
        }
        Self { rows }
    }
    pub fn position(&self, raw: usize) -> Option<usize> {
        self.rows.iter().position(|r| match r {
            Row::Commit(i) => *i == raw,
            Row::Gap { range, .. } => range.contains(&raw),
        })
    }
    pub fn next(&self, raw: usize, down: bool) -> Option<usize> {
        if down {
            self.rows.iter().find_map(|r| match r {
                Row::Commit(i) if *i > raw => Some(*i),
                _ => None,
            })
        } else {
            self.rows.iter().rev().find_map(|r| match r {
                Row::Commit(i) if *i < raw => Some(*i),
                _ => None,
            })
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn c(i: usize, parents: &[usize]) -> Arc<Commit> {
        Arc::new(Commit {
            hash: format!("{i:040x}"),
            parents: parents.iter().map(|p| format!("{p:040x}")).collect(),
            ..Default::default()
        })
    }
    #[test]
    fn expanding_preserves_raw_identity_and_selection_and_long_gaps_are_single_rows() {
        assert!(
            std::mem::size_of::<Row>() <= 48,
            "Commit rows must not allocate lane arrays for every visible commit"
        );
        let commits: Vec<_> = (0..20).map(|i| c(i, &[i + 1])).collect();
        let graph = crate::graph::layout(&commits);
        let matched = BTreeSet::from([commits[0].hash.clone(), commits[19].hash.clone()]);
        let p = Projection::build(
            &commits,
            &graph,
            &matched,
            &BTreeSet::new(),
            Some(10),
            &BTreeSet::new(),
        );
        assert_eq!(p.rows.len(), 5);
        assert_eq!(p.position(10), Some(2));
        assert_eq!(p.next(0, true), Some(10));
        assert_eq!(p.next(19, false), Some(10));
        let expanded = commits[1..10].iter().map(|c| c.hash.clone()).collect();
        let p = Projection::build(
            &commits,
            &graph,
            &matched,
            &expanded,
            Some(10),
            &BTreeSet::new(),
        );
        assert!(matches!(p.rows[1], Row::Commit(1)));
        assert_eq!(p.position(10), Some(10));
    }
    #[test]
    fn connector_never_claims_reused_columns_are_one_continuous_lane() -> Result<(), String> {
        let commits = vec![
            c(0, &[1]),
            c(1, &[2]),
            c(2, &[]),
            c(3, &[4]),
            c(4, &[5]),
            c(5, &[]),
            c(6, &[]),
        ];
        let graph = crate::graph::layout(&commits);
        let matched = BTreeSet::from([commits[0].hash.clone(), commits[5].hash.clone()]);
        let p = Projection::build(
            &commits,
            &graph,
            &matched,
            &BTreeSet::new(),
            None,
            &BTreeSet::new(),
        );
        let Row::Gap { top, bottom, range } = &p.rows[1] else {
            return Err("Missing gap".into());
        };
        assert_eq!(range, &(1..5));
        assert!(top[0].is_some());
        assert!(bottom[0].is_some());
        assert_ne!(top[0], bottom[0]);
        Ok(())
    }
    #[test]
    fn connector_boundaries_keep_merge_lanes_distinct_and_stashes_visible() -> Result<(), String> {
        let commits = vec![
            c(0, &[3, 1]),
            c(1, &[2]),
            c(2, &[4]),
            c(3, &[4]),
            c(4, &[5]),
            c(5, &[]),
        ];
        let graph = crate::graph::layout(&commits);
        let p = Projection::build(
            &commits,
            &graph,
            &BTreeSet::from([commits[0].hash.clone(), commits[5].hash.clone()]),
            &BTreeSet::new(),
            None,
            &BTreeSet::new(),
        );
        let Row::Gap { top, bottom, .. } = &p.rows[1] else {
            return Err("Missing gap".into());
        };
        assert_ne!(top[0], top[1]);
        assert_eq!(top[0], bottom[0]);
        assert_eq!(bottom[1], None);
        let p = Projection::build(
            &commits,
            &graph,
            &BTreeSet::new(),
            &BTreeSet::new(),
            None,
            &BTreeSet::from([commits[3].hash.clone()]),
        );
        assert!(p.rows.iter().any(|r| matches!(r, Row::Commit(3))));
        Ok(())
    }
}
