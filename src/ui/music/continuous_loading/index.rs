// SPDX-License-Identifier: GPL-3.0-or-later
//! Full liked collection and debounced search over bounded membership and track batches.
//! Batches are query-independent; each reply projects the current local query.

use super::*;

#[cfg(test)]
mod native_tests;
pub(super) fn source(feed: &Feed) -> Option<CollectionSource> {
    match &feed.source {
        FeedSource::Liked => Some(CollectionSource::Liked),
        FeedSource::Playlist(playlist) => Some(CollectionSource::Playlist(playlist.id.clone())),
        FeedSource::SearchPlaylist(playlist) => {
            Some(CollectionSource::Playlist(playlist.id.clone()))
        }
        _ => None,
    }
}
pub(super) fn has_dates(feed: &Feed) -> bool {
    let index = feed.index.borrow();
    !index.changed
        && index
            .snapshot
            .as_ref()
            .is_some_and(CollectionSnapshot::has_added_dates)
}

pub(super) fn update_window_buttons(feed: &Feed, bar: &collection_filter::FilterBar, count: usize) {
    let windowed = feed.windowed.get();
    bar.previous
        .set_visible(windowed && feed.window_start.get() > 0);
    bar.next
        .set_visible(windowed && feed.window_start.get() + feed.result_window() < count);
}

enum IndexReply {
    Manifest(CollectionSnapshot),
    Batch(IndexedBatch),
    Verified(bool),
}

impl Controller {
    pub(super) fn render_collection_window(self: &Rc<Self>, feed: &Feed) {
        let data = feed.data.borrow();
        let count = data.filter.visible().len();
        let window = feed.result_window();
        let start = feed
            .window_start
            .get()
            .min(count.saturating_sub(1) / window * window);
        feed.window_start.set(start);
        let positions = data
            .filter
            .visible()
            .iter()
            .skip(start)
            .take(window)
            .copied()
            .collect::<Vec<_>>();
        drop(data);
        let rendered = feed.rendered_positions.borrow();
        if *rendered == positions && (!positions.is_empty() || feed.rows.first_child().is_none()) {
            return;
        }
        // Retain the mounted prefix as tracks arrive; do not replace a row
        // the listener might be focusing just because a batch appended below.
        let reuse = !rendered.is_empty() && positions.starts_with(&rendered);
        let kept = if reuse { rendered.len() } else { 0 };
        drop(rendered);
        if !reuse {
            clear_list(&feed.rows);
        }
        for index in &positions[kept..] {
            self.append_feed_row(feed, *index);
        }
        feed.rendered_positions.replace(positions);
    }

    pub(super) fn turn_liked_window(
        self: &Rc<Self>,
        adjustment: &gtk::Adjustment,
        forward: bool,
        keyboard: bool,
    ) -> bool {
        let edge = if forward {
            adjustment.value() + adjustment.page_size() + 1.0 >= adjustment.upper()
        } else {
            adjustment.value() <= adjustment.lower() + 1.0
        };
        if !edge || adjustment.page_size() <= 0.0 {
            return false;
        }
        let feed = self
            .track_feeds
            .borrow()
            .iter()
            .find(|feed| {
                feed.auto_index.get()
                    && feed.rows.is_mapped()
                    && !feed.adjustment_guard.get()
                    && !feed.window_turning.get()
            })
            .cloned();
        let Some(feed) = feed.filter(|feed| self.feed_is_current(feed)) else {
            return false;
        };
        let start = feed.window_start.get();
        let window = feed.result_window();
        let next = if forward {
            let count = feed.data.borrow().filter.visible().len();
            if start + window >= count {
                return false;
            }
            start + window
        } else if start == 0 {
            return false;
        } else {
            start.saturating_sub(window)
        };
        feed.window_turning.set(true);
        feed.window_start.set(next);
        self.render_collection_window(&feed);
        self.render_feed_status(&feed);
        self.sync_catalog_rows();
        if forward {
            adjustment.set_value(adjustment.lower());
        }
        let weak_feed = Rc::downgrade(&feed);
        let adjustment = adjustment.clone();
        feed.rows.add_tick_callback(move |rows, _| {
            let Some(feed) = weak_feed.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !rows.is_mapped() || feed.window_start.get() != next || !feed.window_turning.get() {
                feed.window_turning.set(false);
                return glib::ControlFlow::Break;
            }
            let row = if forward {
                rows.first_child()
            } else {
                rows.last_child()
            };
            let Some(row) = row else {
                feed.window_turning.set(false);
                return glib::ControlFlow::Break;
            };
            if row.height() <= 0 || adjustment.upper() + 1.0 < f64::from(rows.height()) {
                return glib::ControlFlow::Continue;
            }
            if !forward {
                adjustment.set_value(
                    (adjustment.upper() - adjustment.page_size()).max(adjustment.lower()),
                );
            }
            if keyboard {
                row.grab_focus();
            }
            feed.window_turning.set(false);
            glib::ControlFlow::Break
        });
        true
    }

    pub(super) fn retire_collection_index(&self, feed: &Feed) {
        feed.cancel_index_debounce();
        let mut index = feed.index.borrow_mut();
        if index.running || index.busy {
            index.running = false;
            index.busy = false;
            index.serial = index.serial.wrapping_add(1);
            self.requests
                .borrow_mut()
                .invalidate(Operation::CollectionIndex);
        }
    }

    pub(super) fn pause_collection_index(self: &Rc<Self>, feed: &Feed) {
        self.retire_collection_index(feed);
        feed.index.borrow_mut().paused = true;
        self.sync_busy();
        self.render_feed_status(feed);
    }

    fn collection_index_available(&self, feed: &Feed, one_page: bool) -> bool {
        if !self.feed_is_current(feed) || !feed.rows.is_mapped() {
            return false;
        }
        let data = feed.data.borrow();
        let index = feed.index.borrow();
        let requested = if feed.auto_index.get() {
            !one_page && !data.loading
        } else {
            data.filter.active() != one_page
        };
        requested
            && !index.complete
            && !index.running
            && index.error != Some(MusicError::ResponseTooLarge)
            && (data.indexed
                || data.has_more
                || data.incomplete
                || data.error.is_some()
                || feed.restart_required.get())
    }

    pub(super) fn start_liked_index(self: &Rc<Self>, feed: &Feed) {
        if !feed.auto_index.get() {
            return;
        }
        let data = feed.data.borrow();
        let index = feed.index.borrow();
        let ready = ((data.next_page > 0 || data.indexed) && (data.has_more || data.incomplete)
            || matches!(
                data.error,
                Some(MusicError::InvalidResponse | MusicError::ResponseTooLarge)
            ))
            && !index.paused
            && !index.changed
            && index.error.is_none();
        drop(index);
        drop(data);
        if ready {
            self.start_collection_index(feed, false);
        }
    }

    pub(super) fn schedule_collection_index(self: &Rc<Self>, feed: &Feed) {
        feed.cancel_index_debounce();
        if source(feed).is_none() || !self.collection_index_available(feed, false) {
            return;
        }
        let index = feed.index.borrow();
        if index.paused || index.error.is_some() || index.changed {
            return;
        }
        drop(index);
        let weak = Rc::downgrade(self);
        let weak_feed = Rc::downgrade(feed);
        let timeout = glib::timeout_add_local_once(COLLECTION_SEARCH_DEBOUNCE, move || {
            if let Some(feed) = weak_feed.upgrade() {
                feed.index_debounce.borrow_mut().take();
                if let Some(this) = weak.upgrade() {
                    this.start_collection_index(&feed, false);
                }
            }
        });
        feed.index_debounce.replace(Some(timeout));
    }

    pub(super) fn activate_collection_search(self: &Rc<Self>, feed: &Feed) {
        feed.cancel_index_debounce();
        if source(feed).is_some() {
            self.start_collection_index(feed, false);
        } else if self.feed_is_current(feed)
            && feed.rows.is_mapped()
            && feed.data.borrow().filter.active()
        {
            // An artist page has no full membership contract. Album slices
            // are already local, and both keep the existing bounded prefix.
            self.request_track_feed(feed, false);
        }
    }

    fn start_collection_index(self: &Rc<Self>, feed: &Feed, one_page: bool) {
        if !self.collection_index_available(feed, one_page) {
            return;
        }
        // One indexing lane. A new collection retires the preceding intent.
        for other in self.track_feeds.borrow().clone() {
            if !Rc::ptr_eq(&other, feed) {
                self.retire_collection_index(&other);
            }
        }
        let mut index = feed.index.borrow_mut();
        if index.changed {
            *index = CollectionIndex {
                serial: index.serial.wrapping_add(1),
                ..Default::default()
            };
        }
        index.running = true;
        index.one_page = one_page;
        if !one_page {
            index.paused = false;
        }
        index.error = None;
        index.serial = index.serial.wrapping_add(1);
        drop(index);
        // No outstanding old prefix request may append to the new manifest.
        let operation = match feed.source {
            FeedSource::Liked => Operation::Library,
            _ => Operation::Detail,
        };
        self.interrupt_feed_reads(operation);
        self.requests.borrow_mut().invalidate(operation);
        let mut data = feed.data.borrow_mut();
        data.loading = false;
        data.error = None;
        data.indexed = true;
        drop(data);
        if !feed.windowed.replace(true) {
            // The ordinary rows are in provider sequence, not a window.
            feed.rendered_positions.borrow_mut().clear();
        }
        self.render_collection_window(feed);
        self.render_feed_status(feed);
        self.request_collection_index(feed);
    }

    fn request_collection_index(self: &Rc<Self>, feed: &Feed) {
        if !self.feed_is_current(feed) || !feed.rows.is_mapped() {
            return;
        }
        let Some(source) = source(feed) else {
            return;
        };
        let (snapshot, cursor, serial) = {
            let mut index = feed.index.borrow_mut();
            if !index.running || index.busy || index.complete {
                return;
            }
            index.busy = true;
            (index.snapshot.clone(), index.cursor, index.serial)
        };
        let completed = feed.clone();
        self.run(
            Operation::CollectionIndex,
            move |service, token| match snapshot {
                None => service
                    .collection_snapshot(&token, &source)
                    .map(IndexReply::Manifest),
                Some(snapshot) if cursor >= snapshot.len() => service
                    .collection_snapshot(&token, &source)
                    .map(|current| IndexReply::Verified(snapshot.same_membership(&current))),
                Some(snapshot) => service
                    .collection_index_batch(&token, &snapshot, cursor)
                    .map(IndexReply::Batch),
            },
            move |this, result| {
                if !this.feed_is_current(&completed) || completed.index.borrow().serial != serial {
                    return;
                }
                if !completed.rows.is_mapped() {
                    this.pause_collection_index(&completed);
                    return;
                }
                completed.index.borrow_mut().busy = false;
                let mut browsing_page_finished = false;
                match result {
                    Ok(IndexReply::Manifest(snapshot)) => {
                        completed.restart_required.set(false);
                        let mut data = completed.data.borrow_mut();
                        let prefix = data.positions.iter().copied().eq(0..data.items.len())
                            && snapshot.matches_prefix(&data.items);
                        let cursor = if prefix { data.items.len() } else { 0 };
                        let mut remount_rows = !prefix;
                        if prefix {
                            // Bind retained rows to the fresh manifest. Their
                            // original menus may have an obsolete revision
                            // or may have been built from cached display data.
                            remount_rows = data.rebind_prefix_removals(&snapshot);
                        }
                        if !prefix {
                            let old = ring::digest::digest(
                                &ring::digest::SHA256,
                                data.source_key.as_bytes(),
                            );
                            let key = old
                                .as_ref()
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect::<String>();
                            let epoch = completed.index.borrow().serial;
                            data.source_key = format!("continuous:index:{key}:{epoch}");
                            data.items.clear();
                            data.positions.clear();
                            data.removals.clear();
                            let sort = data.filter.sort();
                            data.filter = collection_filter::CollectionFilter::default();
                            if let Some(bar) = &completed.filter_ui {
                                data.filter.set_query(&bar.search.text());
                            }
                            data.filter.set_sort(sort);
                        }
                        let LoadedTracks {
                            positions, filter, ..
                        } = &mut *data;
                        if !snapshot.has_added_dates() && filter.sort().needs_dates() {
                            filter.set_sort(collection_filter::CollectionSort::Provider);
                        }
                        filter.replace_dates(
                            positions
                                .iter()
                                .map(|position| snapshot.added_at(*position)),
                        );
                        data.contexts.borrow_mut().clear();
                        // Totals remain provisional until the final manifest
                        // confirms that membership did not change underneath us.
                        data.has_more = true;
                        data.incomplete = false;
                        let mut index = completed.index.borrow_mut();
                        index.cursor = cursor;
                        index.snapshot = Some(snapshot);
                        drop(index);
                        drop(data);
                        if remount_rows {
                            completed.rendered_positions.borrow_mut().clear();
                            clear_list(&completed.rows);
                        }
                    }
                    Ok(IndexReply::Batch(batch)) => {
                        let mut data = completed.data.borrow_mut();
                        let snapshot = completed.index.borrow().snapshot.clone();
                        data.append_indexed(&batch, snapshot.as_ref());
                        let mut index = completed.index.borrow_mut();
                        index.cursor = batch.next_position;
                        let remaining = index
                            .snapshot
                            .as_ref()
                            .is_some_and(|snapshot| index.cursor < snapshot.len());
                        if index.one_page && remaining {
                            index.running = false;
                            browsing_page_finished = true;
                        }
                    }
                    Ok(IndexReply::Verified(same)) => {
                        let mut index = completed.index.borrow_mut();
                        index.complete = same;
                        index.changed = !same;
                        index.running = false;
                        index.error = (!same).then_some(MusicError::InvalidResponse);
                        let mut data = completed.data.borrow_mut();
                        data.has_more = !same;
                        if same
                            && index
                                .snapshot
                                .as_ref()
                                .is_some_and(CollectionSnapshot::is_truncated)
                        {
                            data.incomplete = true;
                        }
                        if !same && data.filter.sort().needs_dates() {
                            data.set_sort(collection_filter::CollectionSort::Provider);
                            completed.window_start.set(0);
                        }
                    }
                    Err(error) => {
                        let mut index = completed.index.borrow_mut();
                        index.running = false;
                        index.error = Some(error);
                        if matches!(
                            error,
                            MusicError::AccessDenied | MusicError::Unauthenticated
                        ) {
                            drop(index);
                            this.show_error(error);
                            return;
                        }
                    }
                }
                this.render_collection_window(&completed);
                this.render_feed_status(&completed);
                this.sync_catalog_rows();
                if browsing_page_finished {
                    // A query typed during this explicit page can use its
                    // newly admitted data and then follow normal debounce.
                    this.schedule_collection_index(&completed);
                } else if completed.index.borrow().running {
                    // Yield to native input and paint between bounded batches.
                    // Each reply uses the current projection, regardless of
                    // the query text before this request began.
                    let weak = Rc::downgrade(this);
                    let weak_feed = Rc::downgrade(&completed);
                    glib::idle_add_local_once(move || {
                        if let (Some(this), Some(feed)) = (weak.upgrade(), weak_feed.upgrade()) {
                            this.request_collection_index(&feed);
                        }
                    });
                }
            },
        );
    }

    pub(super) fn render_index_browsing_footer(self: &Rc<Self>, feed: &Feed) {
        // Other collections browse a page at a time when their search clears.
        let automatic = feed.auto_index.get();
        let index = feed.index.borrow();
        if index.running {
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_halign(gtk::Align::Center);
            feed.status.append(&spinner);
            return;
        }
        let (message, action) = if index.complete {
            if index
                .snapshot
                .as_ref()
                .is_some_and(CollectionSnapshot::is_truncated)
            {
                feed.status.append(&text(
                    "Достигнут лимит коллекции: 20 000 позиций.",
                    "dim-label",
                ));
            } else if automatic && feed.data.borrow().incomplete {
                feed.status
                    .append(&text("Часть треков недоступна.", "dim-label"));
            }
            return;
        } else if index.changed {
            (
                Some(if automatic {
                    "Библиотека изменилась"
                } else {
                    "Коллекция изменилась"
                }),
                "Обновить",
            )
        } else if index.error == Some(MusicError::ResponseTooLarge) {
            (Some("Показана ограниченная часть коллекции"), "")
        } else if index.error.is_some() {
            (Some("Не удалось загрузить следующие треки"), "Повторить")
        } else if automatic {
            (None, "Продолжить")
        } else {
            (None, "Загрузить ещё")
        };
        if let Some(message) = message {
            feed.status.append(&text(message, "dim-label"));
        }
        if !action.is_empty() {
            let button = gtk::Button::with_label(action);
            button.set_widget_name("music-collection-browse-more");
            button.set_halign(gtk::Align::Center);
            let weak = Rc::downgrade(self);
            let weak_feed = Rc::downgrade(feed);
            button.connect_clicked(move |_| {
                if let (Some(this), Some(feed)) = (weak.upgrade(), weak_feed.upgrade()) {
                    if feed.auto_index.get() && feed.restart_required.get() {
                        // A confirmed mutation retires the old provider page
                        // and its row actions, including a normalized index.
                        this.load_library_refresh(0);
                    } else {
                        this.start_collection_index(&feed, !feed.auto_index.get());
                    }
                }
            });
            feed.status.append(&button);
        }
    }

    pub(super) fn update_collection_index_bar(
        &self,
        feed: &Feed,
        bar: &collection_filter::FilterBar,
        data: &LoadedTracks,
    ) {
        let index = feed.index.borrow();
        let count = data.filter.visible().len();
        update_window_buttons(feed, bar, count);
        if !data.filter.active() {
            let waiting = self.waiting_for_liked_shuffle(feed, data);
            bar.set_status(
                if waiting {
                    "Для перемешивания нужна вся коллекция"
                } else {
                    ""
                },
                waiting && index.running,
                None,
            );
            return;
        }
        let total = index.snapshot.as_ref().map(CollectionSnapshot::len);
        let truncated = index
            .snapshot
            .as_ref()
            .is_some_and(CollectionSnapshot::is_truncated);
        let range = if count == 0 {
            String::new()
        } else {
            format!(
                " · {}–{}",
                feed.window_start.get() + 1,
                (feed.window_start.get() + feed.result_window()).min(count)
            )
        };
        let progress = match total {
            Some(total) => format!("Проверено {} из {total} позиций", index.cursor),
            None => "Получение состава коллекции".into(),
        };
        let (message, action) = if index.changed {
            (
                "Коллекция изменилась · Поиск неполный".into(),
                Some("Обновить"),
            )
        } else if index.error == Some(MusicError::ResponseTooLarge) {
            (
                format!("Лимит индекса: 20 000 позиций / 4 МБ · Найдено: {count}{range}"),
                None,
            )
        } else if index.error.is_some() {
            (
                format!("Поиск прерван · {progress} · Найдено: {count}{range}"),
                Some("Повторить"),
            )
        } else if index.complete && truncated {
            (
                format!("Достигнут лимит коллекции: 20 000 позиций · Найдено: {count}{range}"),
                None,
            )
        } else if index.complete {
            let result = if count == 0 && !data.incomplete {
                "Совпадений нет".into()
            } else {
                format!("Найдено: {count}{range}")
            };
            (
                format!(
                    "Вся коллекция проверена · {result}{}",
                    if data.incomplete {
                        " · Часть данных недоступна"
                    } else {
                        ""
                    }
                ),
                None,
            )
        } else if index.running {
            (
                format!("{progress} · Найдено: {count}{range}"),
                Some("Пауза"),
            )
        } else {
            (
                format!("{progress} · Найдено: {count}{range} · На паузе"),
                Some("Продолжить"),
            )
        };
        bar.set_status(&message, index.running, action);
    }
}
