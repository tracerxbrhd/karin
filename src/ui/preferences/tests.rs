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
fn cache_limit_scale_has_exact_discrete_choices_and_rejects_nonfinite_values() {
    for (raw, expected) in [
        (-100.0, CacheLimit::MiB500),
        (0.49, CacheLimit::MiB500),
        (0.5, CacheLimit::GiB1),
        (1.49, CacheLimit::GiB1),
        (2.2, CacheLimit::GiB2),
        (3.5, CacheLimit::GiB4),
        (4.7, CacheLimit::Unlimited),
        (100.0, CacheLimit::Unlimited),
    ] {
        assert_eq!(limit_from_scale(raw), Some(expected));
    }
    for raw in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(limit_from_scale(raw), None);
    }
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
        ui_style: UiStyle::Karin,
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
        can_retry_cache: Cell::new(false),
        callbacks: PreferencesCallbacks {
            changed: Rc::new({
                let changes = Rc::clone(&changes);
                move |value| changes.borrow_mut().push(value)
            }),
            clear_cache: Rc::new(|| {}),
            retry_session: Rc::new(|| {}),
            reload_stats: Rc::new(|| {}),
            retry_cache: Rc::new(|| {}),
        },
    };
    state.change(|value| value.appearance = Appearance::Dark);
    assert_eq!(changes.borrow().len(), 1);
    let changed = changes.borrow()[0].clone();
    assert!(changed.forget_pending);
    assert!(!changed.track_info_open);
    assert_eq!(changed.remember_login, values.remember_login);
    assert_eq!(changed.ui_style, UiStyle::Karin);
    state.updating.set(true);
    state.change(|value| value.remember_login = false);
    state.updating.set(false);
    state.ready.set(false);
    state.change(|value| value.cache_enabled = false);
    assert_eq!(changes.borrow().len(), 1);
}

// Adwaita reads its documented debug settings once, at initialization. Isolate
// them in fresh processes rather than mutating GTK/global environment in a
// running test. These are synthetic system settings, not a KDE portal claim.
fn appearance_system_matrix() -> bool {
    const SELECTOR: &str = "ui::preferences::tests::native_preferences_persistent_controls_snapshot_updates_and_cleanup";
    if let Ok(mode) = std::env::var("KARIN_TEST_APPEARANCE_MODE") {
        native_system_appearance(&mode);
        return true;
    }
    for (mode, scheme, contrast) in [
        ("system-light", "prefer-light", "0"),
        ("system-dark", "prefer-dark", "0"),
        ("contrast-light", "prefer-light", "1"),
        ("contrast-dark", "prefer-dark", "1"),
    ] {
        let output = Command::new("timeout")
            .arg("20")
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                SELECTOR,
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env("KARIN_TEST_APPEARANCE_MODE", mode)
            .env("ADW_DEBUG_COLOR_SCHEME", scheme)
            .env("ADW_DEBUG_HIGH_CONTRAST", contrast)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "native appearance {mode}: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    false
}

fn native_system_appearance(mode: &str) {
    gtk::init().unwrap();
    adw::init().unwrap();
    let dark = mode.ends_with("dark");
    let high_contrast = mode.starts_with("contrast-");
    let manager = adw::StyleManager::default();
    assert_eq!(manager.is_high_contrast(), high_contrast);
    let window = adw::ApplicationWindow::builder()
        .title("Karin native appearance fixture")
        .default_width(420)
        .default_height(720)
        .build();
    window.add_css_class("music-shell");
    let label = gtk::Label::new(Some("Системное оформление · Native appearance"));
    window.set_content(Some(&label));
    let parent = label.parent();
    let settings = gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(false);
    let font = settings.gtk_font_name();
    let values = Preferences::default();
    let applied = super::super::style::apply(&values, &window);
    assert_eq!(applied.requested, UiStyle::System);
    assert_eq!(applied.effective, UiStyle::System);
    assert_eq!(manager.color_scheme(), adw::ColorScheme::Default);
    pump_until(|| manager.is_dark() == dark);
    window.present();
    pump_until(|| window.is_mapped());
    assert_eq!(window.has_css_class("karin-high-contrast"), high_contrast);
    if let Ok(scale) = std::env::var("GDK_SCALE") {
        assert_eq!(window.scale_factor(), scale.parse::<i32>().unwrap());
    }
    for color in [Appearance::Light, Appearance::Dark, Appearance::System] {
        let mut next = values.clone();
        next.ui_style = UiStyle::Karin;
        next.appearance = color;
        let selected = super::super::style::apply(&next, &window);
        assert_eq!(selected.requested, UiStyle::Karin);
        assert_eq!(selected.effective, UiStyle::System);
        assert!(window.has_css_class("karin-style-system"));
        assert!(!window.has_css_class("karin-style-custom"));
        pump_until(|| {
            manager.is_dark()
                == match color {
                    Appearance::System => dark,
                    Appearance::Light => false,
                    Appearance::Dark => true,
                }
        });
        assert_eq!(manager.is_high_contrast(), high_contrast);
        assert_eq!(
            settings.gtk_font_name(),
            font,
            "appearance must inherit system font"
        );
        assert!(
            !settings.is_gtk_enable_animations(),
            "appearance must preserve reduced motion"
        );
        assert_eq!(
            label.parent(),
            parent,
            "a scheme change does not rebuild the view"
        );
    }
    // Returning to System picks the settings source again after both overrides.
    assert_eq!(manager.color_scheme(), adw::ColorScheme::Default);
    window.close();
    eprintln!(
        "Karin synthetic appearance: mode={mode} system-return=pass overrides=pass font=preserved reduced-motion=preserved"
    );
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

pub(super) fn click(window: &adw::ApplicationWindow, id: &str, widget: &impl IsA<gtk::Widget>) {
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
    if appearance_system_matrix() {
        return;
    }
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
    let cache_retry = Rc::new(Cell::new(0));
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
        cache_busy: false,
        can_retry_cache: false,
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
        retry_cache: Rc::new({
            let retry = Rc::clone(&cache_retry);
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
    assert!(view.style_system.is_active());
    assert!(view.style_system.is_sensitive());
    assert!(!view.style_karin.is_active());
    assert!(!view.style_karin.is_sensitive());
    assert!(matches!(
        view.style_system.accessible_role(),
        gtk::AccessibleRole::Radio | gtk::AccessibleRole::Checkbox
    ));
    assert_eq!(
        view.style_system.accessible_role(),
        view.style_karin.accessible_role()
    );
    assert_eq!(view.cache_limit.value(), 2.0);
    assert_eq!(view.cache_limit_label.label(), "2 ГБ");
    assert_eq!(view.cache_limit.adjustment().lower(), 0.0);
    assert_eq!(view.cache_limit.adjustment().upper(), 5.0);
    assert_eq!(view.cache_limit.adjustment().step_increment(), 1.0);
    assert_eq!(
        view.cache_limit.accessible_role(),
        gtk::AccessibleRole::Slider
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
    // Exercise the real native Scale with keyboard input, not only set_value.
    assert!(view.cache_limit.grab_focus());
    input(&["key", "End"]);
    pump_until(|| changes.borrow().last().unwrap().cache_limit == CacheLimit::Unlimited);
    assert_eq!(view.cache_limit.value(), 5.0);
    assert_eq!(view.cache_limit_label.label(), "Без ограничений");
    input(&["key", "Home"]);
    pump_until(|| changes.borrow().last().unwrap().cache_limit == CacheLimit::MiB500);
    input(&["key", "Right"]);
    pump_until(|| changes.borrow().last().unwrap().cache_limit == CacheLimit::GiB1);
    // A real pointer activation hits the Scale's native slider surface. This
    // exact target uses the same clipping and two-frame checks as other controls.
    click(&window, id, &view.cache_limit);
    assert_eq!(view.cache_limit.value().fract(), 0.0);
    // Show the full cache group in the proof captures, including ticks, usage
    // and Clear, instead of accepting only the Scale's visible center.
    let cache_scroller = view
        .cache_limit
        .ancestor(gtk::ScrolledWindow::static_type())
        .unwrap()
        .downcast::<gtk::ScrolledWindow>()
        .unwrap();
    let cache_adjustment = cache_scroller.vadjustment();
    cache_adjustment.set_value(cache_adjustment.upper() - cache_adjustment.page_size());
    pump_until(|| {
        view.cache_limit
            .compute_bounds(&cache_scroller)
            .is_some_and(|bounds| {
                bounds.y() >= 0.0 && bounds.y() + bounds.height() <= cache_scroller.height() as f32
            })
    });
    for limit in CacheLimit::ALL {
        view.cache_limit.set_value(f64::from(limit.index()));
        assert_eq!(view.cache_limit_label.label(), limit.label());
        assert_eq!(changes.borrow().last().unwrap().cache_limit, limit);
        capture(id, &format!("preferences-unified-limit-{}", limit.index()));
    }
    let count_before_fraction = changes.borrow().len();
    view.cache_limit.set_value(3.4);
    assert_eq!(view.cache_limit.value(), 3.0);
    assert_eq!(
        changes.borrow().last().unwrap().cache_limit,
        CacheLimit::GiB3
    );
    assert!(changes.borrow().len() <= count_before_fraction + 1);
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
        next.values.cache_limit = CacheLimit::MiB500;
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
    assert!(view.cache_limit.grab_focus());
    pump_until(|| coordinates(&window, view.cache_limit.upcast_ref()).is_some());
    let bounds = view.cache_limit.compute_bounds(&window).unwrap();
    assert!(bounds.x() >= 0.0 && bounds.x() + bounds.width() <= window.width() as f32);
    assert_eq!(view.cache_limit.value(), 0.0);
    capture(id, "preferences-seamless-compact");
    window.set_default_size(1440, 900);
    pump_until(|| window.width() >= 1400);
    assert!(view.cache_limit.grab_focus());
    pump_until(|| coordinates(&window, view.cache_limit.upcast_ref()).is_some());
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
    assert!(
        view.cache_enabled.is_sensitive(),
        "cache off remains available during work"
    );
    assert!(
        view.cache_limit.is_sensitive(),
        "quota gestures coalesce while work is running"
    );
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
    snapshot.borrow_mut().can_retry_cache = true;
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
    assert!(view.cache_retry.is_visible());
    click(&window, id, &view.cache_retry);
    pump_until(|| cache_retry.get() == 1);
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
    // The unified cache group is shorter than the former two-quota layout.
    // The standard 420×720 case above now legitimately shows this switch even
    // at the bottom. Use a separate smaller native viewport to establish the
    // original clipped-target precondition, keeping its actual hit/focus gates.
    window.set_default_size(420, 520);
    pump_until(|| window.height() <= 544);
    // Scroll to the bottom while the row already owns focus. Reasserting focus
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
    // A persisted future design is an explicit fallback, not a fake active
    // theme. A real keyboard activation confirms System exactly once and
    // preserves the independent color, auth/barrier, pane and cache choices.
    {
        let mut next = snapshot.borrow_mut();
        next.values.ui_style = UiStyle::Karin;
        next.values.forget_pending = true;
        next.values.cache_limit = CacheLimit::GiB4;
        next.values.track_info_open = false;
    }
    view.update(&snapshot.borrow());
    assert_eq!(changes.borrow().len(), count + 2);
    assert!(view.style_system.is_active());
    assert!(!view.style_karin.is_active());
    assert!(!view.style_karin.is_sensitive());
    assert!(
        view.system_style_row
            .subtitle()
            .unwrap()
            .contains("пока недоступен")
    );
    click(&window, id, &view.system_style_row);
    pump_until(|| changes.borrow().len() == count + 3);
    let confirmed = changes.borrow().last().unwrap().clone();
    let mut expected = snapshot.borrow().values.clone();
    expected.ui_style = UiStyle::System;
    assert_eq!(confirmed, expected);
    assert!(view.style_system.grab_focus());
    input(&["key", "--window", id, "space"]);
    settle();
    assert_eq!(
        changes.borrow().len(),
        count + 3,
        "already selected native radio is idempotent"
    );
    view.dialog.close();
    pump_until(|| !view.dialog.is_mapped());
    window.close();
    let weak = view.dialog.downgrade();
    drop(view);
    drop(window);
    pump_until(|| weak.upgrade().is_none());
}
