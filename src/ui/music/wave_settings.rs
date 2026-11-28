// SPDX-License-Identifier: GPL-3.0-or-later
//! An explicit personal Wave start with choices advertised by the service.

use super::*;
use crate::music::wave::{WaveSettingGroup, WaveTuning};

const DIALOG_NAME: &str = "music-wave-settings-dialog";

pub(super) struct PendingRead {
    generation: u64,
    dialog: glib::WeakRef<adw::Dialog>,
    body: glib::WeakRef<gtk::Box>,
}

impl Controller {
    pub(super) fn connect_wave_settings(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.widgets.home_wave.settings.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.open_wave_settings();
            }
        });
    }

    fn open_wave_settings(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if !self.radio_status().authenticated || window.visible_dialog().is_some() {
            return;
        }

        let dialog = adw::Dialog::builder()
            .title("Настройка «Моей волны»")
            .content_width(420)
            .content_height(340)
            .build();
        dialog.set_widget_name(DIALOG_NAME);
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
        super::super::margins(&body, 18);
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&body)
            .build();
        toolbar.set_content(Some(&scroll));
        dialog.set_child(Some(&toolbar));
        dialog.present(Some(&window));
        self.load_wave_settings(&dialog, &body);
    }

    pub(super) fn retire_wave_settings(&self) {
        self.wave_settings_read.borrow_mut().take();
        if let Some(dialog) = self
            .window
            .upgrade()
            .and_then(|window| window.visible_dialog())
            .filter(|dialog| dialog.widget_name() == DIALOG_NAME)
        {
            dialog.close();
        }
    }

    fn load_wave_settings(self: &Rc<Self>, dialog: &adw::Dialog, body: &gtk::Box) {
        let Some(service) = self.service.clone() else {
            return;
        };
        let (generation, token) = {
            let model = self.model.borrow();
            let Some(token) = model
                .token
                .clone()
                .filter(|_| model.account_valid && !model.closed)
            else {
                return;
            };
            (model.generation, token)
        };
        clear_box(body);
        body.append(&message("Загружаем настройки…"));
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_halign(gtk::Align::Start);
        body.append(&spinner);

        let read = PendingRead {
            generation,
            dialog: dialog.downgrade(),
            body: body.downgrade(),
        };
        let previous = self.wave_settings_read.replace(Some(read));
        if previous.is_some_and(|read| read.generation == generation) {
            return;
        }

        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let response = gio::spawn_blocking(move || service.personal_wave_settings(&token))
                .await
                .unwrap_or(Err(MusicError::Configuration));
            let Some(this) = weak.upgrade() else {
                return;
            };
            let Some(read) = this
                .wave_settings_read
                .borrow_mut()
                .take_if(|read| read.generation == generation)
            else {
                return;
            };
            let (Some(dialog), Some(body)) = (read.dialog.upgrade(), read.body.upgrade()) else {
                return;
            };
            if !dialog.is_mapped() {
                return;
            }
            if this.model.borrow().generation != generation {
                dialog.close();
                return;
            }
            clear_box(&body);
            match response {
                Ok(groups) if groups.is_empty() => {
                    body.append(&message("Сервис не предложил настройки для «Моей волны»."));
                }
                Ok(groups) => this.render_wave_settings(&dialog, &body, generation, groups),
                Err(error) => {
                    let (_, detail) = music_error_message(error);
                    body.append(&message(detail));
                    let retry = gtk::Button::with_label("Повторить");
                    retry.set_halign(gtk::Align::Start);
                    body.append(&retry);
                    let (weak, dialog, body) =
                        (Rc::downgrade(&this), dialog.downgrade(), body.downgrade());
                    retry.connect_clicked(move |_| {
                        if let (Some(this), Some(dialog), Some(body)) =
                            (weak.upgrade(), dialog.upgrade(), body.upgrade())
                        {
                            this.load_wave_settings(&dialog, &body);
                        }
                    });
                }
            }
        });
    }

    fn render_wave_settings(
        self: &Rc<Self>,
        dialog: &adw::Dialog,
        body: &gtk::Box,
        generation: u64,
        groups: Vec<WaveSettingGroup>,
    ) {
        body.append(&message("Выберите параметры новой сессии «Моей волны»."));
        let form = adw::PreferencesGroup::new();
        let previous = self.personal_wave_tuning();
        let choices: Vec<_> = groups
            .into_iter()
            .map(|group| {
                let labels: Vec<_> = group.labels().collect();
                let row = adw::ComboRow::builder()
                    .title(group.label())
                    .model(&gtk::StringList::new(&labels))
                    .selected(previous.selection(&group))
                    .build();
                form.add(&row);
                (group, row)
            })
            .collect();
        body.append(&form);
        let play = gtk::Button::with_label("Слушать");
        play.set_widget_name("music-wave-settings-play");
        play.add_css_class("suggested-action");
        play.set_halign(gtk::Align::Start);
        body.append(&play);

        let (weak, dialog) = (Rc::downgrade(self), dialog.downgrade());
        play.connect_clicked(move |_| {
            let (Some(this), Some(dialog)) = (weak.upgrade(), dialog.upgrade()) else {
                return;
            };
            if this.model.borrow().generation != generation {
                dialog.close();
                return;
            }
            let mut tuning = WaveTuning::default();
            for (group, row) in &choices {
                tuning.choose(group, row.selected() as usize);
            }
            let source = tuning.into_source();
            dialog.close();
            this.listen_to_configured_wave(source);
        });
    }
}

fn message(value: &str) -> gtk::Label {
    let label = text(value, "dim-label");
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label
}
