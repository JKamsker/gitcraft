//! _What a package will publish_: a panel with the files `cargo package --list` or
//! `npm pack --dry-run` would ship for a package of the repository, those of the project left
//! out, those that look shipped by accident, and how they differ from the version published.

#[cfg(test)]
mod tests;
mod view;

use egui::Context;

use super::RepoTab;
use crate::package::{self, Package, Published, Report};

/// The panel, while it is open.
pub(crate) struct PackageView {
    /// `None` while they are read.
    packages: Option<Result<Vec<Package>, package::Failure>>,
    selected: usize,
    /// The report of the package selected; `None` while it is worked out.
    report: Option<Result<Report, package::Failure>>,
    /// How it compares with the registry's version, asked once the report is in (`None`
    /// meanwhile); `Ok(None)`: never published there.
    published: Option<Result<Option<Published>, package::Failure>>,
    /// Numbers the reports asked for: only the latest is shown.
    request: u64,
}

/// What the panel asks for.
pub(crate) enum PackageAct {
    Open,
    Select(usize),
    /// The packages and the report read again (after a change to the files).
    Refresh,
    Close,
}

/// The panel, if `request` is still its latest.
fn current(tab: &mut RepoTab, request: u64) -> Option<&mut PackageView> {
    tab.package.as_mut().filter(|v| v.request == request)
}

/// A number no request of any panel had: one asked before a panel was closed and opened again
/// is never taken for the new one's.
fn next_request() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

impl RepoTab {
    pub(super) fn package_act(&mut self, act: PackageAct, ctx: &Context) {
        match act {
            PackageAct::Open | PackageAct::Refresh => {
                let selected = match (&act, &self.package) {
                    (PackageAct::Refresh, Some(view)) => view.selected,
                    _ => 0,
                };
                let request = next_request();
                self.package = Some(PackageView {
                    packages: None,
                    selected,
                    report: None,
                    published: None,
                    request,
                });
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || package::packages_witnessed(&repo),
                    move |tab, found, ctx| {
                        let Some(view) = &mut tab.package else { return };
                        if view.request != request {
                            return;
                        }
                        let count = found.as_ref().map_or(0, Vec::len);
                        view.packages = Some(found);
                        if count > 0 {
                            tab.package_act(PackageAct::Select(selected.min(count - 1)), ctx);
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
                view.published = None;
                view.request = next_request();
                let request = view.request;
                let repo = self.path.clone();
                self.then(
                    ctx,
                    move || package::report_witnessed(&repo, &pkg).map(|r| (r, pkg)),
                    move |tab, report, ctx| {
                        let Some(view) = current(tab, request) else {
                            return;
                        };
                        let report = match report {
                            Ok((report, pkg)) => {
                                let files = report.files.clone();
                                view.report = Some(Ok(report));
                                (pkg, files)
                            }
                            Err(e) => {
                                view.report = Some(Err(e));
                                return;
                            }
                        };
                        // The registry apart: it can be slow, the files are there meanwhile.
                        let (pkg, files) = report;
                        tab.then(
                            ctx,
                            move || package::published_witnessed(&pkg, &files),
                            move |tab, published, _| {
                                if let Some(view) = current(tab, request) {
                                    view.published = Some(published);
                                }
                            },
                        );
                    },
                );
            }
            PackageAct::Close => self.package = None,
        }
    }
}
