// SPDX-License-Identifier: GPL-3.0-or-later
//! Queue presentation only; order, selection and playback remain unchanged.

use super::*;

impl Controller {
    pub(super) fn render_queue(self: &Rc<Self>) {
        self.guard_music_binding();
        clear_list(&self.widgets.queue_rows);
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
        for (index, track) in model.queue.items().iter().enumerate() {
            let prefix = queue_prefix(index, current_index, next_index, track.available);
            let (row, body) = item(
                &track.title,
                &format!("{prefix}{}", track.artists.join(", ")),
            );
            if current_index == Some(index) {
                row.set_widget_name("music-queue-current");
                row.add_css_class("accent");
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
                40,
            ));
            let play = icon_button("media-playback-start-symbolic", "Воспроизвести трек");
            play.set_sensitive(track.available);
            let weak = Rc::downgrade(self);
            play.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.play_music_queue(index);
                }
            });
            play.add_css_class("suggested-action");
            body.append(&play);
            let details = icon_button("dialog-information-symbolic", "Сведения о треке");
            let weak = Rc::downgrade(self);
            let id = track.id.clone();
            let context = model
                .queue
                .context()
                .and_then(|context| context.with_current_index(index));
            details.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.open_track_context(id.clone(), context.clone());
                }
            });
            body.append(&details);
            let remove = icon_button("list-remove-symbolic", "Удалить трек из очереди");
            let weak = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade() {
                    this.remove_music_queue(index);
                }
            });
            body.append(&remove);
            self.row_action(&row, RowAction::Queue(index));
            self.widgets.queue_rows.append(&row);
        }
        if model.queue.items().is_empty() {
            set_empty(
                &self.widgets.queue_rows,
                "Добавляйте треки из библиотеки, поиска или альбома.",
            );
        }
        drop(model);
        self.render_music_player();
    }
}

fn next_available_index(items: &[Track], current: Option<usize>) -> Option<usize> {
    let start = current?.checked_add(1)?;
    items
        .get(start..)?
        .iter()
        .position(|track| track.available)
        .map(|offset| start + offset)
}

fn queue_prefix(
    index: usize,
    current: Option<usize>,
    next: Option<usize>,
    available: bool,
) -> &'static str {
    if current == Some(index) {
        if available {
            "Текущий · "
        } else {
            "Текущий · Недоступен · "
        }
    } else if next == Some(index) {
        "Далее · "
    } else if !available {
        "Недоступен · "
    } else {
        ""
    }
}

#[cfg(test)]
mod tests;
