//! _What a package will publish_: a panel with the files `cargo package --list` or
//! `npm pack --dry-run` would ship for a package of the repository, those of the project left
//! out, those that look shipped by accident, and how they differ from the version published.

#[cfg(test)]
mod tests;
mod view;

use egui::Context;

use super::RepoTab;
use crate::package::{self, Package, Report};

/// The panel, while it is open.
pub(crate) struct PackageView {
    /// `None` while they are read.
    packages: Option<Result<Vec<Package>, String>>,
    selected: usize,
    /// The report of the package selected; `None` while it is worked out.
    report: Option<Result<Report, String>>,
    /// Numbers the reports asked for: only the latest is shown.
    request: u64,
}

/// What the panel asks for.
pub(crate) enum PackageAct {
    Open,
    Select(usize),
    Close,
}

impl RepoTab {
    pub(super) fn package_act(&mut self, act: PackageAct, ctx: &Context) {
        match act {
            PackageAct::Open => {
                self.package = Some(PackageView {
                    packages: None,
                    selected: 0,
                    report: None,
                    request: 0,
                });
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || package::packages(&repo),
                    |tab, found, ctx| {
                        let Some(view) = &mut tab.package else { return };
                        let any = found.as_ref().is_ok_and(|p| !p.is_empty());
                        view.packages = Some(found);
                        if any {
                            tab.package_act(PackageAct::Select(0), ctx);
                        }
                    },
                );
            }
            PackageAct::Select(i) => {
                let Some(view) = &mut self.package else {
                    return;
                };
                let Some(Ok(list)) = &view.packages else {
                    return;
                };
                let Some(pkg) = list.get(i).cloned() else {
                    return;
                };
                view.selected = i;
                view.report = None;
                view.request += 1;
                let request = view.request;
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || package::report(&repo, &pkg, true),
                    move |tab, report, _| {
                        if let Some(view) = &mut tab.package
                            && view.request == request
                        {
                            view.report = Some(report);
                        }
                    },
                );
            }
            PackageAct::Close => self.package = None,
        }
    }
}
