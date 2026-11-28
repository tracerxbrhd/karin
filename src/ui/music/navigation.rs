// SPDX-License-Identifier: GPL-3.0-or-later
//! Back navigation for the regular music workspace.

use super::*;

const HISTORY_LIMIT: usize = 24;

pub(super) type DetailHistory = Vec<(DetailRefresh, f64)>;

#[derive(Default)]
pub(super) struct History {
    sections: RefCell<Vec<Section>>,
    pub(super) details: RefCell<DetailHistory>,
    pub(super) detail_origin_scroll: Cell<f64>,
}

struct Section {
    page: &'static str,
    scroll: f64,
}

fn section_name(page: &str) -> Option<&'static str> {
    match page {
        "home" => Some("home"),
        "collection" => Some("collection"),
        "library" => Some("library"),
        "playlists" => Some("playlists"),
        _ => None,
    }
}

fn remember<T>(history: &RefCell<Vec<T>>, entry: T) {
    let mut history = history.borrow_mut();
    if history.len() == HISTORY_LIMIT {
        history.remove(0);
    }
    history.push(entry);
}

impl Controller {
    pub(super) fn navigate_music_section(self: &Rc<Self>, page: &'static str) {
        let current = self.widgets.stack.visible_child_name();
        if current.as_deref() != Some(page) {
            let previous = match current.as_deref() {
                Some("detail") => section_name(&self.model.borrow().detail_origin)
                    .map(|origin| (origin, self.navigation_history.detail_origin_scroll.get()))
                    .or_else(|| {
                        self.search_return_section().and_then(|(origin, scroll)| {
                            section_name(&origin).map(|origin| (origin, scroll))
                        })
                    }),
                Some("search") => self.search_return_section().and_then(|(origin, scroll)| {
                    section_name(&origin).map(|origin| (origin, scroll))
                }),
                Some(current) => section_name(current).map(|origin| (origin, self.music_scroll())),
                None => None,
            };
            if let Some((previous, scroll)) = previous.filter(|(previous, _)| *previous != page) {
                remember(
                    &self.navigation_history.sections,
                    Section {
                        page: previous,
                        scroll,
                    },
                );
            }
        }
        self.close_global_search(false);
        self.show_music_section(page);
    }

    fn show_music_section(self: &Rc<Self>, page: &str) {
        self.widgets.stack.set_visible_child_name(page);
        if matches!(page, "library" | "playlists") {
            let mode = u32::from(page == "playlists");
            let selection = &self.widgets.library_mode;
            if selection.selected() == mode {
                self.show_library();
            } else {
                selection.set_selected(mode);
            }
        }
        self.sync_music_back();
    }

    pub(super) fn sync_music_back(&self) {
        let ready = {
            let model = self.model.borrow();
            model.account_valid && !model.closed
        };
        if let Some(action) = self
            .window
            .upgrade()
            .and_then(|window| window.lookup_action("music-back"))
            .and_downcast::<gio::SimpleAction>()
        {
            action.set_enabled(
                ready
                    && (self.now_playing.is_active()
                        || self.widgets.search_header.is_open()
                        || self.compact_panel_is_open()
                        || self.widgets.stack.visible_child_name().as_deref() == Some("detail")
                        || !self.navigation_history.sections.borrow().is_empty()),
            );
        }
    }

    pub(super) fn music_back(self: &Rc<Self>) {
        if self.close_now_playing(true) {
            return;
        }
        if self.compact_panel_is_open() {
            self.set_panel_mode(workspace::PanelMode::Closed);
            self.return_information_focus();
            return;
        }
        let in_detail = self.widgets.stack.visible_child_name().as_deref() == Some("detail");
        if self.widgets.search_header.is_open()
            && (!in_detail || self.model.borrow().detail_origin != "search")
        {
            self.close_global_search(true);
            return;
        }
        if in_detail {
            self.back_from_detail();
            return;
        }
        let previous = self.navigation_history.sections.borrow_mut().pop();
        if let Some(previous) = previous {
            self.show_music_section(previous.page);
            self.restore_music_scroll(previous.scroll);
        }
    }

    fn compact_panel_is_open(&self) -> bool {
        self.info_host.borrow().as_ref().is_some_and(|host| {
            host.is_open() && host.presentation() == workspace::InfoPresentation::Compact
        })
    }

    pub(super) fn remember_detail(self: &Rc<Self>) {
        if self.widgets.stack.visible_child_name().as_deref() == Some("detail")
            && let Some(reopen) = self.detail_refresh.borrow().clone()
        {
            remember(
                &self.navigation_history.details,
                (reopen, self.music_scroll()),
            );
        }
    }

    pub(super) fn reopen_previous_detail(self: &Rc<Self>) -> bool {
        let previous = self.navigation_history.details.borrow_mut().pop();
        if let Some((reopen, scroll)) = previous {
            reopen(self, false);
            self.restore_music_scroll(scroll);
            return true;
        }
        false
    }

    pub(super) fn retire_detail_history(&self) {
        self.navigation_history.details.borrow_mut().clear();
    }

    pub(super) fn retire_navigation_history(&self) {
        self.retire_detail_history();
        self.navigation_history.sections.borrow_mut().clear();
        self.navigation_history.detail_origin_scroll.set(0.0);
        self.sync_music_back();
    }

    pub(super) fn music_scroll(&self) -> f64 {
        self.content_scroll
            .borrow()
            .upgrade()
            .map_or(0.0, |scroll| scroll.vadjustment().value())
    }

    pub(super) fn restore_music_scroll(self: &Rc<Self>, position: f64) {
        if let Some(scroll) = self.content_scroll.borrow().upgrade() {
            let weak = Rc::downgrade(self);
            let generation = self.model.borrow().view_generation;
            // GtkStack measures its newly visible child in the next layout.
            // Restore the offset on the following frame when its range is current.
            let first = Cell::new(true);
            scroll.add_tick_callback(move |scroll, _| {
                let Some(this) = weak.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                if this.model.borrow().view_generation != generation {
                    return glib::ControlFlow::Break;
                }
                if first.replace(false) {
                    return glib::ControlFlow::Continue;
                }
                scroll.vadjustment().set_value(position);
                glib::ControlFlow::Break
            });
        }
    }
}
