// SPDX-License-Identifier: GPL-3.0-or-later
//! One panel queue view of the application policy, retaining physical identities.

use super::*;

/// An in-process GTK value, with no serializer or textual MIME representation.
/// A second window/account or retired presentation cannot consume the gesture.
#[derive(Clone, glib::Boxed)]
#[boxed_type(name = "KarinQueueOccurrenceDrag")]
struct QueueDrag {
    owner: glib::WeakRef<gtk::ListBox>,
    generation: u64,
    context: Option<PlaybackContext>,
    occurrence: crate::queue::OccurrenceId,
}

impl Controller {
    pub(super) fn render_queue(self: &Rc<Self>) {
        self.guard_music_binding();
        clear_list(&self.widgets.queue_rows);
        self.widgets.queue_rows.remove_css_class("boxed-list");
        self.widgets.queue_rows.add_css_class("music-track-list");
        let radio_playable = self.wave_current_allowed();
        let model = self.model.borrow();
        let restored = model
            .queue
            .context()
            .and_then(PlaybackContext::source_id)
            .is_some_and(|source| source.starts_with("restored:"));
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
                ContextKind::Wave => "Моя волна · эксперимент",
                ContextKind::Queue => "Ваша очередь",
            })
            .unwrap_or("Очередь");
        let partial = model
            .queue
            .context()
            .is_some_and(PlaybackContext::is_partial);
        self.widgets.queue_count.set_text(&format!(
            "{} · {}{}",
            if restored {
                "Сохранённая очередь"
            } else {
                source
            },
            track_count_text(model.queue.items().len() as u64),
            if restored {
                " · исходный раздел мог измениться"
            } else if partial {
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
        let information = selected
            .map(|track| {
                let artists = track.artists.join(", ");
                if artists.is_empty() {
                    format!("Сведения о треке: {}", track.title)
                } else {
                    format!("Сведения о треке: {} · {artists}", track.title)
                }
            })
            .unwrap_or_else(|| "Показать сведения о текущем треке".into());
        self.widgets
            .current_art_button
            .set_tooltip_text(Some(&information));
        self.widgets
            .current_art_button
            .update_property(&[gtk::accessible::Property::Label(&information)]);
        self.widgets.position.set_text(
            &selected
                .and_then(|track| track.duration_ms)
                .map(|duration| format!("— / {}", format_duration(duration)))
                .unwrap_or_else(|| "— / —".into()),
        );
        let requested_cover = selected.and_then(|track| track.cover.clone());
        if self.widgets.current_art.first_child().is_none()
            || self.current_art_cover.borrow().as_ref() != Some(&requested_cover)
        {
            // Future-queue edits and transport/mode updates do not invalidate
            // the current texture or its in-flight native artwork task.
            clear_box(&self.widgets.current_art);
            self.widgets.current_art.append(&cover_widget(
                self.artwork.as_ref(),
                requested_cover.as_ref(),
                56,
            ));
            *self.current_art_cover.borrow_mut() = Some(requested_cover);
        }
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
            .set_sensitive(model.queue.can_go_previous());
        self.widgets.next.set_sensitive(model.queue.can_go_next());
        self.widgets
            .clear_queue
            .set_sensitive(!model.queue.items().is_empty());
        let current_index = model.queue.current_index();
        let upcoming = model.queue.upcoming_indices();
        let editable_upcoming = model.queue.editable_upcoming();
        let history = model.queue.history_indices();
        let next_index = upcoming.first().copied();
        let repeat_one = model.queue.repeat_mode() == crate::queue::RepeatMode::One;
        self.widgets
            .queue_count
            .set_tooltip_text(Some(if repeat_one {
                "Повтор текущего трека. Следующие позиции — после явного перехода."
            } else {
                "Локальная очередь Karin в порядке воспроизведения"
            }));
        let order = display_order(model.queue.items().len(), current_index, &upcoming);
        let generation = model.generation;
        let queue_snapshot = model.queue.context().cloned();
        for index in order {
            let track = &model.queue.items()[index];
            let occurrence = model
                .queue
                .occurrence_id(index)
                .expect("admitted occurrence")
                .clone();
            let ordinary = model
                .queue
                .context()
                .is_some_and(|context| context.source_kind() != ContextKind::Wave);
            let upcoming_slot = editable_upcoming
                .iter()
                .position(|position| *position == index);
            let (row, body) = catalog_layout::catalog_item(&track.title, &track.artists.join(", "));
            body.set_spacing(4);
            if let Some(line) = body
                .first_child()
                .and_then(|labels| labels.first_child())
                .and_downcast::<gtk::Box>()
            {
                let cache = cache_presence::indicator("music-queue-cache");
                line.append(&cache);
                self.register_cache_indicator(&track.id, &cache);
            }
            // Even setting GtkActionable's target without an action-name
            // creates GtkActionHelper and bypasses ListBox::row-activated.
            // Keep the sole dispatcher above intact; tests use a presentation-
            // neutral locator that never changes native activation semantics.
            #[cfg(test)]
            row.add_css_class(&format!("music-queue-occurrence-{index}"));
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
                40,
            ));
            if ordinary && upcoming_slot.is_some() {
                let handle = gtk::Image::from_icon_name("list-drag-handle-symbolic");
                handle.set_widget_name("music-queue-drag-handle");
                handle.add_css_class("dim-label");
                handle.set_tooltip_text(Some("Перетащите трек; с клавиатуры — Alt+↑ или Alt+↓"));
                handle.set_cursor_from_name(Some("grab"));
                body.prepend(&handle);
            }
            let caption = queue_badge(index, current_index, next_index)
                .map(|caption| {
                    if caption == "Далее" && repeat_one {
                        "После пропуска"
                    } else {
                        caption
                    }
                })
                .or_else(|| {
                    (current_index != Some(index) && history.contains(&index)).then_some("Ранее")
                });
            if let Some(caption) = caption {
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
            let play = icon_button("media-playback-start-symbolic", "Воспроизвести трек");
            play.set_widget_name("music-queue-play");
            play.add_css_class("flat");
            play.set_sensitive(track.available && radio_playable);
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
            let information = track.clone();
            let context = model
                .queue
                .context()
                .and_then(|context| context.with_current_index(index));
            let expected = queue_snapshot.clone();
            details.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade()
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.select_track_info(information.clone(), context.clone(), false);
                    this.set_info_open(true);
                }
            });

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
            let menu = gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .tooltip_text("Действия с позицией очереди")
                .build();
            menu.set_widget_name("music-queue-more");
            menu.add_css_class("flat");
            menu.update_property(&[gtk::accessible::Property::Label(
                "Действия с позицией очереди",
            )]);
            let add_playlist = gtk::Button::with_label("Добавить в плейлист");
            add_playlist.set_widget_name("music-queue-add-playlist");
            add_playlist.add_css_class("flat");
            let weak = Rc::downgrade(self);
            let selected = track.clone();
            let expected = queue_snapshot.clone();
            add_playlist.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade()
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.present_add_to_playlist(selected.clone());
                }
            });
            let actions = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let next = gtk::Button::with_label("Воспроизвести следующим");
            next.set_widget_name("music-queue-play-next");
            next.add_css_class("flat");
            next.set_sensitive(ordinary && track.available && current_index != Some(index));
            let weak = Rc::downgrade(self);
            let expected = queue_snapshot.clone();
            let selected = occurrence.clone();
            next.connect_clicked(move |_| {
                if let Some(this) = weak.upgrade()
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.promote_queue_occurrence(&selected);
                }
            });
            let up = gtk::Button::with_label("Поднять выше · Alt+↑");
            up.set_widget_name("music-queue-move-up");
            up.add_css_class("flat");
            up.set_sensitive(ordinary && upcoming_slot.is_some_and(|slot| slot > 0));
            let down = gtk::Button::with_label("Опустить ниже · Alt+↓");
            down.set_widget_name("music-queue-move-down");
            down.add_css_class("flat");
            down.set_sensitive(
                ordinary && upcoming_slot.is_some_and(|slot| slot + 1 < editable_upcoming.len()),
            );
            for (button, delta) in [(&up, -1), (&down, 1)] {
                let weak = Rc::downgrade(self);
                let expected = queue_snapshot.clone();
                let selected = occurrence.clone();
                button.connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade()
                        && queue_action_is_current(&this, generation, &expected)
                    {
                        this.shift_queue_occurrence(&selected, delta);
                    }
                });
            }
            actions.append(&next);
            actions.append(&up);
            actions.append(&down);
            actions.append(&details);
            actions.append(&add_playlist);
            actions.append(&remove);
            let popover = gtk::Popover::new();
            popover.set_child(Some(&actions));
            menu.set_popover(Some(&popover));
            for button in [&next, &up, &down, &details, &add_playlist, &remove] {
                let weak = popover.downgrade();
                button.connect_clicked(move |_| {
                    if let Some(popover) = weak.upgrade() {
                        popover.popdown();
                    }
                });
            }
            body.append(&menu);
            if ordinary && upcoming_slot.is_some() {
                let source = gtk::DragSource::new();
                source.set_actions(gtk::gdk::DragAction::MOVE);
                let payload = QueueDrag {
                    owner: self.widgets.queue_rows.downgrade(),
                    generation,
                    context: queue_snapshot.clone(),
                    occurrence: occurrence.clone(),
                };
                let weak = Rc::downgrade(self);
                let anchor = row.downgrade();
                source.connect_prepare(move |_, _, _| {
                    let this = weak.upgrade()?;
                    if !anchor.upgrade().is_some_and(|row| row.is_mapped())
                        || !queue_action_is_current(&this, payload.generation, &payload.context)
                    {
                        return None;
                    }
                    Some(gtk::gdk::ContentProvider::for_value(&payload.to_value()))
                });
                row.add_controller(source);
                let target =
                    gtk::DropTarget::new(QueueDrag::static_type(), gtk::gdk::DragAction::MOVE);
                let weak = Rc::downgrade(self);
                let selected = occurrence.clone();
                let expected = queue_snapshot.clone();
                let anchor = row.downgrade();
                target.connect_drop(move |_, value, _, y| {
                    let (Some(this), Some(row), Ok(payload)) =
                        (weak.upgrade(), anchor.upgrade(), value.get::<QueueDrag>())
                    else {
                        return false;
                    };
                    if !row.is_mapped()
                        || payload.owner.upgrade().as_ref() != Some(&this.widgets.queue_rows)
                        || !queue_action_is_current(&this, generation, &expected)
                        || !queue_action_is_current(&this, payload.generation, &payload.context)
                    {
                        return false;
                    }
                    this.move_queue_occurrence(
                        &payload.occurrence,
                        &selected,
                        y >= f64::from(row.height()) / 2.0,
                    )
                });
                row.add_controller(target);
            }
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(self);
            let expected = queue_snapshot.clone();
            let selected = occurrence.clone();
            let anchor = row.downgrade();
            keys.connect_key_pressed(move |_, key, _, modifiers| {
                let delta = match key {
                    gtk::gdk::Key::Up => -1,
                    gtk::gdk::Key::Down => 1,
                    _ => return glib::Propagation::Proceed,
                };
                if modifiers.contains(gtk::gdk::ModifierType::ALT_MASK)
                    && let Some(this) = weak.upgrade()
                    && anchor.upgrade().is_some_and(|row| row.is_mapped())
                    && queue_action_is_current(&this, generation, &expected)
                {
                    this.shift_queue_occurrence(&selected, delta);
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                }
            });
            row.add_controller(keys);
            self.row_action(&row, RowAction::Queue(index));
            row.set_activatable(track.available && radio_playable);
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
            row.set_tooltip_text(Some(if !radio_playable {
                "Радиосессия завершена. Запустите новую волну на Главной."
            } else if track.available {
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
        self.sync_current_like();
        self.render_music_player();
    }

    fn finish_queue_edit(self: &Rc<Self>, occurrence: &crate::queue::OccurrenceId) {
        // Index changes alone must not stop/reprepare the currently bound engine.
        self.note_playback_intent();
        self.cancel_pending_wave_for_ordinary_action();
        self.observe_current_occurrence();
        self.render_queue();
        self.publish_mpris();
        let display = {
            let model = self.model.borrow();
            model.queue.occurrence_index(occurrence).and_then(|index| {
                let order = display_order(
                    model.queue.items().len(),
                    model.queue.current_index(),
                    &model.queue.upcoming_indices(),
                );
                order.iter().position(|position| *position == index)
            })
        };
        if let Some(display) = display
            && let Some(row) = self.widgets.queue_rows.row_at_index(display as i32)
        {
            row.grab_focus();
        }
    }

    pub(super) fn move_queue_occurrence(
        self: &Rc<Self>,
        occurrence: &crate::queue::OccurrenceId,
        target: &crate::queue::OccurrenceId,
        after: bool,
    ) -> bool {
        if self.model.borrow().closed
            || !self.model.borrow().account_valid
            || !self.wave_manual_append_allowed()
        {
            return false;
        }
        let changed = self
            .model
            .borrow_mut()
            .queue
            .move_upcoming(occurrence, target, after);
        if changed {
            self.finish_queue_edit(occurrence);
        }
        changed
    }

    pub(super) fn shift_queue_occurrence(
        self: &Rc<Self>,
        occurrence: &crate::queue::OccurrenceId,
        delta: i32,
    ) -> bool {
        let target = {
            let model = self.model.borrow();
            let Some(index) = model.queue.occurrence_index(occurrence) else {
                return false;
            };
            let future = model.queue.editable_upcoming();
            let Some(slot) = future.iter().position(|position| *position == index) else {
                return false;
            };
            let Some(target) = slot
                .checked_add_signed(delta as isize)
                .and_then(|slot| future.get(slot))
            else {
                return false;
            };
            model
                .queue
                .occurrence_id(*target)
                .expect("admitted future")
                .clone()
        };
        self.move_queue_occurrence(occurrence, &target, delta > 0)
    }

    pub(super) fn promote_queue_occurrence(
        self: &Rc<Self>,
        occurrence: &crate::queue::OccurrenceId,
    ) -> bool {
        if self.model.borrow().closed
            || !self.model.borrow().account_valid
            || !self.wave_manual_append_allowed()
        {
            return false;
        }
        let changed = self.model.borrow_mut().queue.move_next(occurrence);
        if changed {
            self.finish_queue_edit(occurrence);
        }
        changed
    }

    /// Reflect a retired radio identity without rebuilding the queue on every
    /// snapshot. Keep Details/remove available; only playback is unsupported.
    pub(super) fn sync_wave_queue_controls(&self) {
        if !self.is_wave_queue() || self.wave_current_allowed() {
            return;
        }
        let guidance = "Радиосессия завершена. Запустите новую волну на Главной.";
        let mut child = self.widgets.queue_rows.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            let Some(row) = widget.downcast_ref::<gtk::ListBoxRow>() else {
                continue;
            };
            if !row.has_css_class("music-queue-row") {
                continue;
            }
            row.set_activatable(false);
            row.set_tooltip_text(Some(guidance));
            let mut action = row.child().and_then(|body| body.first_child());
            while let Some(widget) = action {
                action = widget.next_sibling();
                if widget.widget_name() == "music-queue-play" {
                    widget.set_sensitive(false);
                    widget.set_tooltip_text(Some(guidance));
                }
            }
        }
    }

    #[cfg(test)]
    pub(super) fn queue_row_at_physical(&self, index: usize) -> Option<gtk::ListBoxRow> {
        if index >= 500 {
            return None;
        }
        let locator = format!("music-queue-occurrence-{index}");
        let mut child = self.widgets.queue_rows.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            if let Ok(row) = widget.downcast::<gtk::ListBoxRow>()
                && row.has_css_class(&locator)
            {
                return Some(row);
            }
        }
        None
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
        && match (model.queue.context(), expected.as_ref()) {
            (None, None) => true,
            (Some(current), Some(expected)) => current.same_presentation_snapshot(expected),
            _ => false,
        }
}

/// Keep each physical position once. A repeat-cycle preview may include the
/// current position again; it already has its own current row. Invalid indices
/// are ignored defensively without inventing new queue entries.
fn display_order(len: usize, current: Option<usize>, upcoming: &[usize]) -> Vec<usize> {
    let mut seen = vec![false; len];
    current
        .into_iter()
        .chain(upcoming.iter().copied())
        .chain(0..len)
        .filter(|index| {
            seen.get_mut(*index).is_some_and(|seen| {
                let fresh = !*seen;
                *seen = true;
                fresh
            })
        })
        .collect()
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
