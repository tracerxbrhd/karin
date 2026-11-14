// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicit incremental whole-collection loading with a bounded native result
//! window. No timer or SearchEntry change starts an HTTP request.

use super::*;

pub(super) fn sort_choice(choice: u32) -> collection_filter::CollectionSort {
    use collection_filter::CollectionSort;
    match choice {
        1 => CollectionSort::ReverseProvider,
        2 => CollectionSort::Title,
        3 => CollectionSort::Artist,
        4 => CollectionSort::AddedNewest,
        5 => CollectionSort::AddedOldest,
        _ => CollectionSort::Provider,
    }
}

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
pub(super) fn update_window_buttons(feed: &Feed, bar: &collection_filter::FilterBar, count: usize) {
    let windowed = feed.windowed.get();
    bar.previous
        .set_visible(windowed && feed.window_start.get() > 0);
    bar.next
        .set_visible(windowed && feed.window_start.get() + RESULT_WINDOW < count);
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
        let start = feed
            .window_start
            .get()
            .min(count.saturating_sub(1) / RESULT_WINDOW * RESULT_WINDOW);
        feed.window_start.set(start);
        let positions = data
            .filter
            .visible()
            .iter()
            .skip(start)
            .take(RESULT_WINDOW)
            .copied()
            .collect::<Vec<_>>();
        drop(data);
        if *feed.rendered_positions.borrow() == positions
            && (!positions.is_empty() || feed.rows.first_child().is_none())
        {
            return;
        }
        // Only the 200-row projection is mounted. The complete normalized index
        // and the immutable 500-item playback snapshot have independent owners.
        clear_list(&feed.rows);
        for index in &positions {
            self.append_feed_row(feed, *index);
        }
        feed.rendered_positions.replace(positions);
    }

    pub(super) fn pause_collection_index(self: &Rc<Self>, feed: &Feed) {
        {
            let mut index = feed.index.borrow_mut();
            index.running = false;
            index.busy = false;
            index.serial = index.serial.wrapping_add(1);
        }
        self.requests
            .borrow_mut()
            .invalidate(Operation::CollectionIndex);
        self.render_feed_status(feed);
    }

    pub(super) fn start_collection_index(self: &Rc<Self>, feed: &Feed) {
        if !self.feed_is_current(feed) || source(feed).is_none() || feed.index.borrow().running {
            return;
        }
        // One indexing lane; a new collection retires the preceding intent.
        for other in self.track_feeds.borrow().clone() {
            if !Rc::ptr_eq(&other, feed) && other.index.borrow().running {
                self.pause_collection_index(&other);
            }
        }
        let mut index = feed.index.borrow_mut();
        if index.complete {
            return;
        }
        if index.changed {
            *index = CollectionIndex {
                serial: index.serial.wrapping_add(1),
                ..Default::default()
            };
        }
        index.running = true;
        index.error = None;
        index.serial = index.serial.wrapping_add(1);
        drop(index);
        // No outstanding old prefix request may append to the new manifest.
        self.interrupt_feed_reads(match feed.source {
            FeedSource::Liked => Operation::Library,
            _ => Operation::Detail,
        });
        self.requests.borrow_mut().invalidate(match feed.source {
            FeedSource::Liked => Operation::Library,
            _ => Operation::Detail,
        });
        feed.data.borrow_mut().loading = false;
        feed.data.borrow_mut().error = None;
        feed.data.borrow_mut().indexed = true;
        feed.windowed.set(true);
        self.update_index_sort_options(feed);
        // The initial window already exists, but its physical positions may
        // differ from a filtered view; rebuild it once on entering this mode.
        feed.rendered_positions.borrow_mut().clear();
        self.render_collection_window(feed);
        self.render_feed_status(feed);
        self.request_collection_index(feed);
    }

    fn request_collection_index(self: &Rc<Self>, feed: &Feed) {
        if !self.feed_is_current(feed) {
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
                match result {
                    Ok(IndexReply::Manifest(snapshot)) => {
                        completed.restart_required.set(false);
                        let mut data = completed.data.borrow_mut();
                        let prefix = snapshot.matches_prefix(&data.items);
                        let cursor = if prefix { data.items.len() } else { 0 };
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
                            data.contexts.borrow_mut().clear();
                            completed.rendered_positions.borrow_mut().clear();
                            clear_list(&completed.rows);
                        }
                        data.filter = collection_filter::CollectionFilter::default();
                        if let Some(bar) = &completed.filter_ui {
                            data.filter.set_query(&bar.search.text());
                            data.filter.set_sort(sort_choice(bar.sort.selected()));
                        }
                        let dates = data
                            .positions
                            .iter()
                            .map(|position| snapshot.added_at(*position))
                            .collect::<Vec<_>>();
                        let LoadedTracks { items, filter, .. } = &mut *data;
                        filter.append_dated(items, &dates);
                        data.has_more = cursor < snapshot.len();
                        data.incomplete = false;
                        let mut index = completed.index.borrow_mut();
                        index.cursor = cursor;
                        index.snapshot = Some(snapshot);
                    }
                    Ok(IndexReply::Batch(batch)) => {
                        let mut data = completed.data.borrow_mut();
                        let snapshot = completed.index.borrow().snapshot.clone();
                        data.append_indexed(&batch, snapshot.as_ref());
                        let mut index = completed.index.borrow_mut();
                        index.cursor = batch.next_position;
                        data.has_more = index
                            .snapshot
                            .as_ref()
                            .is_some_and(|snapshot| index.cursor < snapshot.len());
                    }
                    Ok(IndexReply::Verified(same)) => {
                        let mut index = completed.index.borrow_mut();
                        index.complete = same;
                        index.changed = !same;
                        index.running = false;
                        index.error = (!same).then_some(MusicError::InvalidResponse);
                        completed.data.borrow_mut().has_more = !same;
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
                this.update_index_sort_options(&completed);
                this.render_collection_window(&completed);
                this.render_feed_status(&completed);
                this.sync_catalog_rows();
                if completed.index.borrow().running {
                    // Yield to native input and paint between bounded batches.
                    // No artificial delay and no HTTP scheduled by typing.
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

    fn update_index_sort_options(&self, feed: &Feed) {
        let Some(bar) = &feed.filter_ui else {
            return;
        };
        let dates = feed
            .index
            .borrow()
            .snapshot
            .as_ref()
            .is_some_and(CollectionSnapshot::has_added_dates);
        let choices = [
            "Порядок коллекции",
            "Обратный порядок",
            "По названию",
            "По исполнителю",
            "Добавлены позже",
            "Добавлены раньше",
        ];
        let count = if dates { 6 } else { 4 };
        if bar
            .sort
            .model()
            .is_none_or(|model| model.n_items() != count)
        {
            let selected = if bar.sort.selected() < count {
                bar.sort.selected()
            } else {
                0
            };
            bar.sort
                .set_model(Some(&gtk::StringList::new(&choices[..count as usize])));
            bar.sort.set_selected(selected);
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
        let total = index.snapshot.as_ref().map(CollectionSnapshot::len);
        let range = if count == 0 {
            String::new()
        } else {
            format!(
                " · {}–{}",
                feed.window_start.get() + 1,
                (feed.window_start.get() + RESULT_WINDOW).min(count)
            )
        };
        let progress = match total {
            Some(total) => format!("Проверено {} из {total} позиций", index.cursor),
            None => "Получение состава коллекции".into(),
        };
        bar.status.set_text(&if index.changed {
            "Коллекция изменилась. Обновите индекс; текущая очередь сохранена.".into()
        } else if index.error == Some(MusicError::ResponseTooLarge) {
            "Индекс ограничен 20 000 позиций и 4 МБ состава. Полный поиск не выполнен.".into()
        } else if index.error.is_some() {
            format!("Не удалось продолжить загрузку. {progress} · Найдено: {count}{range}")
        } else if index.complete {
            format!(
                "Вся коллекция проверена · Найдено: {count}{range}{}",
                if data.incomplete {
                    " · Часть данных недоступна"
                } else {
                    ""
                }
            )
        } else {
            format!(
                "{progress} · Найдено: {count}{range}{}",
                if index.running {
                    ""
                } else {
                    " · Загрузка приостановлена"
                }
            )
        });
        bar.status.set_visible(true);
        bar.more.set_visible(false);
        bar.index_all.set_label(if index.changed {
            "Обновить индекс"
        } else if index.error.is_some() {
            "Повторить загрузку"
        } else {
            "Продолжить загрузку всей коллекции"
        });
        bar.index_all.set_visible(!index.running && !index.complete);
        bar.index_all.set_sensitive(self.feed_is_current(feed));
        bar.cancel_index.set_visible(index.running);
        update_window_buttons(feed, bar, count);
    }
}
