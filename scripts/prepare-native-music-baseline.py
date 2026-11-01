#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-or-later
"""Extend only a9eed45's test harness; keep its production UI unmodified.

The redesigned panel API cannot be copied into the old application. This
adapter drives the archived application's real original controls instead and
uses identical outer geometries, synthetic data, themes and scale factors.
The old Queue is intentionally captured in its real central-page location.
"""

from pathlib import Path
import sys


root = Path(sys.argv[1])
fixture = root / "src/ui/music/design_tests.rs"
source = fixture.read_text()
setup_start = source.index("    let fixture = Fixture::new();", source.index("fn native_design_matched_matrix_and_populated_surfaces()"))
setup_end = source.index("    // Four geometries × open/closed", setup_start)
motion_setup = source[setup_start:setup_end]
start = source.index("    // Four geometries × open/closed")
end = source.index('    this.widgets.stack.set_visible_child_name("home");', start)
replacement = '''    // Test-only extended geometry adapter for the exact a9eed45 baseline.
    // Its Queue is a central route; do not pretend it already had a side mode.
    for (width, height, name) in [
        (420, 720, "compact"),
        (720, 650, "narrow"),
        (900, 650, "medium"),
        (1280, 800, "desktop"),
        (1440, 900, "wide"),
        (1650, 930, "owner-wide"),
        (1920, 1080, "large"),
    ] {
        set_pane(&ui, false);
        resize(&ui, width, height);
        for mode in ["closed", "track", "queue"] {
            this.widgets.stack.set_visible_child_name(if mode == "queue" { "queue" } else { "library" });
            set_pane(&ui, mode == "track");
            settle();
            evidence(&ui, &format!("matrix-{name}-panel-{mode}"));
        }
    }
    set_pane(&ui, false);
    ui.window.maximize();
    pump(|| ui.window.is_maximized());
    settle_geometry(&ui, "maximized");
    for mode in ["closed", "track", "queue"] {
        this.widgets.stack.set_visible_child_name(if mode == "queue" { "queue" } else { "library" });
        set_pane(&ui, mode == "track");
        settle();
        evidence(&ui, &format!("matrix-maximized-panel-{mode}"));
    }
    set_pane(&ui, false);
    ui.window.unmaximize();
    pump(|| !ui.window.is_maximized());
    resize(&ui, 1440, 900);

'''
source = source[:start] + replacement + source[end:]
source = source.replace('"pane_open":pane_open(&ui.music.controller),',
                        '"pane_open":pane_open(&ui.music.controller),"renderer":ui.window.renderer().unwrap().type_().name(),')
motion_helpers = r'''

// Geometry-only observation of the original GtkRevealer, with actual native
// motion enabled. A rejected baseline is measured, not declared acceptable.
struct BaselineMotionFrame {
    time: i64,
    requested: bool,
    node: gtk::gsk::RenderNode,
    host: gtk::graphene::Rect,
    surface: Option<gtk::graphene::Rect>,
    children: Vec<Option<gtk::graphene::Rect>>,
}
fn baseline_motion(ui: &crate::ui::MainWindow, reveal: &gtk::Revealer, opening: bool) {
    let surface = ui.music.controller.track_pane.widget.clone().upcast::<gtk::Widget>();
    let mut probes = vec![ui.music.controller.track_pane.close.clone().upcast::<gtk::Widget>()];
    for name in ["music-track-pane-cover","music-track-pane-title","music-track-pane-artist","music-track-pane-actions"] {
        probes.push(descendants(&surface).into_iter().find(|widget| widget.widget_name()==name).unwrap());
    }
    let frames = Rc::new(RefCell::new(Vec::new()));
    let captured = frames.clone();
    let window = ui.window.clone();
    let host = reveal.clone();
    let clock = window.frame_clock().unwrap();
    let handler = clock.connect_after_paint(move |clock| {
        // Fresh observers synchronously read this completed native frame,
        // bypassing WidgetPaintable's deferred pending-image publication.
        let paintable=gtk::WidgetPaintable::new(Some(&window));
        let snapshot=gtk::Snapshot::new();
        paintable.snapshot(&snapshot,f64::from(window.width()),f64::from(window.height()));
        if let Some(node)=snapshot.to_node() {
            captured.borrow_mut().push(BaselineMotionFrame { time:clock.frame_time(),requested:host.reveals_child(),node,
                host:host.compute_bounds(&window).unwrap_or_else(||gtk::graphene::Rect::new(0.0,0.0,0.0,0.0)),
                surface:surface.compute_bounds(&window),children:probes.iter().map(|widget|widget.compute_bounds(&window)).collect() });
        }
    });
    ui.window.queue_draw();
    pump(|| !frames.borrow().is_empty());
    gtk::prelude::WidgetExt::activate_action(&ui.window,"win.music-info",None).unwrap();
    let end = Instant::now()+Duration::from_millis(u64::from(reveal.transition_duration())+100);
    pump(|| Instant::now()>=end);
    clock.disconnect(handler);
    let frames=frames.borrow();
    assert!(frames.len()>=5,"old native motion must expose intermediate frames");
    let start=frames.iter().find(|frame|frame.requested==opening).unwrap().time;
    let duration=i64::from(reveal.transition_duration())*1000;
    let direction=if opening {"opening"} else {"closing"};
    let Some(directory)=std::env::var_os("KARIN_TEST_CAPTURE_DIR") else {return;};
    let directory=std::path::PathBuf::from(directory);
    std::fs::create_dir_all(&directory).unwrap();
    let scale=ui.window.scale_factor();
    let mut manifest=OpenOptions::new().create(true).append(true).open(directory.join(format!("baseline-panel-motion-scale{scale}.jsonl"))).unwrap();
    for frame in frames.iter() {
        let rectangles=frame.children.iter().map(|rect|rect.map(|rect|[rect.x(),rect.y(),rect.width(),rect.height()])).collect::<Vec<_>>();
        writeln!(manifest,"{}",json!({"direction":direction,"frame_time":frame.time,"elapsed_fraction":(frame.time-start) as f64/duration as f64,
            "animations_enabled":true,"renderer":ui.window.renderer().unwrap().type_().name(),"host":[frame.host.x(),frame.host.y(),frame.host.width(),frame.host.height()],
            "surface":frame.surface.map(|rect|[rect.x(),rect.y(),rect.width(),rect.height()]),"children":rectangles,
            "input":"original-window-info-action","source":"original-a9eed45-GtkRevealer-after-paint-geometry"})).unwrap();
    }
    for percent in [0,25,50,75,100] {
        let target=start+duration*i64::from(percent)/100;
        let frame=frames.iter().min_by_key(|frame|(frame.time-target).abs()).unwrap();
        let snapshot=gtk::Snapshot::new();
        snapshot.scale(scale as f32,scale as f32);
        snapshot.append_node(&frame.node);
        let node=snapshot.to_node().unwrap();
        let texture=ui.window.renderer().unwrap().render_texture(&node,Some(&gtk::graphene::Rect::new(0.0,0.0,(ui.window.width()*scale) as f32,(ui.window.height()*scale) as f32)));
        texture.save_to_png(directory.join(format!("before-animation-{direction}-{percent:03}-scale{scale}.png"))).unwrap();
    }
}

#[test]
#[ignore="observation of rejected a9eed45 GtkRevealer motion, isolated native fixture"]
fn native_baseline_partial_surface_motion_probe() {
    gtk::init().unwrap();
    adw::init().unwrap();
    let _manager=WindowManager::start();
'''
motion_body = r'''
    let _album = album;
    let actual=ui.window.renderer().unwrap().type_().name().to_string();
    eprintln!("Original baseline native renderer: {actual}");
    if std::env::var("GSK_RENDERER").as_deref()==Ok("ngl") {assert!(actual.to_ascii_lowercase().contains("glrenderer"));}
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    let settings=gtk::Settings::default().unwrap();
    settings.set_gtk_enable_animations(true);
    assert!(settings.is_gtk_enable_animations());
    let mut ancestor=this.track_pane.widget.parent();
    let reveal=loop {
        let widget=ancestor.expect("original revealer ancestor");
        if let Some(reveal)=widget.downcast_ref::<gtk::Revealer>() {break reveal.clone();}
        ancestor=widget.parent();
    };
    set_pane(&ui,false);
    baseline_motion(&ui,&reveal,true);
    baseline_motion(&ui,&reveal,false);
    ui.music.close();
    ui.window.close();
    settle();
}
'''
source += motion_helpers + motion_setup + motion_body
fixture.write_text(source)
