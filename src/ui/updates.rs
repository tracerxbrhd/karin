// SPDX-License-Identifier: GPL-3.0-or-later
//! Manual update checks and confirmed installation through the Flatpak portal.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use adw::prelude::*;
use gtk::{gio, glib};

use crate::{
    config,
    updates::{
        self, CheckError, CheckResult, Deployment, DeploymentStatus, Release,
        flatpak::{Availability, FlatpakMonitor, PortalError, PortalEvent, Progress},
    },
};

pub fn install_update_action(app: &adw::Application) {
    let controller = Rc::new(UpdateController::new(app));
    let weak = Rc::downgrade(&controller);
    app.connect_startup(move |_| {
        // The portal's normal discovery is asynchronous and has no synchronous
        // CheckNow method. Keep one monitor for a configured Flatpak while its
        // window is open, without querying GitHub until the user asks.
        if config::package_kind() == "Flatpak"
            && updates::channel_configured()
            && let Some(controller) = weak.upgrade()
        {
            controller.ensure_monitor();
        }
    });
    let action = gio::SimpleAction::new("check-updates", None);
    action.connect_activate(move |_, _| controller.show());
    app.add_action(&action);
}

struct UpdateController {
    app: glib::WeakRef<adw::Application>,
    dialog: RefCell<Option<UpdateDialog>>,
    phase: RefCell<Phase>,
    check: RefCell<Option<Arc<AtomicBool>>>,
    monitor: RefCell<Monitor>,
    monitor_generation: Cell<u64>,
    install_deadline: RefCell<Option<glib::SourceId>>,
}

#[derive(Clone)]
enum Phase {
    Checking,
    Latest,
    CheckFailed(CheckError),
    Offer(Release),
    Installing(Deployment, Option<f64>),
    Installed(Deployment),
    Restarting,
    InstallFailed(PortalError),
    InstallTimeout,
    OpeningLink(Release),
    LinkFailed(Release),
}

enum Monitor {
    Idle,
    Connecting,
    Ready(Rc<FlatpakMonitor>),
    Failed(PortalError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Primary {
    Check,
    OpenRelease,
    Install,
    Restart,
}

impl Primary {
    fn label(self) -> &'static str {
        match self {
            Self::Check => "Повторить",
            Self::OpenRelease => "Открыть страницу выпуска",
            Self::Install => "Обновить и перезапустить",
            Self::Restart => "Перезапустить",
        }
    }
}

#[derive(Clone)]
struct UpdateDialog {
    dialog: adw::Dialog,
    heading: gtk::Label,
    body: gtk::Label,
    notes: gtk::Label,
    notes_scroll: gtk::ScrolledWindow,
    progress: gtk::ProgressBar,
    spinner: gtk::Spinner,
    primary: gtk::Button,
    secondary: gtk::Button,
}

struct Presentation {
    heading: &'static str,
    body: String,
    notes: String,
    primary: Option<Primary>,
    secondary: Option<&'static str>,
    busy: bool,
    progress: Option<f64>,
    can_close: bool,
}

impl Presentation {
    fn message(heading: &'static str, body: impl Into<String>) -> Self {
        Self {
            heading,
            body: body.into(),
            notes: String::new(),
            primary: None,
            secondary: Some("Закрыть"),
            busy: false,
            progress: None,
            can_close: true,
        }
    }

    fn with_action(mut self, action: Primary) -> Self {
        self.primary = Some(action);
        self
    }
}

impl UpdateController {
    fn new(app: &adw::Application) -> Self {
        Self {
            app: app.downgrade(),
            dialog: RefCell::new(None),
            phase: RefCell::new(Phase::Checking),
            check: RefCell::new(None),
            monitor: RefCell::new(Monitor::Idle),
            monitor_generation: Cell::new(0),
            install_deadline: RefCell::new(None),
        }
    }

    fn show(self: &Rc<Self>) {
        let Some(window) = self.app.upgrade().and_then(|app| app.active_window()) else {
            return;
        };
        if self.dialog.borrow().is_some() {
            return;
        }
        let dialog = UpdateDialog::new();
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_closed(move |_| {
            if let Some(this) = weak.upgrade() {
                this.dialog_closed();
            }
        });
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_close_attempt(move |dialog| {
            if let Some(this) = weak.upgrade()
                && this.cancel_install()
            {
                dialog.force_close();
            }
        });
        let weak = Rc::downgrade(self);
        dialog.primary.connect_clicked(move |_| {
            if let Some(this) = weak.upgrade() {
                this.activate_primary();
            }
        });
        let weak_dialog = dialog.dialog.downgrade();
        let weak = Rc::downgrade(self);
        dialog.secondary.connect_clicked(move |_| {
            let cancelled_installation = weak.upgrade().is_some_and(|this| this.cancel_install());
            if let Some(dialog) = weak_dialog.upgrade() {
                if cancelled_installation {
                    dialog.force_close();
                } else {
                    dialog.close();
                }
            }
        });
        self.dialog.replace(Some(dialog.clone()));
        self.begin_check();
        dialog.dialog.present(Some(&window));
    }

    fn begin_check(self: &Rc<Self>) {
        self.set_phase(Phase::Checking);
        if self.check.borrow().is_some() {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.check.replace(Some(cancelled.clone()));
        let weak = Rc::downgrade(self);
        let flatpak = config::package_kind() == "Flatpak";
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || updates::check(&cancelled, flatpak))
                .await
                .unwrap_or(Err(CheckError::Internal));
            let Some(this) = weak.upgrade() else {
                return;
            };
            let was_cancelled = this
                .check
                .borrow_mut()
                .take()
                .is_some_and(|flag| flag.load(Ordering::Relaxed));
            if this.dialog.borrow().is_none() {
                return;
            }
            if was_cancelled {
                // Reopening while the cancelled request was draining could not
                // start another worker. Take that pending check now.
                this.begin_check();
                return;
            }
            match result {
                Ok(CheckResult::UpToDate) => this.set_phase(Phase::Latest),
                Ok(CheckResult::Available(release)) => {
                    let supported = matches!(release.deployment, DeploymentStatus::Verified(_));
                    if supported {
                        this.ensure_monitor();
                    }
                    this.set_phase(Phase::Offer(*release));
                }
                Err(error) => this.set_phase(Phase::CheckFailed(error)),
            }
        });
    }

    fn ensure_monitor(self: &Rc<Self>) {
        let open = match &*self.monitor.borrow() {
            Monitor::Connecting => true,
            Monitor::Ready(monitor) => monitor.is_open(),
            Monitor::Idle | Monitor::Failed(_) => false,
        };
        if open {
            return;
        }
        self.monitor.replace(Monitor::Connecting);
        let generation = self.next_monitor_generation();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let weak_event = weak.clone();
            let result = FlatpakMonitor::new(move |event| {
                if let Some(this) = weak_event
                    .upgrade()
                    .filter(|this| this.monitor_generation.get() == generation)
                {
                    this.portal_event(event);
                }
            })
            .await;
            if let Some(this) = weak
                .upgrade()
                .filter(|this| this.monitor_generation.get() == generation)
            {
                let monitor = match result {
                    Ok(monitor) => Monitor::Ready(Rc::new(monitor)),
                    Err(error) => Monitor::Failed(error),
                };
                this.monitor.replace(monitor);
                this.render();
            }
        });
    }

    fn next_monitor_generation(&self) -> u64 {
        let generation = self.monitor_generation.get().wrapping_add(1);
        self.monitor_generation.set(generation);
        generation
    }

    fn dialog_closed(&self) {
        self.dialog.borrow_mut().take();
        if let Some(cancelled) = self.check.borrow().as_ref() {
            cancelled.store(true, Ordering::Relaxed);
        }
        self.cancel_install();
    }

    fn cancel_install(&self) -> bool {
        if !matches!(*self.phase.borrow(), Phase::Installing(..)) {
            return false;
        }
        self.clear_install_deadline();
        // The portal owns cancellation and rollback. Do not restart even
        // if its operation happens to reach success before Close wins.
        self.retire_monitor();
        self.phase.replace(Phase::Checking);
        true
    }

    fn retire_monitor(&self) {
        self.next_monitor_generation();
        if let Monitor::Ready(monitor) = self.monitor.replace(Monitor::Idle) {
            monitor.close();
        }
    }

    fn clear_install_deadline(&self) {
        if let Some(deadline) = self.install_deadline.borrow_mut().take() {
            deadline.remove();
        }
    }

    fn ready_monitor(&self) -> Option<Rc<FlatpakMonitor>> {
        match &*self.monitor.borrow() {
            Monitor::Ready(monitor) => Some(monitor.clone()),
            _ => None,
        }
    }

    fn activate_primary(self: &Rc<Self>) {
        let phase = self.phase.borrow().clone();
        match self.presentation().primary {
            Some(Primary::Check) => self.begin_check(),
            Some(Primary::OpenRelease) => {
                if let Phase::Offer(release) | Phase::LinkFailed(release) = phase {
                    self.open_release(release);
                }
            }
            Some(Primary::Install | Primary::Restart) => {
                if let Phase::Offer(Release {
                    deployment: DeploymentStatus::Verified(deployment),
                    ..
                }) = phase
                    && let Some(monitor) = self.ready_monitor()
                {
                    match monitor.availability(&deployment) {
                        Availability::Install => self.install(monitor, deployment),
                        Availability::Restart => self.restart(deployment),
                        Availability::Waiting => self.render(),
                    }
                }
            }
            None => {}
        }
    }

    fn open_release(self: &Rc<Self>, release: Release) {
        let Some(window) = self.app.upgrade().and_then(|app| app.active_window()) else {
            return;
        };
        self.set_phase(Phase::OpeningLink(release.clone()));
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let opened = gtk::UriLauncher::new(&release.page)
                .launch_future(Some(&window))
                .await
                .is_ok();
            if let Some(this) = weak.upgrade() {
                let current =
                    matches!(&*this.phase.borrow(), Phase::OpeningLink(value) if *value == release);
                if current {
                    this.set_phase(if opened {
                        Phase::Offer(release)
                    } else {
                        Phase::LinkFailed(release)
                    });
                }
            }
        });
    }

    fn install(self: &Rc<Self>, monitor: Rc<FlatpakMonitor>, deployment: Deployment) {
        self.set_phase(Phase::Installing(deployment.clone(), None));
        let generation = self.monitor_generation.get();
        let weak = Rc::downgrade(self);
        let deadline = glib::timeout_add_local_once(Duration::from_secs(600), move || {
            if let Some(this) = weak.upgrade() {
                this.install_deadline.borrow_mut().take();
                this.cancel_install();
                this.set_phase(Phase::InstallTimeout);
            }
        });
        self.install_deadline.replace(Some(deadline));
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = monitor.install(&deployment, "").await;
            if let Err(error) = result
                && let Some(this) = weak
                    .upgrade()
                    .filter(|this| this.monitor_generation.get() == generation)
            {
                let still_installing = matches!(*this.phase.borrow(), Phase::Installing(..));
                if still_installing {
                    this.retire_monitor();
                    this.set_phase(Phase::InstallFailed(error));
                }
            }
        });
    }

    fn portal_event(self: &Rc<Self>, event: PortalEvent) {
        match event {
            PortalEvent::OfferChanged => self.render(),
            PortalEvent::Disconnected => {
                self.next_monitor_generation();
                self.monitor
                    .replace(Monitor::Failed(PortalError::Unavailable));
                if matches!(
                    *self.phase.borrow(),
                    Phase::Installing(..) | Phase::Installed(_) | Phase::Restarting
                ) {
                    self.set_phase(Phase::InstallFailed(PortalError::Unavailable));
                } else {
                    self.render();
                }
            }
            PortalEvent::Progress(progress) => {
                let Phase::Installing(deployment, _) = self.phase.borrow().clone() else {
                    return;
                };
                match progress {
                    Progress::Running(fraction) => {
                        self.set_phase(Phase::Installing(deployment, fraction))
                    }
                    Progress::Empty => self.set_phase(Phase::InstallFailed(PortalError::Stale)),
                    Progress::Failed(error) => self.set_phase(Phase::InstallFailed(error)),
                    Progress::Done => {
                        self.set_phase(Phase::Installed(deployment.clone()));
                        let weak = Rc::downgrade(self);
                        glib::idle_add_local_once(move || {
                            if let Some(this) = weak.upgrade() {
                                let ready = this.dialog.borrow().is_some()
                                    && matches!(&*this.phase.borrow(), Phase::Installed(target) if *target == deployment);
                                if ready {
                                    this.restart(deployment);
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    fn restart(self: &Rc<Self>, deployment: Deployment) {
        let Some(monitor) = self.ready_monitor() else {
            self.set_phase(Phase::InstallFailed(PortalError::Unavailable));
            return;
        };
        self.set_phase(Phase::Restarting);
        let generation = self.monitor_generation.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = monitor.restart(&deployment).await;
            let Some(this) = weak
                .upgrade()
                .filter(|this| this.monitor_generation.get() == generation)
            else {
                return;
            };
            match result {
                Ok(handoff) => {
                    if let Some(app) = this.app.upgrade()
                        && let Err(error) = handoff.close_application(&app)
                    {
                        this.set_phase(Phase::InstallFailed(error));
                    }
                }
                Err(error) => this.set_phase(Phase::InstallFailed(error)),
            }
        });
    }

    fn set_phase(&self, phase: Phase) {
        if !matches!(phase, Phase::Installing(..)) {
            self.clear_install_deadline();
        }
        self.phase.replace(phase);
        self.render();
    }

    fn render(&self) {
        let dialog = self.dialog.borrow().clone();
        if let Some(dialog) = dialog {
            dialog.render(self.presentation());
        }
    }

    fn presentation(&self) -> Presentation {
        let phase = self.phase.borrow();
        match &*phase {
            Phase::Checking => {
                let mut view = Presentation::message("Проверка обновлений", "Получаем сведения о последнем стабильном выпуске…");
                view.busy = true;
                view.secondary = Some("Отменить");
                view
            }
            Phase::Latest => Presentation::message("Установлена актуальная версия", format!("Karin {}", config::VERSION)),
            Phase::CheckFailed(error) => Presentation::message("Не удалось проверить обновления", error_text(*error)).with_action(Primary::Check),
            Phase::Offer(release) => self.present_release(release),
            Phase::OpeningLink(release) => {
                let mut view = self.present_release(release);
                view.primary = None;
                view.busy = true;
                view
            }
            Phase::LinkFailed(release) => {
                let mut view = self.present_release(release);
                view.body.push_str("\n\nНе удалось открыть страницу в браузере.");
                view
            }
            Phase::Installing(_, progress) => {
                let mut view = Presentation::message("Загрузка и установка", "Flatpak устанавливает новую версию Karin.");
                view.progress = *progress;
                view.busy = progress.is_none();
                view.secondary = Some("Отменить");
                view.can_close = false;
                view
            }
            Phase::Installed(target) => {
                let mut view = Presentation::message("Обновление установлено", format!("Karin {} готов к перезапуску.", target.version));
                view.can_close = false;
                view.secondary = None;
                view
            }
            Phase::Restarting => {
                let mut view = Presentation::message("Обновление установлено", "Перезапускаем Karin…");
                view.busy = true;
                view.can_close = false;
                view.secondary = None;
                view
            }
            Phase::InstallFailed(error) => Presentation::message("Не удалось завершить обновление", error.to_string()).with_action(Primary::Check),
            Phase::InstallTimeout => Presentation::message("Не удалось завершить обновление", "Flatpak не завершил установку вовремя. Повторите проверку позже или воспользуйтесь системным менеджером приложений.").with_action(Primary::Check),
        }
    }

    fn present_release(&self, release: &Release) -> Presentation {
        let (action, explanation) = match &release.deployment {
            DeploymentStatus::Native => (Some(Primary::OpenRelease),
                "Для Native доступна ручная установка пакета со страницы выпуска.".into()),
            DeploymentStatus::ChannelNotConfigured => (Some(Primary::OpenRelease),
                "Для этой сборки не настроен подписанный канал обновлений Flatpak.".into()),
            DeploymentStatus::NotPublished => (Some(Primary::OpenRelease),
                "Эта версия ещё не доступна в канале обновлений Flatpak.".into()),
            DeploymentStatus::Unverified(error) => (Some(Primary::Check),
                format!("Не удалось подтвердить обновление Flatpak. {}", error_text(*error))),
            DeploymentStatus::Verified(target) => match &*self.monitor.borrow() {
                Monitor::Idle | Monitor::Connecting => (None, "Подключаемся к службе обновлений Flatpak…".into()),
                Monitor::Failed(error) => (Some(Primary::Check), error.to_string()),
                Monitor::Ready(monitor) => match monitor.availability(target) {
                    Availability::Waiting => (Some(Primary::OpenRelease),
                        "Flatpak ещё не подтвердил доступность этой версии для установленного пакета. Проверьте источник Karin в системном менеджере приложений.".into()),
                    Availability::Install => (Some(Primary::Install), String::new()),
                    Availability::Restart => (Some(Primary::Restart), "Версия уже установлена. Перезапустите Karin.".into()),
                },
            },
        };
        let mut view = Presentation::message(
            "Доступно обновление Karin",
            format!(
                "Установлена: {}\nДоступна: {}",
                config::VERSION,
                release.version
            ),
        );
        if !explanation.is_empty() {
            view.body.push_str("\n\n");
            view.body.push_str(&explanation);
        }
        view.primary = action;
        view.secondary = Some("Позже");
        view.notes = release.notes.clone();
        view
    }
}

impl Drop for UpdateController {
    fn drop(&mut self) {
        self.clear_install_deadline();
        if let Some(cancelled) = self.check.get_mut().take() {
            cancelled.store(true, Ordering::Relaxed);
        }
        if let Monitor::Ready(monitor) = self.monitor.get_mut() {
            monitor.close();
        }
    }
}

impl UpdateDialog {
    fn new() -> Self {
        let dialog = adw::Dialog::builder()
            .title("Обновления Karin")
            .content_width(435)
            .build();
        dialog.set_widget_name("karin-updates");
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
        super::margins(&content, 18);
        let heading = wrapped_label();
        heading.add_css_class("title-2");
        let body = wrapped_label();
        let notes = wrapped_label();
        notes.set_selectable(true);
        let notes_scroll = gtk::ScrolledWindow::builder()
            .propagate_natural_height(true)
            .max_content_height(170)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&notes)
            .build();
        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        let spinner = gtk::Spinner::new();
        spinner.set_halign(gtk::Align::Center);
        content.append(&heading);
        content.append(&body);
        content.append(&notes_scroll);
        content.append(&spinner);
        content.append(&progress);
        let scroll = gtk::ScrolledWindow::builder()
            .propagate_natural_height(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&content)
            .build();
        toolbar.set_content(Some(&scroll));
        let actions = gtk::Box::new(gtk::Orientation::Vertical, 8);
        super::margins(&actions, 12);
        let primary = gtk::Button::new();
        primary.add_css_class("suggested-action");
        let secondary = gtk::Button::new();
        actions.append(&primary);
        actions.append(&secondary);
        toolbar.add_bottom_bar(&actions);
        dialog.set_child(Some(&toolbar));
        dialog.set_focus(Some(&secondary));
        Self {
            dialog,
            heading,
            body,
            notes,
            notes_scroll,
            progress,
            spinner,
            primary,
            secondary,
        }
    }

    fn render(&self, view: Presentation) {
        self.heading.set_text(view.heading);
        self.body.set_text(&view.body);
        self.notes.set_text(&view.notes);
        self.notes_scroll.set_visible(!view.notes.is_empty());
        self.spinner.set_visible(view.busy);
        self.spinner.set_spinning(view.busy);
        self.progress.set_visible(view.progress.is_some());
        if let Some(progress) = view.progress {
            self.progress.set_fraction(progress.clamp(0.0, 1.0));
        }
        self.primary.set_visible(view.primary.is_some());
        if let Some(primary) = view.primary {
            self.primary.set_label(primary.label());
        }
        self.secondary.set_visible(view.secondary.is_some());
        if let Some(secondary) = view.secondary {
            self.secondary.set_label(secondary);
        }
        self.dialog.set_can_close(view.can_close);
    }
}

fn wrapped_label() -> gtk::Label {
    gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build()
}

fn error_text(error: CheckError) -> String {
    match error {
        CheckError::Cancelled => "Проверка отменена.".into(),
        CheckError::Network => "Нет связи с GitHub. Проверьте подключение к сети.".into(),
        CheckError::Timeout => "GitHub не ответил вовремя. Повторите попытку позже.".into(),
        CheckError::RateLimited => {
            "GitHub ограничил частоту запросов. Повторите попытку позже.".into()
        }
        CheckError::Http(status) => format!("GitHub вернул ошибку HTTP {status}."),
        CheckError::InvalidData => {
            "GitHub вернул неполные или некорректные сведения о выпуске.".into()
        }
        CheckError::Internal => "Не удалось подготовить проверку обновлений.".into(),
    }
}
