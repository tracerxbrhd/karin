// SPDX-License-Identifier: GPL-3.0-or-later
//! Per-request cache presentation; cached records never authenticate a session.

use super::*;
use crate::catalog_cache::{Cached, CachedCatalog, Provenance};

#[cfg(test)]
mod tests;

pub(super) fn network<T>(value: T) -> Cached<T> {
    Cached {
        value,
        provenance: Provenance::Network,
        lease: None,
        cache_error: None,
    }
}

impl Controller {
    pub(super) fn cached_catalog(&self) -> Option<CachedCatalog> {
        if let Some(catalog) = self.catalog.borrow().as_ref() {
            return Some(catalog.clone());
        }
        let cache = self.persistent_cache()?;
        let catalog = CachedCatalog::new(cache);
        self.catalog.replace(Some(catalog.clone()));
        Some(catalog)
    }

    pub(super) fn run_catalog<T, F, D>(
        self: &Rc<Self>,
        operation: Operation,
        library_page: Option<(u32, u32)>,
        force: bool,
        work: F,
        done: D,
    ) where
        T: Send + 'static,
        F: Fn(
                MusicService,
                AccessToken,
                Option<CachedCatalog>,
                bool,
            ) -> Result<Cached<T>, MusicError>
            + Send
            + Sync
            + 'static,
        D: FnOnce(&Rc<Self>, Result<T, MusicError>) + 'static,
    {
        self.run_catalog_attempt(operation, library_page, force, Arc::new(work), done);
    }

    fn run_catalog_attempt<T, F, D>(
        self: &Rc<Self>,
        operation: Operation,
        library_page: Option<(u32, u32)>,
        force: bool,
        work: Arc<F>,
        done: D,
    ) where
        T: Send + 'static,
        F: Fn(
                MusicService,
                AccessToken,
                Option<CachedCatalog>,
                bool,
            ) -> Result<Cached<T>, MusicError>
            + Send
            + Sync
            + 'static,
        D: FnOnce(&Rc<Self>, Result<T, MusicError>) + 'static,
    {
        let catalog = self.cached_catalog();
        let worker = work.clone();
        self.run(
            operation,
            move |service, token| worker(service, token, catalog, force),
            move |this, result| {
                match result {
                    Ok(reply) => {
                        if reply
                            .lease
                            .as_ref()
                            .is_some_and(|lease| !lease.is_current())
                        {
                            if force {
                                done(this, Err(MusicError::Network));
                            } else {
                                // A clear/disable can retire a decoded cached reply
                                // before GTK receives it. Retry this read once from
                                // the network, through the same bounded request lane.
                                this.run_catalog_attempt(operation, library_page, true, work, done);
                            }
                            return;
                        }
                        let provenance = reply.provenance;
                        let cache_error = reply.cache_error.is_some();
                        if let Some((section, page)) = library_page {
                            this.remember_library_origin(section, page, provenance, cache_error);
                        } else if operation == Operation::Detail {
                            this.detail_cache.set(Some((provenance, cache_error)));
                        }
                        done(this, Ok(reply.value));
                        this.refresh_catalog_notice();
                    }
                    Err(error) => {
                        if operation == Operation::Detail {
                            this.detail_cache.set(None);
                        }
                        done(this, Err(error));
                        this.refresh_catalog_notice();
                    }
                }
            },
        );
    }

    pub(super) fn show_cache_provenance(&self, provenance: Provenance, storage_error: bool) {
        if !self.model.borrow().account_valid {
            self.widgets.cache_notice.set_revealed(false);
            self.widgets.cache_hint.set_visible(false);
            return;
        }
        let stale = provenance == Provenance::StaleCache;
        self.widgets.cache_notice.set_title(if stale {
            "Часть данных показана из сохранённой копии и может быть устаревшей."
        } else {
            "Данные получены, но не сохранены в кеше."
        });
        self.widgets
            .cache_notice
            .set_revealed(stale || storage_error);
        self.widgets
            .cache_hint
            .set_visible(provenance == Provenance::FreshCache);
    }

    pub(super) fn remember_library_origin(
        &self,
        section: u32,
        page: u32,
        provenance: Provenance,
        storage_error: bool,
    ) {
        let mut model = self.model.borrow_mut();
        model
            .library_origins
            .retain(|(s, p, _, _)| *s != section || *p != page);
        if page < 100 {
            model
                .library_origins
                .push((section, page, provenance, storage_error));
        }
    }

    pub(super) fn refresh_catalog_notice(&self) {
        if let Some(origin) = self.visible_feed_cache_state() {
            if let Some((origin, error)) = origin {
                self.show_cache_provenance(origin, error);
            } else {
                self.widgets.cache_notice.set_revealed(false);
                self.widgets.cache_hint.set_visible(false);
            }
            return;
        }
        let model = self.model.borrow();
        let key = match self.widgets.stack.visible_child_name().as_deref() {
            Some("library") => model.library_displaying,
            Some("playlists") => {
                let source = continuous_loading::aggregate_origins(
                    model
                        .library_origins
                        .iter()
                        .filter(|(section, _, _, _)| match section {
                            1 => model.playlists.as_ref().is_some_and(Result::is_ok),
                            2 => model.liked_playlists.as_ref().is_some_and(Result::is_ok),
                            _ => false,
                        })
                        .map(|(_, _, origin, error)| (*origin, *error)),
                );
                drop(model);
                if let Some((origin, error)) = source {
                    self.show_cache_provenance(origin, error);
                } else {
                    self.widgets.cache_notice.set_revealed(false);
                    self.widgets.cache_hint.set_visible(false);
                }
                return;
            }
            Some("detail") => {
                let source = self.detail_cache.get();
                drop(model);
                if let Some((origin, error)) = source {
                    self.show_cache_provenance(origin, error);
                } else {
                    self.widgets.cache_notice.set_revealed(false);
                    self.widgets.cache_hint.set_visible(false);
                }
                return;
            }
            _ => None,
        };
        let source = key.and_then(|(section, page)| {
            model
                .library_origins
                .iter()
                .find(|(s, p, _, _)| *s == section && *p == page)
                .map(|(_, _, origin, error)| (*origin, *error))
        });
        drop(model);
        if let Some((origin, error)) = source {
            self.show_cache_provenance(origin, error);
        } else {
            self.widgets.cache_notice.set_revealed(false);
            self.widgets.cache_hint.set_visible(false);
        }
    }

    pub(super) fn refresh_catalog(self: &Rc<Self>) {
        let page = self.widgets.stack.visible_child_name();
        match page.as_deref() {
            Some("library") => {
                let page = self.model.borrow().library_page;
                self.load_library_refresh(page);
            }
            Some("playlists") => self.load_playlist_sections(true),
            Some("detail") => {
                let action = self.detail_refresh.borrow().clone();
                if let Some(action) = action {
                    action(self);
                }
            }
            _ => self.load_initial_library_policy(true),
        }
    }
}
