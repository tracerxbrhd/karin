// SPDX-License-Identifier: GPL-3.0-or-later
//! Queue presentation only; order, selection and playback remain unchanged.

use super::*;

impl Controller {
    pub(super) fn render_queue(self: &Rc<Self>) {
        self.guard_music_binding();
        clear_list(&self.widgets.queue_rows);
        self.widgets.queue_rows.remove_css_class("boxed-list");
        self.widgets.queue_rows.add_css_class("music-track-list");
        let model = self.model.borrow();
        let source = model
            .queue
            .context()
            .map(|context| match context.source_kind() {
                ContextKind::LikedTracks => "Понравившиеся треки",
                ContextKind::Album => "Альбом",
                ContextKind::Playlist => "Плейлист",
                ContextKind::Search => "Результаты поиска",
                ContextKind::Artist => "Исполнитель",
                ContextKind::SingleTrack => "Выбранный трек",
                ContextKind::Queue => "Ваша очередь",
            })
            .unwrap_or("Очередь");
        let partial = model
            .queue
            .context()
            .is_some_and(PlaybackContext::is_partial);
        self.widgets.queue_count.set_text(&format!(
            "{source} · {}{}",
            track_count_text(model.queue.items().len() as u64),
            if partial {
                " · загруженная часть"
            } else {
                ""
            }
        ));
        let current = model
            .queue
            .current()
            .map(|track| track.title.as_str())
            .unwrap_or("Трек не выбран");
        self.widgets.current.set_text(current);
        let selected = model.queue.current();
        self.widgets.current_artist.set_text(
            &selected
                .map(|track| track.artists.join(", "))
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| {
                    if selected.is_some() {
                        "Исполнитель не указан"
                    } else {
                        "Выберите трек"
                    }
                    .into()
                }),
        );
        self.widgets
            .current_artist
            .set_tooltip_text(Some(&self.widgets.current_artist.text()));
        self.widgets
            .current_artist
            .update_property(&[gtk::accessible::Property::Label(
                &self.widgets.current_artist.text(),
            )]);
        self.widgets.position.set_text(
            &selected
                .and_then(|track| track.duration_ms)
                .map(|duration| format!("— / {}", format_duration(duration)))
                .unwrap_or_else(|| "— / —".into()),
        );
        clear_box(&self.widgets.current_art);
        self.widgets.current_art.append(&cover_widget(
            self.artwork.as_ref(),
            selected.and_then(|track| track.cover.as_ref()),
            56,
        ));
        self.widgets
            .home_queue
            .set_text(&if model.queue.items().is_empty() {
                "Очередь пуста".into()
            } else {
                format!(
                    "В очереди: {}",
                    track_count_text(model.queue.items().len() as u64)
                )
            });
        self.widgets.current.set_tooltip_text(Some(current));
        self.widgets
            .current
            .update_property(&[gtk::accessible::Property::Label(current)]);
        self.widgets
            .previous
            .set_sensitive(model.queue.current_index().is_some_and(|i| i > 0));
        self.widgets.next.set_sensitive(
            model
                .queue
                .current_index()
                .is_some_and(|i| i + 1 < model.queue.items().len()),
        );
        self.widgets
            .clear_queue
            .set_sensitive(!model.queue.items().is_empty());
        let current_index = model.queue.current_index();
        let next_index = next_available_index(model.queue.items(), current_index);
        let generation = model.generation;
        let queue_snapshot = model.queue.context().cloned();
        for (index, track) in model.queue.items().iter().enumerate() {
            let (row, body) = catalog_layout::catalog_item(&track.title, &track.artists.join(", "));
            body.set_spacing(8);
            let labels = body
                .first_child()
                .and_downcast::<gtk::Box>()
                .expect("catalog row labels");
            // Status belongs beside the quiet artist line, leaving the title
            // and three native actions usable even in a compact queue.
            let credits = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            if let Some(artist) = labels
                .last_child()
                .filter(|child| child.has_css_class("music-track-artist"))
            {
                labels.remove(&artist);
                artist.set_hexpand(true);
                credits.append(&artist);
            }
            row.add_css_class("music-queue-row");
            if current_index == Some(index) {
                row.set_widget_name("music-queue-current");
                row.add_css_class("music-current-row");
            } else if next_index == Some(index) {
                row.set_widget_name("music-queue-next");
            } else if !track.available {
                row.set_widget_name("music-queue-unavailable");
            } else {
                row.set_widget_name("music-queue-track");
            }
            body.prepend(&cover_widget(
                self.artwork.as_ref(),
                track.cover.as_ref(),
                48,
            ));
            if let Some(caption) = queue_badge(index, current_index, next_index) {
                let badge = gtk::Label::new(Some(caption));
                badge.add_css_class("caption");
                badge.add_css_class("music-row-badge");
                badge.set_valign(gtk::Align::Center);
                badge.set_widget_name(if current_index == Some(index) {
                    "music-queue-current-badge"
                } else {
                    "music-queue-next-badge"
                });
                if current_index == Some(index) {
                    badge.add_css_class("accent");
                } else {
                    badge.add_css_class("dim-label");
                }
                credits.append(&badge);
            }
            if !track.available {
                let unavailable = gtk::Image::from_icon_name("action-unavailable-symbolic");
                unavailable.set_widget_name("music-queue-unavailable-mark");
                unavailable.add_css_class("dim-label");
                unavailable.set_tooltip_text(Some("Трек недоступен"));
                unavailable.update_property(&[gtk::accessible::Property::Label("Трек недоступен")]);
                credits.append(&unavailable);
            }
            if credits.first_child().is_some() {
                labels.append(&credits);
            }
            if let Some(duration) = track.duration_ms {
                let duration = gtk::Label::new(Some(&format_duration(duration)));
                duration.add_css_class("caption");
                duration.add_css_class("dim-label");
                duration.add_css_class("music-track-duration");
                duration.set_valign(gtk::Align::Center);
                body.append(&duration);
            }
            let play = icon_button("media-playback-start-symbolic", "Воспроизвести трек");
            play.set_widget_name("music-queue-play");
            play.add_css_class("flat");
            play.set_sensitive(track.available);
            let weak = Rc::downgrade(self);
            let expected = queue_snapshot.clone();
            play.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade()
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.play_music_queue(index);
                }
            });
            body.append(&play);
            let details = icon_button("dialog-information-symbolic", "Сведения о треке");
            details.set_widget_name("music-queue-details");
            details.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let id = track.id.clone();
            let context = model
                .queue
                .context()
                .and_then(|context| context.with_current_index(index));
            let expected = queue_snapshot.clone();
            details.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade()
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.open_track_context(id.clone(), context.clone());
                }
            });
            body.append(&details);
            let remove = icon_button("list-remove-symbolic", "Удалить трек из очереди");
            remove.set_widget_name("music-queue-remove");
            remove.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let expected = queue_snapshot.clone();
            remove.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade()
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.remove_music_queue(index);
                }
            });
            body.append(&remove);
            self.row_action(&row, RowAction::Queue(index));
            row.set_activatable(track.available);
            let state = queue_badge(index, current_index, next_index)
                .map(|caption| format!("{caption} · "))
                .unwrap_or_default();
            let artists = if track.artists.is_empty() {
                String::new()
            } else {
                format!(" · {}", track.artists.join(", "))
            };
            row.update_property(&[gtk::accessible::Property::Label(&format!(
                "{state}{}{artists} · Позиция {} из {}{}",
                track.title,
                index + 1,
                model.queue.items().len(),
                if track.available {
                    ""
                } else {
                    " · Недоступен"
                },
            ))]);
            row.set_tooltip_text(Some(if track.available {
                "Воспроизвести трек"
            } else {
                "Трек недоступен"
            }));
            self.widgets.queue_rows.append(&row);
        }
        if model.queue.items().is_empty() {
            set_empty(&self.widgets.queue_rows, "Очередь пуста");
        }
        drop(model);
        self.render_music_player();
    }
}

fn queue_action_is_current(
    this: &Controller,
    generation: u64,
    expected: &Option<PlaybackContext>,
) -> bool {
    let model = this.model.borrow();
    !model.closed
        && model.account_valid
        && model.generation == generation
        && model.queue.context() == expected.as_ref()
}

fn next_available_index(items: &[Track], current: Option<usize>) -> Option<usize> {
    let start = current?.checked_add(1)?;
    items
        .get(start..)?
        .iter()
        .position(|track| track.available)
        .map(|offset| start + offset)
}

fn queue_badge(index: usize, current: Option<usize>, next: Option<usize>) -> Option<&'static str> {
    if current == Some(index) {
        Some("Сейчас")
    } else if next == Some(index) {
        Some("Далее")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
