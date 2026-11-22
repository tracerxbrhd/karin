// SPDX-License-Identifier: GPL-3.0-or-later
//! Production MusicService requests, native consent and remembered permission.

use super::*;

fn find_check(root: &gtk::Widget) -> Option<gtk::CheckButton> {
    if root.widget_name() == "music-lyrics-remember-external" {
        return root.clone().downcast().ok();
    }
    let mut child = root.first_child();
    while let Some(widget) = child {
        if let Some(check) = find_check(&widget) {
            return Some(check);
        }
        child = widget.next_sibling();
    }
    None
}

fn request_spacing() {
    let deadline = Instant::now() + Duration::from_millis(350);
    pump_until(|| Instant::now() >= deadline);
}

pub(super) fn run(app: &adw::Application) {
    let fixture = Fixture::with_karaoke_audio(true);
    let ui = window_for(app, &fixture);
    let window = &ui.window;
    let this = &ui.music.controller;
    let view = &this.track_pane.extras;
    this.play_music_context(
        PlaybackContext::new(
            ContextKind::Queue,
            None,
            vec![track(42), track(44), track(46)],
            0,
            false,
        )
        .unwrap(),
    );
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Playing)
    });
    this.pause_music();
    pump_until(|| {
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    });
    let output = Command::new("xdotool")
        .args([
            "search",
            "--onlyvisible",
            "--name",
            "^Karin synthetic HTTP fixture$",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).unwrap();
    let id = output.lines().next().unwrap();
    input(&["windowfocus", id]);
    click(window, id, &this.widgets.lyrics, false);
    pump_until(|| view.inner.lyrics.borrow().is_some());
    assert!(!view.inner.synchronized.get());
    assert_eq!(view.inner.lyrics_status.text(), "Текст без синхронизации");
    assert_eq!(fixture.counts.lock().unwrap().external, 0);

    // Checking remember and then cancelling never grants permission or sends metadata.
    click(window, id, &view.inner.external, false);
    let dialog = external_consent_ready(window, view);
    let remember = find_check(dialog.upcast_ref()).unwrap();
    assert!(!remember.is_active());
    click(window, id, &remember, false);
    click(
        window,
        id,
        &find_button(dialog.upcast_ref(), "Отмена").unwrap(),
        false,
    );
    external_consent_closed(view, &dialog);
    assert!(!this.persistence.borrow().values.lyrics_external_auto);
    assert_eq!(fixture.counts.lock().unwrap().external, 0);

    click(window, id, &view.inner.external, false);
    let dialog = external_consent_ready(window, view);
    let remember = find_check(dialog.upcast_ref()).unwrap();
    assert!(!remember.is_active());
    click(window, id, &remember, false);
    click(
        window,
        id,
        &find_button(dialog.upcast_ref(), "Искать").unwrap(),
        false,
    );
    external_consent_closed(view, &dialog);
    pump_until(|| view.inner.synchronized.get());
    assert!(this.persistence.borrow().values.lyrics_external_auto);
    assert!(view.inner.auto_external.get());
    assert!(view.inner.active.get().is_some());
    assert_eq!(fixture.counts.lock().unwrap().external, 1);
    assert!(
        this.music_snapshot()
            .is_some_and(|s| s.state == PlaybackState::Paused)
    );

    // Permission applies to the next opened selection once. Missing/error/plain
    // responses retain the real Yandex TEXT and its honest source/type.
    request_spacing();
    fixture.counts.lock().unwrap().external_status = Some(404);
    this.skip_music(true, false);
    pump_until(|| fixture.counts.lock().unwrap().external == 2 && !view.inner.lyrics_loading.get());
    let plain = view.inner.lyrics.borrow().as_ref().unwrap().plain.clone();
    assert!(plain.contains("text 44"));
    assert!(!view.inner.synchronized.get());
    assert!(
        view.inner
            .lyrics_status
            .text()
            .starts_with("Текст без синхронизации")
    );
    assert_eq!(view.inner.source.text(), "Источник: Яндекс Музыка");
    assert!(view.inner.external_consent.borrow().is_none());
    request_spacing();
    assert_eq!(
        fixture.counts.lock().unwrap().external,
        2,
        "no automatic retry after unavailable LRC"
    );

    fixture.counts.lock().unwrap().external_status = Some(500);
    click(window, id, &view.inner.external, false);
    pump_until(|| fixture.counts.lock().unwrap().external == 3 && !view.inner.lyrics_loading.get());
    assert_eq!(view.inner.lyrics.borrow().as_ref().unwrap().plain, plain);
    assert!(view.inner.lyrics_retry.is_visible());
    request_spacing();
    {
        let mut counts = fixture.counts.lock().unwrap();
        counts.external_status = None;
        counts.external_plain_only = true;
    }
    click(window, id, &view.inner.external, false);
    pump_until(|| fixture.counts.lock().unwrap().external == 4 && !view.inner.lyrics_loading.get());
    assert_eq!(view.inner.lyrics.borrow().as_ref().unwrap().plain, plain);
    assert_eq!(view.inner.source.text(), "Источник: Яндекс Музыка");

    // Revoking the saved preference takes effect for the very next selection.
    let mut values = this.persistence.borrow().values.clone();
    values.lyrics_external_auto = false;
    this.preferences_changed(values);
    assert!(!view.inner.auto_external.get());
    this.skip_music(true, false);
    pump_until(|| {
        view.inner
            .selected
            .borrow()
            .as_ref()
            .is_some_and(|t| t.id == "46")
            && !view.inner.lyrics_loading.get()
            && view.inner.lyrics.borrow().is_some()
    });
    request_spacing();
    assert_eq!(fixture.counts.lock().unwrap().external, 4);
    assert!(!view.inner.synchronized.get());

    // A once-only consented reply cannot replace a newer selection.
    {
        let mut counts = fixture.counts.lock().unwrap();
        counts.external_plain_only = false;
        counts.external_held = true;
        counts.external_entered = false;
    }
    *fixture.gate.0.lock().unwrap() = false;
    click(window, id, &view.inner.external, false);
    let dialog = external_consent_ready(window, view);
    assert!(!find_check(dialog.upcast_ref()).unwrap().is_active());
    click(
        window,
        id,
        &find_button(dialog.upcast_ref(), "Искать").unwrap(),
        false,
    );
    external_consent_closed(view, &dialog);
    pump_until(|| fixture.counts.lock().unwrap().external_entered);
    this.skip_music(false, false);
    fixture.release();
    pump_until(|| {
        view.inner
            .selected
            .borrow()
            .as_ref()
            .is_some_and(|t| t.id == "44")
            && !view.inner.lyrics_loading.get()
            && view
                .inner
                .lyrics
                .borrow()
                .as_ref()
                .is_some_and(|l| l.plain.contains("text 44"))
    });
    assert!(!view.inner.synchronized.get());
    assert!(!this.persistence.borrow().values.lyrics_external_auto);
    assert_eq!(fixture.counts.lock().unwrap().external, 5);

    // A saved permission can finish loading after TEXT is already displayed.
    // Deliver it while the controller model is borrowed, as in a render, and
    // require a deferred request for the same selection without reopening.
    request_spacing();
    fixture.counts.lock().unwrap().external_held = false;
    this.persistence.borrow_mut().values.lyrics_external_auto = true;
    {
        let _model = this.model.borrow_mut();
        view.set_external_preference(true);
        assert_eq!(fixture.counts.lock().unwrap().external, 5);
        assert!(!view.inner.lyrics_loading.get());
    }
    pump_until(|| view.inner.synchronized.get() && !view.inner.lyrics_loading.get());
    assert_eq!(view.inner.selected.borrow().as_ref().unwrap().id, "44");
    assert!(view.inner.external_consent.borrow().is_none());
    assert_eq!(fixture.counts.lock().unwrap().external, 6);
    // The static capture helper disables desktop motion while photographing.
    // Preserve the real desktop settings for the ordinary playback test next.
    let settings = window.settings();
    let animations = settings.is_gtk_enable_animations();
    let style = adw::StyleManager::default();
    let scheme = style.color_scheme();
    capture_native("v08-provider-text-consent-lrc-preference");
    settings.set_gtk_enable_animations(animations);
    style.set_color_scheme(scheme);
    ui.close_for_smoke()(true);
}
