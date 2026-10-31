// SPDX-License-Identifier: GPL-3.0-or-later
//! Native, session-scoped appearance preferences; no credential persistence.

use adw::prelude::*;

pub(super) fn present(window: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::builder().title("Настройки").build();
    let page = adw::PreferencesPage::new();
    let appearance = adw::PreferencesGroup::builder()
        .title("Оформление")
        .description("Изменения действуют в текущем запуске Karin.")
        .build();
    let style = adw::StyleManager::default();
    let scheme = adw::ComboRow::builder()
        .title("Цветовая схема")
        .model(&gtk::StringList::new(&["Системная", "Светлая", "Тёмная"]))
        .selected(match style.color_scheme() {
            adw::ColorScheme::ForceLight | adw::ColorScheme::PreferLight => 1,
            adw::ColorScheme::ForceDark | adw::ColorScheme::PreferDark => 2,
            _ => 0,
        })
        .build();
    scheme.connect_selected_notify(move |row| {
        style.set_color_scheme(match row.selected() {
            1 => adw::ColorScheme::ForceLight,
            2 => adw::ColorScheme::ForceDark,
            _ => adw::ColorScheme::Default,
        });
    });
    appearance.add(&scheme);
    page.add(&appearance);
    dialog.add(&page);
    dialog.present(Some(window));
}
