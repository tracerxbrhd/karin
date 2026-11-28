// SPDX-License-Identifier: GPL-3.0-or-later

use adw::prelude::*;
use gtk::{gdk, gio, glib};

pub(super) fn install_transport(
    window: &adw::ApplicationWindow,
    play: &gtk::Button,
    previous: &gtk::Button,
    next: &gtk::Button,
    alternate_visible: impl Fn() -> bool + 'static,
) {
    let alternate_visible = std::rc::Rc::new(alternate_visible);
    for (name, button, accelerators) in [
        ("music-play-pause", play, &[][..]),
        ("music-previous", previous, &["<Primary><Shift>p"][..]),
        ("music-next", next, &["<Primary><Shift>n"][..]),
    ] {
        let action = gio::SimpleAction::new(name, None);
        button
            .bind_property("sensitive", &action, "enabled")
            .sync_create()
            .build();
        let button = button.downgrade();
        let window_ref = window.downgrade();
        let alternate_visible = alternate_visible.clone();
        action.connect_activate(move |_, _| {
            if let (Some(window), Some(button)) = (window_ref.upgrade(), button.upgrade())
                && window.visible_dialog().is_none()
                && button.is_sensitive()
                && (button.is_mapped() || alternate_visible())
            {
                button.emit_clicked();
            }
        });
        window.add_action(&action);
        if let Some(app) = window.application() {
            app.set_accels_for_action(&format!("win.{name}"), accelerators);
        }
    }

    // At the window's bubble phase, controls keep their own Space activation.
    // GTK editable children and selected rows are also explicitly excluded.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Bubble);
    let window_ref = window.downgrade();
    let play = play.downgrade();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if key == gdk::Key::space
            && (modifiers & gtk::accelerator_get_default_mod_mask()).is_empty()
            && let Some(window) = window_ref.upgrade()
            && window.visible_dialog().is_none()
            && !gtk::prelude::GtkWindowExt::focus(&window).is_some_and(uses_space)
            && play.upgrade().is_some_and(|play| {
                (play.is_mapped() || alternate_visible()) && play.is_sensitive()
            })
            && gtk::prelude::WidgetExt::activate_action(&window, "win.music-play-pause", None)
                .is_ok()
        {
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
}

fn uses_space(widget: gtk::Widget) -> bool {
    std::iter::successors(Some(widget), gtk::Widget::parent).any(|widget| {
        widget.is::<gtk::Editable>()
            || widget.is::<gtk::TextView>()
            || widget.is::<gtk::Button>()
            || widget.is::<gtk::CheckButton>()
            || widget.is::<gtk::DropDown>()
            || widget.is::<gtk::MenuButton>()
            || widget.is::<gtk::Popover>()
            || widget.is::<gtk::Switch>()
            || widget.is::<gtk::Range>()
            || widget.is::<gtk::ListBox>()
            || widget.is::<gtk::ListBoxRow>()
            || widget.is::<gtk::FlowBoxChild>()
            || widget
                .downcast_ref::<gtk::Label>()
                .is_some_and(gtk::Label::is_selectable)
    })
}

pub(super) fn install_help(window: &adw::ApplicationWindow, app: &adw::Application) {
    let action = gio::SimpleAction::new("shortcuts", None);
    let weak = window.downgrade();
    action.connect_activate(move |_, _| {
        if let Some(window) = weak.upgrade()
            && window.visible_dialog().is_none()
        {
            show_help(&window);
        }
    });
    window.add_action(&action);
    app.set_accels_for_action("win.shortcuts", &["F1"]);
}

fn show_help(window: &adw::ApplicationWindow) {
    let Some(app) = window.application() else {
        return;
    };
    let dialog = adw::PreferencesDialog::builder()
        .title("Сочетания клавиш")
        .search_enabled(false)
        .content_width(520)
        .content_height(580)
        .build();
    dialog.set_widget_name("karin-shortcuts");
    let page = adw::PreferencesPage::new();
    page.add_css_class("karin-font-primary");
    for (title, shortcuts) in [
        (
            "Воспроизведение",
            &[
                ("Воспроизведение и пауза", "space"),
                ("Следующий трек", "win.music-next"),
                ("Предыдущий трек", "win.music-previous"),
            ][..],
        ),
        (
            "Навигация и поиск",
            &[
                ("Главная", "win.music-home"),
                ("Моя коллекция", "win.music-collection"),
                ("Мне нравится", "win.music-library"),
                ("Плейлисты", "win.music-playlists"),
                ("Открыть поиск", "win.music-search"),
                ("Закрыть поиск", "Escape"),
                ("Вернуться из «Сейчас играет»", "Escape"),
                ("Обновить раздел", "win.music-refresh"),
                ("Очередь", "win.music-queue"),
                ("Сведения о треке", "win.music-info"),
                ("Настройки", "win.preferences"),
            ][..],
        ),
    ] {
        let group = adw::PreferencesGroup::builder().title(title).build();
        for (label, key) in shortcuts {
            let row = adw::ActionRow::builder()
                .title(*label)
                .title_lines(0)
                .build();
            if *key == "space" {
                row.set_subtitle("Когда фокус вне поля ввода и элементов управления");
            }
            let keys = gtk::Box::new(gtk::Orientation::Vertical, 4);
            keys.set_valign(gtk::Align::Center);
            if key.starts_with("win.") {
                for accelerator in app.accels_for_action(key) {
                    keys.append(&gtk::ShortcutLabel::new(&accelerator));
                }
            } else {
                keys.append(&gtk::ShortcutLabel::new(key));
            }
            row.add_suffix(&keys);
            group.add(&row);
        }
        page.add(&group);
    }
    dialog.add(&page);
    dialog.present(Some(window));
}
