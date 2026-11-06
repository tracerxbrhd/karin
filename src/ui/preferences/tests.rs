// SPDX-License-Identifier: GPL-3.0-or-later
//! Synthetic view snapshots/callbacks; no settings, keyring or account access.

use super::*;
use gtk::{gio, glib};
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[test]
fn usage_text_reports_actual_file_bytes_and_record_count() {
    assert_eq!(usage_text(CacheStats::default()), "0 Б · 0 записей");
    assert_eq!(
        usage_text(CacheStats {
            bytes: 512,
            entries: 1
        }),
        "512 Б · 1 запись"
    );
    assert_eq!(
        usage_text(CacheStats {
            bytes: 2048,
            entries: 2
        }),
        "2,0 КиБ · 2 записи"
    );
    assert_eq!(
        usage_text(CacheStats {
            bytes: 2 * MIB,
            entries: 11
        }),
        "2,0 МиБ · 11 записей"
    );
    assert_eq!(
        usage_text(CacheStats {
            bytes: 1024 * MIB,
            entries: 21
        }),
        "1,0 ГиБ · 21 запись"
    );
}

#[test]
fn unavailable_storage_allows_forget_intent_but_not_enabling() {
    assert!(remember_change_allowed(true, false, false));
    assert!(!remember_change_allowed(false, true, false));
    assert!(!remember_change_allowed(false, false, false));
    assert!(!remember_change_allowed(true, true, false));
    assert!(remember_change_allowed(true, false, true));
    assert!(remember_change_allowed(false, true, true));
}

#[test]
fn preference_edits_preserve_nonvisible_lifecycle_and_pane_fields() {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let values = Preferences {
        forget_pending: true,
        track_info_open: false,
        ..Preferences::default()
    };
    let state = State {
        values: RefCell::new(values.clone()),
        updating: Cell::new(false),
        ready: Cell::new(true),
        storage_supported: Cell::new(true),
        session_busy: Cell::new(false),
        cache_busy: Cell::new(false),
        can_retry_session: Cell::new(false),
        callbacks: PreferencesCallbacks {
            changed: Rc::new({
                let changes = Rc::clone(&changes);
                move |value| changes.borrow_mut().push(value)
            }),
            clear_cache: Rc::new(|| {}),
            retry_session: Rc::new(|| {}),
            reload_stats: Rc::new(|| {}),
        },
    };
    state.change(|value| value.appearance = Appearance::Dark);
    assert_eq!(changes.borrow().len(), 1);
    let changed = changes.borrow()[0].clone();
    assert!(changed.forget_pending);
    assert!(!changed.track_info_open);
    assert_eq!(changed.remember_login, values.remember_login);
    state.updating.set(true);
    state.change(|value| value.remember_login = false);
    state.updating.set(false);
    state.ready.set(false);
    state.change(|value| value.cache_enabled = false);
    assert_eq!(changes.borrow().len(), 1);
}

fn pump_until(mut condition: impl FnMut() -> bool) {
    let context = glib::MainContext::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if condition() {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "synthetic preferences operation timed out"
        );
        if context.pending() {
            context.iteration(false);
        } else {
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

fn settle() {
    let deadline = Instant::now() + Duration::from_millis(150);
    pump_until(|| Instant::now() >= deadline);
}

fn input(arguments: &[&str]) {
    assert!(
        Command::new("timeout")
            .args(["5", "xdotool"])
            .args(arguments)
            .status()
            .unwrap()
            .success()
    );
}

fn coordinates(window: &adw::ApplicationWindow, widget: &gtk::Widget) -> Option<(i32, i32)> {
    if !widget.is_mapped() || widget.width() <= 0 || widget.height() <= 0 {
        return None;
    }
    let native = widget.native()?;
    let bounds = widget.compute_bounds(native.upcast_ref::<gtk::Widget>())?;
    // A mapped allocation can still be clipped by the preferences viewport
    // while focus scrolling is animating. Require the actual GTK hit target,
    // rather than accepting any center inside the toplevel surface.
    let center_x = f64::from(bounds.x()) + f64::from(bounds.width()) / 2.0;
    let center_y = f64::from(bounds.y()) + f64::from(bounds.height()) / 2.0;
    let picked =
        native
            .upcast_ref::<gtk::Widget>()
            .pick(center_x, center_y, gtk::PickFlags::DEFAULT)?;
    if picked != *widget && !picked.is_ancestor(widget) {
        return None;
    }
    let (dx, dy) = native.surface_transform();
    let mut x = f64::from(bounds.x()) + f64::from(bounds.width()) / 2.0 + dx;
    let mut y = f64::from(bounds.y()) + f64::from(bounds.height()) / 2.0 + dy;
    let mut surface = native.surface()?;
    let target = window.surface()?;
    for _ in 0..8 {
        if surface == target {
            if x < 0.0
                || y < 0.0
                || x >= f64::from(target.width())
                || y >= f64::from(target.height())
            {
                return None;
            }
            let scale = f64::from(window.scale_factor());
            return Some(((x * scale).round() as i32, (y * scale).round() as i32));
        }
        let popup = surface.dynamic_cast::<gtk::gdk::Popup>().ok()?;
        x += f64::from(popup.position_x());
        y += f64::from(popup.position_y());
        surface = popup.parent()?;
    }
    None
}

fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
    let widget = widget.as_ref();
    let focus_accepted = widget.grab_focus();
    // AdwSwitchRow owns keyboard focus; its internal GtkSwitch can report
    // focusable=true while rejecting grab_focus(). When it is clipped, waiting
    // for a hit alone cannot scroll it into view. Focus the native row, then
    // keep targeting the actual switch with the same strict pointer checks.
    let row_focus = if !focus_accepted {
        widget.ancestor(adw::SwitchRow::static_type()).map(|row| {
            assert!(
                row.grab_focus(),
                "native switch row must accept keyboard focus"
            );
            row.type_().name().to_string()
        })
    } else {
        None
    };
    eprintln!(
        "Karin synthetic preferences pointer requested: widget={} target={} focus-accepted={focus_accepted} focus-owner={}",
        widget.type_().name(),
        widget.widget_name(),
        row_focus.as_deref().unwrap_or("target")
    );
    // Accepted focus is not a viewport-position acknowledgment on every
    // supported GTK/Adwaita version. Scroll the real native adjustment when
    // necessary, without substituting activation for an actual pointer hit.
    let reveal = |widget: &gtk::Widget| {
        let Some(scroller) = widget
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_then(|widget| widget.downcast::<gtk::ScrolledWindow>().ok())
        else {
            return;
        };
        if let Some(bounds) = widget.compute_bounds(&scroller) {
            let adjustment = scroller.vadjustment();
            let start = adjustment.value() + f64::from(bounds.y());
            adjustment.clamp_page(start, start + f64::from(bounds.height()));
        }
    };
    if coordinates(window, widget).is_none() {
        reveal(widget);
    }
    // Observe the mapped window's frames, including while the target is
    // clipped. Require two settled native hits before one real mouse click.
    let ready = Rc::new(Cell::new(None));
    let previous = Rc::new(Cell::new(None));
    let stable = Rc::new(Cell::new(0));
    let admitted = ready.clone();
    let target = widget.downgrade();
    window.add_tick_callback(move |window, _| {
        let Some(widget) = target.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let current = coordinates(window, &widget);
        if current.is_none() {
            reveal(&widget);
        }
        if current.is_some() && previous.get() == current {
            stable.set(stable.get() + 1);
        } else {
            stable.set(1);
        }
        previous.set(current);
        if stable.get() >= 2 && current.is_some() {
            admitted.set(current);
            glib::ControlFlow::Break
        } else {
            glib::ControlFlow::Continue
        }
    });
    pump_until(|| ready.get().is_some());
    let (x, y) = ready.get().unwrap();
    eprintln!(
        "Karin synthetic preferences pointer: widget={} target={} physical=({x},{y}) scale={} focus-accepted={focus_accepted} focusable={} native-hit=true stable-frames=2",
        widget.type_().name(),
        widget.widget_name(),
        window.scale_factor(),
        widget.is_focusable(),
    );
    input(&[
        "mousemove",
        "--window",
        id,
        &x.to_string(),
        &y.to_string(),
        "click",
        "1",
    ]);
    settle();
}

fn capture(window_id: &str, name: &str) {
    let Ok(directory) = std::env::var("KARIN_TEST_CAPTURE_DIR") else {
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_enable_animations(false);
    }
    for (theme, scheme) in [
        ("light", adw::ColorScheme::ForceLight),
        ("dark", adw::ColorScheme::ForceDark),
    ] {
        adw::StyleManager::default().set_color_scheme(scheme);
        settle();
        assert!(
            Command::new("import")
                .args(["-silent", "-screen", "-window", window_id])
                .arg(directory.join(format!("{name}-{theme}.png")))
                .status()
                .unwrap()
                .success()
        );
    }
}

#[test]
#[ignore = "requires isolated Xvfb, xdotool and GTK; synthetic snapshots only"]
fn native_preferences_persistent_controls_snapshot_updates_and_cleanup() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let app = adw::Application::builder()
        .application_id("io.github.tracerxbrhd.Karin.PreferencesFixture")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.register(None::<&gio::Cancellable>).unwrap();
    let window = adw::ApplicationWindow::builder()
        .application(&app)
        .title("Karin preferences synthetic fixture")
        .default_width(900)
        .default_height(650)
        .build();
    window.set_content(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
    let changes = Rc::new(RefCell::new(Vec::<Preferences>::new()));
    let clear = Rc::new(Cell::new(0));
    let retry = Rc::new(Cell::new(0));
    let stats = Rc::new(Cell::new(0));
    let snapshot = Rc::new(RefCell::new(PreferencesSnapshot {
        values: Preferences {
            track_info_open: false,
            ..Preferences::default()
        },
        settings_ready: true,
        storage_supported: true,
        session_status: "Сохранённый вход можно проверить повторно.".into(),
        session_busy: false,
        can_retry_session: true,
        cache_stats: Some(CacheStats {
            bytes: 2 * MIB,
            entries: 3,
        }),
        audio_cache_stats: Some(CacheStats::default()),
        cache_busy: false,
        message: None,
    }));
    let view_pointer = Rc::new(RefCell::new(std::rc::Weak::<PreferencesView>::new()));
    let reenter = Rc::new(Cell::new(false));
    let callbacks = PreferencesCallbacks {
        changed: Rc::new({
            let changes = Rc::clone(&changes);
            let pointer = Rc::clone(&view_pointer);
            let snapshot = Rc::clone(&snapshot);
            let reenter = Rc::clone(&reenter);
            move |values| {
                changes.borrow_mut().push(values.clone());
                if reenter.get() {
                    snapshot.borrow_mut().values = values;
                    if let Some(view) = pointer.borrow().upgrade() {
                        view.update(&snapshot.borrow());
                    }
                }
            }
        }),
        clear_cache: Rc::new({
            let clear = Rc::clone(&clear);
            move || clear.set(clear.get() + 1)
        }),
        retry_session: Rc::new({
            let retry = Rc::clone(&retry);
            move || retry.set(retry.get() + 1)
        }),
        reload_stats: Rc::new({
            let stats = Rc::clone(&stats);
            move || stats.set(stats.get() + 1)
        }),
    };
    let view = Rc::new(PreferencesView::new(&snapshot.borrow(), &callbacks));
    *view_pointer.borrow_mut() = Rc::downgrade(&view);
    assert!(
        changes.borrow().is_empty(),
        "initial render must not save settings"
    );
    assert!(view.remember.is_active());
    assert_eq!(view.cache_limit.value(), 256.0);
    assert_eq!(view.audio_cache_limit.value(), 256.0);
    assert!(
        view.audio_cache_usage
            .subtitle()
            .unwrap()
            .contains("Сохранение музыки пока недоступно")
    );
    assert_eq!(
        view.cache_usage.subtitle().as_deref(),
        Some("2,0 МиБ · 3 записи")
    );
    window.present();
    view.present(&window);
    pump_until(|| view.dialog.is_mapped());
    let output = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin preferences synthetic fixture$",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let id = String::from_utf8(output.stdout).unwrap();
    let id = id.lines().next().unwrap();
    input(&["windowfocus", id]);
    capture(id, "preferences-seamless-medium");
    let switch = view
        .remember
        .activatable_widget()
        .expect("native remember switch");
    click(&window, id, &switch);
    pump_until(|| changes.borrow().len() == 1);
    assert!(!changes.borrow()[0].remember_login);
    assert!(!changes.borrow()[0].track_info_open);
    view.scheme.set_selected(2);
    assert_eq!(changes.borrow().len(), 2);
    assert_eq!(
        changes.borrow().last().unwrap().appearance,
        Appearance::Dark
    );
    view.cache_limit.set_value(512.0);
    assert_eq!(
        changes.borrow().last().unwrap().cache_limit_bytes,
        512 * MIB
    );
    view.audio_cache_limit.set_value(1024.0);
    assert_eq!(
        changes.borrow().last().unwrap().audio_cache_limit_bytes,
        1024 * MIB
    );
    assert_eq!(
        changes.borrow().last().unwrap().cache_limit_bytes,
        512 * MIB
    );
    click(&window, id, &view.cache_clear);
    pump_until(|| clear.get() > 0);
    assert_eq!(clear.get(), 1);
    view.cache_refresh.grab_focus();
    input(&["key", "Return"]);
    pump_until(|| stats.get() == 1);
    click(&window, id, &view.session_retry);
    pump_until(|| retry.get() > 0);
    assert_eq!(retry.get(), 1);

    let count = changes.borrow().len();
    {
        let mut next = snapshot.borrow_mut();
        next.values.cache_enabled = false;
        next.values.appearance = Appearance::Light;
        next.values.cache_limit_bytes = 128 * MIB;
        next.cache_stats = Some(CacheStats::default());
    }
    view.update(&snapshot.borrow());
    assert_eq!(
        changes.borrow().len(),
        count,
        "snapshot updates must not dispatch callbacks"
    );
    assert!(view.cache_enabled.subtitle().unwrap().contains("остаются"));
    assert_eq!(
        view.cache_usage.subtitle().as_deref(),
        Some("0 Б · 0 записей")
    );
    window.set_default_size(420, 720);
    // Resize is acknowledged asynchronously by GTK/X11, especially at 2x
    // while another native fixture is rendering. Wait for the existing
    // compact-width gate instead of sampling after a fixed 150 ms sleep.
    pump_until(|| window.width() <= 444);
    assert!(window.width() <= 444);
    capture(id, "preferences-seamless-compact");
    window.set_default_size(1440, 900);
    settle();
    capture(id, "preferences-seamless-wide");
    snapshot.borrow_mut().session_busy = true;
    view.update(&snapshot.borrow());
    assert!(
        view.scheme.is_sensitive(),
        "ordinary session work must not freeze unrelated settings"
    );
    assert!(view.cache_clear.is_sensitive());
    assert!(!view.remember.is_sensitive());
    {
        let mut next = snapshot.borrow_mut();
        next.session_busy = true;
        next.settings_ready = false;
    }
    view.update(&snapshot.borrow());
    assert!(!view.scheme.is_sensitive());
    assert!(!view.remember.is_sensitive());
    assert!(!view.cache_clear.is_sensitive());
    view.session_retry.emit_clicked();
    view.cache_clear.emit_clicked();
    view.cache_refresh.emit_clicked();
    view.scheme.set_selected(0);
    assert_eq!(changes.borrow().len(), count);
    assert_eq!(retry.get(), 1);
    assert_eq!(clear.get(), 1);
    assert_eq!(stats.get(), 1);
    capture(id, "preferences-seamless-session-busy");
    {
        let mut next = snapshot.borrow_mut();
        next.session_busy = false;
        next.settings_ready = true;
        next.cache_busy = true;
        next.message = Some("Обновляем данные кеша…".into());
    }
    view.update(&snapshot.borrow());
    assert!(!view.cache_enabled.is_sensitive());
    assert!(!view.cache_limit.is_sensitive());
    assert!(!view.audio_cache_limit.is_sensitive());
    assert!(!view.cache_clear.is_sensitive());
    assert!(view.scheme.is_sensitive());
    capture(id, "preferences-seamless-cache-busy");
    {
        let mut next = snapshot.borrow_mut();
        next.cache_busy = false;
        next.message = None;
    }
    view.update(&snapshot.borrow());
    window.set_default_size(420, 720);
    settle();
    assert!(view.cache_clear.grab_focus());
    settle();
    snapshot.borrow_mut().message =
        Some("Не удалось полностью очистить кеш. Повторите попытку.".into());
    view.update(&snapshot.borrow());
    pump_until(|| {
        view.operation_status.has_focus()
            && view
                .operation_status
                .compute_bounds(&window)
                .is_some_and(|bounds| {
                    bounds.y() >= 0.0 && bounds.y() + bounds.height() <= window.height() as f32
                })
    });
    let bounds = view
        .operation_status
        .compute_bounds(&window)
        .expect("inline operation status");
    assert!(bounds.y() >= 0.0 && bounds.y() + bounds.height() <= window.height() as f32);
    assert!(
        view.operation_status
            .subtitle()
            .unwrap()
            .contains("Не удалось")
    );
    capture(id, "preferences-seamless-compact-operation-error");
    assert!(view.cache_clear.grab_focus());
    settle();
    for _ in 0..3 {
        view.update(&snapshot.borrow());
        settle();
        assert!(
            view.cache_clear.has_focus(),
            "unchanged periodic status must not steal focus"
        );
    }
    snapshot.borrow_mut().message =
        Some("Настройки не сохранены. Изменения действуют в этом запуске.".into());
    view.update(&snapshot.borrow());
    pump_until(|| view.operation_status.has_focus());
    capture(id, "preferences-seamless-compact-save-error");
    {
        let mut next = snapshot.borrow_mut();
        next.cache_busy = false;
        next.storage_supported = false;
        next.can_retry_session = true;
        next.session_status = "Защищённое хранилище сейчас недоступно.".into();
        next.message = Some("Не удалось выполнить очистку. Повторите попытку.".into());
    }
    // A new inline error reveals itself from an idle callback. A screenshot
    // happens to drain that callback, but a no-capture run must acknowledge it
    // explicitly before beginning the next independent focus/scroll scenario.
    assert!(view.cache_clear.grab_focus());
    assert!(!view.operation_status.has_focus());
    view.update(&snapshot.borrow());
    pump_until(|| view.operation_status.has_focus());
    assert!(view.remember.is_active());
    assert!(view.remember.is_sensitive());
    assert!(view.session_retry.is_visible());
    assert_eq!(changes.borrow().len(), count);
    capture(id, "preferences-seamless-unavailable");
    let switch = view
        .remember
        .activatable_widget()
        .expect("native remember switch");
    // Exercise the same clipped target regardless of Adwaita/font version:
    // scroll to the bottom while its row already owns focus. Reasserting focus
    // alone must not substitute for bringing this real pointer target back.
    assert!(view.remember.grab_focus());
    let scroller = switch
        .ancestor(gtk::ScrolledWindow::static_type())
        .expect("preferences viewport")
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let adjustment = scroller.vadjustment();
    assert!(adjustment.upper() > adjustment.page_size());
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    pump_until(|| {
        coordinates(&window, &switch).is_none()
            && coordinates(&window, view.cache_clear.upcast_ref()).is_some()
    });
    assert!(coordinates(&window, &switch).is_none());
    assert!(view.remember.has_focus());
    eprintln!("Karin synthetic preferences regression: clipped remember switch");
    click(&window, id, &switch);
    pump_until(|| changes.borrow().len() == count + 1);
    assert!(!changes.borrow().last().unwrap().remember_login);
    assert!(!view.remember.is_active());
    assert!(!view.remember.is_sensitive());
    view.remember.set_active(true);
    assert!(
        !view.remember.is_active(),
        "unsupported store cannot enable remembering"
    );
    assert_eq!(changes.borrow().len(), count + 1);
    {
        let mut next = snapshot.borrow_mut();
        next.settings_ready = false;
        next.session_busy = false;
        next.can_retry_session = true;
        next.session_status = "Каталог настроек пока недоступен.".into();
    }
    view.update(&snapshot.borrow());
    assert!(!view.scheme.is_sensitive());
    assert!(
        view.session_retry.is_sensitive(),
        "typed setup Retry remains available while editable settings are frozen"
    );
    click(&window, id, &view.session_retry);
    pump_until(|| retry.get() == 2);
    {
        let mut next = snapshot.borrow_mut();
        next.settings_ready = true;
        next.storage_supported = true;
        next.can_retry_session = false;
        next.values.remember_login = true;
        next.message = None;
    }
    view.update(&snapshot.borrow());
    assert!(!view.session_retry.is_visible());
    reenter.set(true);
    view.scheme.set_selected(2);
    assert_eq!(
        changes.borrow().len(),
        count + 2,
        "reentrant update must not loop"
    );
    view.dialog.close();
    pump_until(|| !view.dialog.is_mapped());
    window.close();
    let weak = view.dialog.downgrade();
    drop(view);
    drop(window);
    pump_until(|| weak.upgrade().is_none());
}
