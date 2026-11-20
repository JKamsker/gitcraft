//! The diff's coverage gutter, read (#173 item 8): what the coverage report says of the lines
//! the open diff adds, worked out in the background by the coverage map's own reading and
//! judging of reports ([`core::of_lines`]), then handed to the diff (`repo::diff::coverage`)
//! as marks it only paints.

use std::collections::HashMap;
use std::sync::Arc;

use crate::git::Area;
use crate::insights::Evidence;
use crate::insights::coverage::{self as core, LineCov, LinesCoverage, Side};
use crate::repo::diff::Kind;
use crate::repo::diff::coverage::{CoverageAct, Gutter, Mark, Overlay, Trust};
use crate::repo::{DiffKey, RepoTab};

/// The side of the diff `key` its added lines are numbered on, when a report can be about it.
pub(super) fn side_of(key: &DiffKey) -> Option<Side> {
    if !crate::repo::diff::coverage::applies(key) {
        return None;
    }
    match key {
        DiffKey::Commit { hash, .. } => Some(Side::Commit(hash.clone())),
        DiffKey::Range { to: Some(to), .. } => Some(Side::Commit(to.clone())),
        DiffKey::Range { to: None, .. } => Some(Side::WorkTree),
        DiffKey::Work(file) if file.area == Area::Staged => Some(Side::Index),
        DiffKey::Work(_) => Some(Side::WorkTree),
        DiffKey::Remerge { .. } => None,
    }
}

/// How far the report `cov` was read with can be trusted, in the gutter's terms.
pub(super) fn trust_of(cov: &LinesCoverage) -> Option<Trust> {
    let found = cov.found.as_ref()?;
    Some(if cov.index_differs {
        Trust::IndexDiffers
    } else if cov.evidence() != Evidence::Unknown {
        Trust::Current
    } else {
        match &found.recorded {
            None => Trust::NoRevision,
            Some(r) if !cov.sha.starts_with(r.as_str()) => Trust::Other(r.clone()),
            Some(_) if found.freshness.stale.is_some() => Trust::EditedAfter,
            Some(_) => Trust::Undetermined,
        }
    })
}

/// The marks of `cov`: `None` without a report. A line the report has nothing for is not
/// executable when the report has the file, and unknown when it hasn't.
pub(super) fn gutter_of(cov: &LinesCoverage) -> Option<Gutter> {
    let trust = trust_of(cov)?;
    let marks: HashMap<u32, Mark> = cov
        .lines
        .iter()
        .filter_map(|&(n, c)| {
            let mark = match c {
                LineCov::Covered(hits) => Mark::Covered(hits),
                LineCov::Uncovered => Mark::Uncovered,
                LineCov::NotInstrumented if cov.in_report => return None,
                LineCov::NotInstrumented | LineCov::Unknown => Mark::Unknown,
            };
            Some((n, mark))
        })
        .collect();
    Some(Gutter {
        report: cov
            .found
            .as_ref()
            .map(|f| f.path.clone())
            .unwrap_or_default(),
        sha: cov.sha.clone(),
        trust,
        marks,
    })
}

impl RepoTab {
    pub(in crate::repo) fn diff_coverage(&mut self, act: CoverageAct, ctx: &egui::Context) {
        match act {
            CoverageAct::Toggle => self.toggle_diff_coverage(ctx),
            CoverageAct::Load => self.load_diff_coverage(ctx),
        }
    }

    /// Reads, in the background, what the report says of the lines the open diff adds.
    fn load_diff_coverage(&mut self, ctx: &egui::Context) {
        let Some(view) = self.diff.as_mut() else {
            return;
        };
        let Some(Ok(data)) = &view.data else {
            return;
        };
        let source = data.source;
        let lines: Vec<u32> = data
            .lines
            .iter()
            .filter(|l| l.kind == Kind::Add)
            .filter_map(|l| l.new)
            .collect();
        let key = view.key.clone();
        view.coverage = Overlay {
            asked: Some(source),
            gutter: None,
        };
        let Some(side) = side_of(&key) else {
            view.coverage.gutter = Some(Ok(None));
            return;
        };
        let (repo, path) = (self.path.clone(), key.path().to_owned());
        self.then(
            ctx,
            move || {
                crate::git::offline(|| core::of_lines(&repo, &side, &path, &lines))
                    .map(|cov| gutter_of(&cov).map(Arc::new))
            },
            move |tab, result, _| {
                if let Some(view) = tab.diff.as_mut()
                    && view.key == key
                    && view.coverage.asked == Some(source)
                {
                    view.coverage.gutter = Some(result);
                }
            },
        );
    }
}
