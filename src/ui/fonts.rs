// SPDX-License-Identifier: GPL-3.0-or-later
//! Process-local, offline fonts. The embedded upstream files stay unmodified;
//! sealed anonymous descriptors avoid font installation or persistent copies.

use std::{
    cell::Cell,
    ffi::{CStr, CString, c_int, c_void},
    fs::File,
    io::Write,
    os::fd::AsRawFd,
    sync::OnceLock,
};

use gtk::{glib::translate::ToGlibPtr, prelude::*};
use rustix::fs::{MemfdFlags, SealFlags, fcntl_add_seals, memfd_create};

const FONTS: [(&str, &[u8]); 2] = [
    (
        "karin-montserrat",
        include_bytes!("../../resources/fonts/Montserrat-VF.ttf"),
    ),
    (
        "karin-inter",
        include_bytes!("../../resources/fonts/InterVariable.ttf"),
    ),
];

/// Fixed startup categories only; no filesystem paths or user data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontError {
    Descriptor,
    Registration,
    Backend,
    Rendering,
}

impl std::fmt::Display for FontError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Descriptor => "bundled font descriptor unavailable",
            Self::Registration => "bundled font registration unavailable",
            Self::Backend => "bundled font backend unavailable",
            Self::Rendering => "bundled font rendering unavailable",
        })
    }
}

impl std::error::Error for FontError {}

struct RegisteredFonts {
    // Fontconfig/FreeType may reopen a face for later glyphs. These descriptors
    // must outlive every GTK window, not just FcConfigAppFontAddFile.
    files: Vec<File>,
    usable: bool,
}

static REGISTERED: OnceLock<Result<RegisteredFonts, FontError>> = OnceLock::new();

#[link(name = "fontconfig")]
unsafe extern "C" {
    #[link_name = "FcConfigGetCurrent"]
    fn fc_config_get_current() -> *mut c_void;
    #[link_name = "FcConfigAppFontAddFile"]
    fn fc_config_app_font_add_file(config: *mut c_void, file: *const u8) -> c_int;
    #[link_name = "FcConfigBuildFonts"]
    fn fc_config_build_fonts(config: *mut c_void) -> c_int;
    #[link_name = "FcPatternGetString"]
    fn fc_pattern_get_string(
        pattern: *const c_void,
        object: *const std::ffi::c_char,
        index: c_int,
        value: *mut *mut u8,
    ) -> c_int;
}

#[link(name = "pangocairo-1.0")]
unsafe extern "C" {
    fn pango_cairo_font_map_get_default() -> *mut gtk::pango::ffi::PangoFontMap;
}

#[link(name = "pangoft2-1.0")]
unsafe extern "C" {
    fn pango_fc_font_map_get_type() -> gtk::glib::ffi::GType;
    fn pango_fc_font_map_config_changed(fontmap: *mut c_void);
    fn pango_fc_font_get_type() -> gtk::glib::ffi::GType;
    fn pango_fc_font_get_pattern(font: *mut c_void) -> *mut c_void;
}

fn register() -> Result<RegisteredFonts, FontError> {
    // Finish allocating/sealing both resources before changing Fontconfig.
    let mut files = Vec::with_capacity(FONTS.len());
    for (name, bytes) in FONTS {
        let fd = memfd_create(name, MemfdFlags::CLOEXEC | MemfdFlags::ALLOW_SEALING)
            .map_err(|_| FontError::Descriptor)?;
        let mut file = File::from(fd);
        file.write_all(bytes).map_err(|_| FontError::Descriptor)?;
        fcntl_add_seals(
            &file,
            SealFlags::WRITE | SealFlags::GROW | SealFlags::SHRINK | SealFlags::SEAL,
        )
        .map_err(|_| FontError::Descriptor)?;
        files.push(file);
    }
    // Fontconfig owns this borrowed current configuration. No application-font
    // clear or system/default configuration replacement is performed.
    let config = unsafe { fc_config_get_current() };
    if config.is_null() {
        return Err(FontError::Registration);
    }
    let mut usable = true;
    for file in &files {
        let path = CString::new(format!("/proc/self/fd/{}", file.as_raw_fd()))
            .map_err(|_| FontError::Descriptor)?;
        // `path` is NUL-terminated, and Fontconfig copies the path while the
        // referenced immutable file remains owned by this process-wide guard.
        usable &= unsafe { fc_config_app_font_add_file(config, path.as_ptr().cast()) != 0 };
    }
    usable &= unsafe { fc_config_build_fonts(config) != 0 };
    // Preserve descriptors even if Fontconfig admitted only one face, so a
    // partial registration never leaves it pointing at a closed/reused fd.
    Ok(RegisteredFonts { files, usable })
}

/// Call before GTK/application startup. Direct widget fixtures may call this
/// from style installation too; no disk files, network or global font install
/// are involved, and the bounded registration runs once per process.
pub(super) fn install() -> Result<(), FontError> {
    let registered = REGISTERED.get_or_init(register).as_ref().map_err(|e| *e)?;
    if !registered.usable {
        return Err(FontError::Registration);
    }
    thread_local! { static REFRESHED: Cell<bool> = const { Cell::new(false) }; }
    REFRESHED.with(|refreshed| {
        if refreshed.get() {
            return Ok(());
        }
        // Cairo's default font map is thread-local. Refresh the current one
        // after adding app fonts, including fixtures that initialized Pango
        // before creating their first Karin window.
        let fontmap = unsafe { pango_cairo_font_map_get_default() };
        if fontmap.is_null()
            || unsafe {
                gtk::glib::gobject_ffi::g_type_check_instance_is_a(
                    fontmap.cast(),
                    pango_fc_font_map_get_type(),
                )
            } == 0
        {
            return Err(FontError::Backend);
        }
        unsafe { pango_fc_font_map_config_changed(fontmap.cast()) };
        refreshed.set(true);
        Ok(())
    })
}

fn font_source(font: &gtk::pango::Font) -> Result<(String, String), FontError> {
    let ptr: *mut gtk::pango::ffi::PangoFont = font.to_glib_none().0;
    if unsafe {
        gtk::glib::gobject_ffi::g_type_check_instance_is_a(ptr.cast(), pango_fc_font_get_type())
    } == 0
    {
        return Err(FontError::Rendering);
    }
    let pattern = unsafe { pango_fc_font_get_pattern(ptr.cast()) };
    if pattern.is_null() {
        return Err(FontError::Rendering);
    }
    let get = |key: &CStr| {
        let mut value = std::ptr::null_mut();
        if unsafe { fc_pattern_get_string(pattern, key.as_ptr(), 0, &mut value) } != 0
            || value.is_null()
        {
            return Err(FontError::Rendering);
        }
        // The pattern and its strings are borrowed from the live `font`.
        unsafe { CStr::from_ptr(value.cast()) }
            .to_str()
            .map(str::to_owned)
            .map_err(|_| FontError::Rendering)
    };
    Ok((get(c"family")?, get(c"file")?))
}

const GLYPHS: &str = "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯабвгдеёжзийклмнопрстуфхцчшщъыьэюяABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789№…";

/// Existing smoke tests call this on the actual packaged application's Pango
/// context. A CSS family declaration or an OS-installed substitute cannot pass.
pub(super) fn verify(context: &gtk::pango::Context) -> Result<(), FontError> {
    install()?;
    let registered = REGISTERED
        .get()
        .ok_or(FontError::Registration)?
        .as_ref()
        .map_err(|error| *error)?;
    let sources: Vec<_> = registered
        .files
        .iter()
        .map(|file| format!("/proc/self/fd/{}", file.as_raw_fd()))
        .collect();
    for family in ["Montserrat", "Inter Variable"] {
        for weight in [
            gtk::pango::Weight::Normal,
            gtk::pango::Weight::Medium,
            gtk::pango::Weight::Semibold,
            gtk::pango::Weight::Bold,
        ] {
            let mut description = gtk::pango::FontDescription::new();
            description.set_family(family);
            description.set_weight(weight);
            description.set_size(12 * gtk::pango::SCALE);
            let font = context
                .load_font(&description)
                .ok_or(FontError::Rendering)?;
            let (actual_family, source) = font_source(&font)?;
            if actual_family != family
                || !sources.contains(&source)
                || font.describe().weight() != weight
                || !GLYPHS.chars().all(|ch| font.has_char(ch))
            {
                return Err(FontError::Rendering);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    fn pump_until(predicate: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let context = gtk::glib::MainContext::default();
        loop {
            if predicate() {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "native font fixture timed out"
            );
            if context.pending() {
                context.iteration(false);
            } else {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        }
    }

    #[test]
    #[ignore = "requires an isolated GTK display; run the registered native fixture"]
    fn native_bundled_typography() {
        install().unwrap();
        gtk::init().unwrap();
        adw::init().unwrap();
        crate::ui::style::install();
        let app = adw::Application::builder()
            .application_id("io.github.tracerxbrhd.Karin.FontFixture")
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(gtk::gio::Cancellable::NONE).unwrap();
        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_top(16);
        content.set_margin_bottom(16);
        content.set_margin_start(16);
        content.set_margin_end(16);
        let window = adw::ApplicationWindow::builder()
            .application(&app)
            .default_width(900)
            .default_height(500)
            .content(&content)
            .build();
        crate::ui::style::attach(&window);
        let css = gtk::CssProvider::new();
        css.load_from_string(
            ".karin-font-fixture-400 { font-weight: 400; }\n\
             .karin-font-fixture-500 { font-weight: 500; }\n\
             .karin-font-fixture-600 { font-weight: 600; }\n\
             .karin-font-fixture-700 { font-weight: 700; }",
        );
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        let mut labels = Vec::new();
        let sample = "Ёжик / Track №42 — Кириллица …";
        for (family, class) in [
            ("Montserrat", "karin-font-primary"),
            ("Inter Variable", "karin-font-secondary"),
        ] {
            for (weight, value) in [
                (gtk::pango::Weight::Normal, 400),
                (gtk::pango::Weight::Medium, 500),
                (gtk::pango::Weight::Semibold, 600),
                (gtk::pango::Weight::Bold, 700),
            ] {
                let label = gtk::Label::new(Some(sample));
                label.add_css_class(class);
                label.add_css_class(&format!("karin-font-fixture-{value}"));
                label.set_halign(gtk::Align::Start);
                content.append(&label);
                labels.push((label, family, weight));
            }
        }
        let long = gtk::Label::new(Some(&format!(
            "{} {}",
            sample.repeat(6),
            "Long mixed title"
        )));
        long.add_css_class("karin-font-primary");
        long.set_ellipsize(gtk::pango::EllipsizeMode::End);
        long.set_max_width_chars(24);
        long.set_halign(gtk::Align::Start);
        content.append(&long);
        window.present();
        pump_until(|| long.is_mapped() && long.width() > 0);
        verify(&window.pango_context()).unwrap();
        let files = &REGISTERED.get().unwrap().as_ref().unwrap().files;
        let sources: Vec<_> = files
            .iter()
            .map(|f| format!("/proc/self/fd/{}", f.as_raw_fd()))
            .collect();
        let mut records = Vec::new();
        for (label, expected_family, expected_weight) in &labels {
            let layout = label.layout();
            let mut iter = layout.iter();
            let mut seen = 0;
            loop {
                if let Some(run) = iter.run_readonly() {
                    let font = run.item().analysis().font();
                    let (family, source) = font_source(&font).unwrap();
                    assert_eq!(&family, expected_family);
                    assert!(
                        sources.contains(&source),
                        "the actual glyph font must be bundled, not a system substitute"
                    );
                    assert_eq!(font.describe().weight(), *expected_weight);
                    for ch in GLYPHS.chars() {
                        assert!(font.has_char(ch), "{expected_family} missing {ch}");
                    }
                    records.push(json!({"family":family,"weight":format!("{expected_weight:?}"),"bundled_source":true,"cyrillic":true}));
                    seen += 1;
                }
                if !iter.next_run() {
                    break;
                }
            }
            assert!(seen > 0);
        }
        assert!(long.layout().is_ellipsized());
        assert_eq!(long.layout().line_count(), 1);
        let settings = gtk::Settings::default().unwrap();
        let dpi: i32 = settings.property("gtk-xft-dpi");
        let before = labels[0].0.layout().pixel_size().1;
        settings.set_property("gtk-xft-dpi", dpi.max(96 * 1024) * 5 / 4);
        pump_until(|| labels[0].0.layout().pixel_size().1 > before);
        settings.set_property("gtk-xft-dpi", dpi);
        if let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(PathBuf::from(directory).join("bundled-fonts.json"),
                serde_json::to_vec_pretty(&json!({"scope":"real Pango glyph fonts from embedded sealed descriptors", "scale":window.scale_factor(),"records":records,"ellipsis":true,"system_text_scaling":true})).unwrap()).unwrap();
        }
        gtk::style_context_remove_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &css,
        );
        window.close();
    }
}
