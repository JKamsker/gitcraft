use super::*;
use crate::forge::discovery::RepositoryDiscovery;
use std::sync::mpsc::{TryRecvError, channel};

impl Browser {
    pub(super) fn request(&mut self, query: Query, more: bool, ctx: &egui::Context) {
        if !self.session.valid() {
            self.invalidate();
        }
        if more && self.pending.is_some() {
            return;
        }
        let next = if more {
            self.next.clone()
        } else {
            self.reset();
            None
        };
        if more && next.is_none() {
            return;
        }
        self.query = Some(query.clone());
        self.error = None;
        if !query.public() && !self.session.account.selected() {
            self.error = Some(DiscoveryError::AuthRequired);
            return;
        }
        if let Some(next) = &next
            && !self.seen_pages.insert(next.clone())
        {
            self.next = None;
            return;
        }
        self.serial = self.serial.saturating_add(1);
        let tag = Tag {
            owner: self.owner,
            serial: self.serial,
            connection: self.session.generation,
            query,
        };
        let returned = tag.clone();
        let session = self.session.clone();
        let repaint = ctx.clone();
        let scope = crate::tasks::Scope::default();
        let (tx, rx) = channel();
        let result = crate::tasks::pool().and_then(|pool| {
            pool.submit(
                scope.0.clone(),
                crate::tasks::Priority::Interactive,
                None,
                Box::new(move || {
                    let result = session.repositories(&returned.query, next.as_deref());
                    let _ = tx.send((returned, result));
                    repaint.request_repaint();
                }),
            )
            .map_err(str::to_owned)
        });
        match result {
            Ok(()) => {
                self.pending = Some(Pending {
                    tag,
                    rx,
                    _scope: scope,
                })
            }
            Err(_) => self.apply(Err(DiscoveryError::QueueUnavailable)),
        }
    }
    pub(super) fn poll(&mut self) {
        if !self.session.valid() {
            self.invalidate();
            return;
        }
        let Some(pending) = &self.pending else {
            return;
        };
        let (tag, result) = match pending.rx.try_recv() {
            Ok(reply) => reply,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => {
                self.pending = None;
                self.apply(Err(DiscoveryError::Transport));
                return;
            }
        };
        let expected = pending.tag.clone();
        self.pending = None;
        if tag != expected
            || tag.owner != self.owner
            || tag.serial != self.serial
            || tag.connection != self.session.generation
            || self.query.as_ref() != Some(&tag.query)
        {
            return;
        }
        self.apply(result);
    }
    pub(super) fn apply(&mut self, result: Result<Page, DiscoveryError>) {
        match result {
            Ok(page) => {
                let before = self.repositories.len() + self.owners.len();
                let mut repos: HashSet<_> = self.repositories.iter().map(|repo| repo.id).collect();
                self.repositories.extend(
                    page.repositories
                        .into_iter()
                        .filter(|repo| repos.insert(repo.id)),
                );
                let mut owners: HashSet<_> = self.owners.iter().map(|owner| owner.id).collect();
                self.owners.extend(
                    page.owners
                        .into_iter()
                        .filter(|owner| owners.insert(owner.id)),
                );
                let count = self.repositories.len() + self.owners.len();
                self.next = if count > before && count < 1000 {
                    page.next
                } else {
                    None
                };
                self.total = page.total;
                self.incomplete = page.incomplete
                    || page.total.is_some_and(|total| total > 1000)
                    || count >= 1000;
                self.loaded = true;
                // Keep rate metadata transient; a failed request supplies actionable retry time.
                let _rate = page.rate;
            }
            Err(error) => {
                if matches!(
                    error,
                    DiscoveryError::AuthRefused | DiscoveryError::Cancelled
                ) {
                    self.repositories.clear();
                    self.owners.clear();
                    self.next = None;
                }
                // A failed page can be retried without losing already loaded rows.
                if let Some(next) = &self.next {
                    self.seen_pages.remove(next);
                }
                self.error = Some(error);
            }
        }
    }
}
