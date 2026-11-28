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
        flatpak::{Availability, FlatpakMonitor, PortalError, PortalEvent, Progress, WaitReason},
    },
};

mod notes;

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
    release: RefCell<Option<Release>>,
    check: RefCell<Option<Arc<AtomicBool>>>,
    monitor: RefCell<Monitor>,
    monitor_generation: Cell<u64>,
    install_deadline: RefCell<Option<glib::SourceId>>,
}

#[derive(Clone)]
enum Phase {
    Checking { retry: bool },
    Latest,
    CheckFailed(CheckError),
    Offer,
    Installing(Deployment, Option<f64>),
    Verifying(Completion),
    Restarting,
    InstallFailed(PortalError),
    InstallTimeout,
    OpeningLink,
    LinkFailed(LinkTarget),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Completion {
    Installed,
    NoOperations,
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
    OpenSystemManager,
    Install,
    Restart,
}

impl Primary {
    fn label(self) -> &'static str {
        match self {
            Self::Check => "Проверить снова",
            Self::OpenRelease => "Открыть страницу выпуска",
            Self::OpenSystemManager => "Открыть в менеджере приложений",
            Self::Install => "Обновить и перезапустить",
            Self::Restart => "Перезапустить",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Secondary {
    Dismiss(&'static str),
    OpenRelease,
}

impl Secondary {
    fn label(self) -> &'static str {
        match self {
            Self::Dismiss(label) => label,
            Self::OpenRelease => "Страница выпуска",
        }
    }
}

#[derive(Clone, Copy)]
enum LinkTarget {
    Release,
    SystemManager,
}

#[derive(Clone)]
struct UpdateDialog {
    dialog: adw::Dialog,
    available_version: gtk::Label,
    heading: gtk::Label,
    body: gtk::Label,
    notes: gtk::Label,
    notes_block: gtk::Box,
    progress: gtk::ProgressBar,
    spinner: gtk::Spinner,
    primary: gtk::Button,
    secondary: gtk::Button,
}

struct Presentation {
    heading: &'static str,
    body: String,
    available: String,
    notes: String,
    primary: Option<Primary>,
    secondary: Option<Secondary>,
    busy: bool,
    progress: Option<f64>,
    can_close: bool,
}

impl Presentation {
    fn message(heading: &'static str, body: impl Into<String>) -> Self {
        Self {
            heading,
            body: body.into(),
            available: "Не определена".into(),
            notes: String::new(),
            primary: None,
            secondary: Some(Secondary::Dismiss("Закрыть")),
            busy: false,
            progress: None,
            can_close: true,
        }
    }

    fn with_action(mut self, action: Primary) -> Self {
        self.primary = Some(action);
        self
    }

    fn with_system_manager(mut self) -> Self {
        self.primary = Some(Primary::OpenSystemManager);
        self.secondary = Some(Secondary::OpenRelease);
        self
    }
}

impl UpdateController {
    fn new(app: &adw::Application) -> Self {
        Self {
            app: app.downgrade(),
            dialog: RefCell::new(None),
            phase: RefCell::new(Phase::Checking { retry: false }),
            release: RefCell::new(None),
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
        if self.dialog.borrow().is_some() || matches!(*self.phase.borrow(), Phase::Restarting) {
            return;
        }
        self.release.borrow_mut().take();
        self.set_phase(Phase::Checking { retry: false });
        self.present_dialog(&window);
        self.begin_check(false);
    }

    fn present_dialog(self: &Rc<Self>, window: &impl IsA<gtk::Widget>) {
        let dialog = UpdateDialog::new();
        let weak = Rc::downgrade(self);
        dialog.dialog.connect_closed(move |dialog| {
            if let Some(this) = weak.upgrade() {
                this.dialog_closed(dialog);
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
            let Some(this) = weak.upgrade() else {
                return;
            };
            if this.presentation().secondary == Some(Secondary::OpenRelease) {
                this.open_link(LinkTarget::Release);
                return;
            }
            let cancelled_installation = this.cancel_install();
            if let Some(dialog) = weak_dialog.upgrade() {
                if cancelled_installation {
                    dialog.force_close();
                } else {
                    dialog.close();
                }
            }
        });
        self.dialog.replace(Some(dialog.clone()));
        self.render();
        dialog.dialog.present(Some(window));
    }

    fn begin_check(self: &Rc<Self>, retry: bool) {
        self.set_phase(Phase::Checking { retry });
        if self.check.borrow().is_some() {
            return;
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        self.check.replace(Some(cancelled.clone()));
        let weak = Rc::downgrade(self);
        let flatpak = config::package_kind() == "Flatpak";
        glib::spawn_future_local(async move {
            let result = gio::spawn_blocking(move || {
                let mut result = updates::check(&cancelled, flatpak)?;
                if let CheckResult::Available(release) = &mut result {
                    release.notes = notes::excerpt(&release.notes);
                }
                Ok(result)
            })
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
                let retry = matches!(*this.phase.borrow(), Phase::Checking { retry: true });
                this.begin_check(retry);
                return;
            }
            match result {
                Ok(CheckResult::UpToDate) => {
                    this.release.borrow_mut().take();
                    this.set_phase(Phase::Latest);
                }
                Ok(CheckResult::Available(release)) => {
                    let supported = matches!(release.deployment, DeploymentStatus::Verified(_));
                    this.release.replace(Some(*release));
                    if supported {
                        this.ensure_monitor();
                    }
                    this.set_phase(Phase::Offer);
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

    fn dialog_closed(&self, dialog: &adw::Dialog) {
        let is_current = self
            .dialog
            .borrow()
            .as_ref()
            .is_some_and(|current| current.dialog == *dialog);
        if !is_current {
            return;
        }
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
        self.phase.replace(Phase::Checking { retry: false });
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

    fn arm_install_deadline(self: &Rc<Self>) {
        self.clear_install_deadline();
        let weak = Rc::downgrade(self);
        let deadline = glib::timeout_add_local_once(Duration::from_secs(600), move || {
            if let Some(this) = weak.upgrade() {
                this.install_deadline.borrow_mut().take();
                if this.cancel_install() {
                    this.set_phase(Phase::InstallTimeout);
                }
            }
        });
        self.install_deadline.replace(Some(deadline));
    }

    fn ready_monitor(&self) -> Option<Rc<FlatpakMonitor>> {
        match &*self.monitor.borrow() {
            Monitor::Ready(monitor) => Some(monitor.clone()),
            _ => None,
        }
    }

    fn activate_primary(self: &Rc<Self>) {
        match self.presentation().primary {
            Some(Primary::Check) => self.begin_check(true),
            Some(Primary::OpenRelease) => self.open_link(LinkTarget::Release),
            Some(Primary::OpenSystemManager) => self.open_link(LinkTarget::SystemManager),
            Some(Primary::Install | Primary::Restart) => {
                let deployment = self
                    .release
                    .borrow()
                    .as_ref()
                    .and_then(|release| match &release.deployment {
                        DeploymentStatus::Verified(deployment) => Some(deployment.clone()),
                        _ => None,
                    });
                if let (Some(deployment), Some(monitor)) = (deployment, self.ready_monitor()) {
                    match monitor.availability(&deployment) {
                        Availability::Install => self.install(monitor, deployment),
                        Availability::Restart => self.restart(deployment, None),
                        Availability::Waiting(_) => self.render(),
                    }
                }
            }
            None => {}
        }
    }

    fn open_link(self: &Rc<Self>, target: LinkTarget) {
        let Some(window) = self.app.upgrade().and_then(|app| app.active_window()) else {
            return;
        };
        let Some(uri) = self.release.borrow().as_ref().map(|release| match target {
            LinkTarget::Release => release.page.clone(),
            LinkTarget::SystemManager => format!("appstream:{}", config::APP_ID),
        }) else {
            return;
        };
        let owned_dialog = self
            .dialog
            .borrow()
            .as_ref()
            .map(|view| view.dialog.downgrade());
        self.set_phase(Phase::OpeningLink);
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = gtk::UriLauncher::new(&uri)
                .launch_future(Some(&window))
                .await;
            if let Some(this) = weak.upgrade() {
                let dialog = owned_dialog.and_then(|dialog| dialog.upgrade());
                let current = matches!(*this.phase.borrow(), Phase::OpeningLink)
                    && this
                        .dialog
                        .borrow()
                        .as_ref()
                        .is_some_and(|view| Some(&view.dialog) == dialog.as_ref());
                if current {
                    this.set_phase(match result {
                        Ok(()) => Phase::Offer,
                        Err(error)
                            if error.matches(gtk::DialogError::Dismissed)
                                || error.matches(gtk::DialogError::Cancelled)
                                || error.matches(gio::IOErrorEnum::Cancelled) =>
                        {
                            Phase::Offer
                        }
                        Err(_) => Phase::LinkFailed(target),
                    });
                }
            }
        });
    }

    fn install(self: &Rc<Self>, monitor: Rc<FlatpakMonitor>, deployment: Deployment) {
        self.set_phase(Phase::Installing(deployment.clone(), None));
        let generation = self.monitor_generation.get();
        self.arm_install_deadline();
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
                    if error == PortalError::ManagerRequired {
                        this.set_phase(Phase::Offer);
                    } else {
                        this.retire_monitor();
                        this.set_phase(Phase::InstallFailed(error));
                    }
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
                    Phase::Installing(..) | Phase::Verifying(_) | Phase::Restarting
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
                        // The portal can update a runtime as well. Bound a silent
                        // transaction instead of cancelling one still progressing.
                        self.arm_install_deadline();
                        self.set_phase(Phase::Installing(deployment, fraction));
                    }
                    Progress::Failed(PortalError::ManagerRequired) => self.set_phase(Phase::Offer),
                    Progress::Failed(error) => {
                        self.retire_monitor();
                        self.set_phase(Phase::InstallFailed(error));
                    }
                    Progress::Done | Progress::Empty => {
                        let completion = if progress == Progress::Done {
                            Completion::Installed
                        } else {
                            Completion::NoOperations
                        };
                        self.set_phase(Phase::Verifying(completion));
                        let weak = Rc::downgrade(self);
                        glib::idle_add_local_once(move || {
                            if let Some(this) = weak.upgrade() {
                                let ready = this.dialog.borrow().is_some()
                                    && matches!(*this.phase.borrow(), Phase::Verifying(current) if current == completion);
                                if ready {
                                    this.restart(deployment, Some(completion));
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    fn restart(self: &Rc<Self>, deployment: Deployment, completion: Option<Completion>) {
        let Some(monitor) = self.ready_monitor() else {
            self.set_phase(Phase::InstallFailed(PortalError::Unavailable));
            return;
        };
        let Some(dialog) = self
            .dialog
            .borrow()
            .as_ref()
            .map(|view| view.dialog.clone())
        else {
            return;
        };
        let Some(owner) = dialog.root().and_downcast::<adw::ApplicationWindow>() else {
            self.set_phase(Phase::InstallFailed(PortalError::RestartFailed));
            return;
        };
        self.set_phase(Phase::Restarting);
        let generation = self.monitor_generation.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = monitor.restart(&deployment).await;
            let Some(this) = weak
                .upgrade()
                .filter(|this| this.restart_is_current(generation))
            else {
                return;
            };
            let handoff = match result {
                Ok(handoff) => handoff,
                Err(error) => {
                    let error = if completion == Some(Completion::NoOperations)
                        && matches!(
                            error,
                            PortalError::Stale
                                | PortalError::InvalidIdentity
                                | PortalError::RestartFailed
                        ) {
                        PortalError::Stale
                    } else {
                        error
                    };
                    this.set_phase(Phase::InstallFailed(error));
                    return;
                }
            };

            // An AdwApplicationWindow with any open dialog consumes the first
            // close request. Dismissing our dialog must finish before app.quit
            // can run the normal window, player and persistence shutdown.
            if owner.dialogs().n_items() == 1 && owner.visible_dialog().as_ref() == Some(&dialog) {
                dialog.force_close();
                for _ in 0..120 {
                    if !this.restart_is_current(generation) || !owner.is_visible() {
                        return;
                    }
                    if dialog.parent().is_none()
                        && owner.dialogs().n_items() == 0
                        && owner.visible_dialog().is_none()
                    {
                        if let Some(app) = this.app.upgrade()
                            && let Err(error) = handoff.close_application(&app)
                        {
                            this.restart_failed(&owner, error);
                        }
                        return;
                    }
                    glib::timeout_future(Duration::from_millis(25)).await;
                }
            }
            this.restart_failed(&owner, PortalError::RestartFailed);
        });
    }

    fn restart_is_current(&self, generation: u64) -> bool {
        self.monitor_generation.get() == generation
            && matches!(*self.phase.borrow(), Phase::Restarting)
    }

    fn restart_failed(self: &Rc<Self>, owner: &adw::ApplicationWindow, error: PortalError) {
        self.set_phase(Phase::InstallFailed(error));
        if self.dialog.borrow().is_none() && owner.is_visible() && owner.dialogs().n_items() == 0 {
            self.present_dialog(owner);
        }
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
        let release = self.release.borrow();
        let mut view = match &*phase {
            Phase::Checking { retry } => {
                let heading = if *retry { "Повторная проверка" } else { "Проверка обновлений" };
                let mut view = Presentation::message(heading, "Получаем сведения о последнем стабильном выпуске…");
                view.busy = true;
                view.secondary = Some(Secondary::Dismiss("Отменить"));
                view
            }
            Phase::Latest => {
                let mut view = Presentation::message("Установлена актуальная версия", "Новых стабильных выпусков нет.").with_action(Primary::Check);
                view.available = "Нет новой версии".into();
                view
            }
            Phase::CheckFailed(error) => Presentation::message("Не удалось проверить обновления", error_text(*error)).with_action(Primary::Check),
            Phase::Offer | Phase::OpeningLink | Phase::LinkFailed(_) => {
                let Some(release) = release.as_ref() else {
                    return Presentation::message("Не удалось проверить обновления", error_text(CheckError::Internal)).with_action(Primary::Check);
                };
                let mut view = self.present_release(release);
                match *phase {
                    Phase::OpeningLink => {
                        view.primary = None;
                        view.secondary = Some(Secondary::Dismiss("Позже"));
                        view.busy = true;
                    }
                    Phase::LinkFailed(LinkTarget::SystemManager)
                        if view.primary == Some(Primary::OpenSystemManager) =>
                    {
                        view.body = "Не удалось открыть менеджер приложений. Пакет доступен на странице выпуска.".into();
                        view.primary = Some(Primary::OpenRelease);
                        view.secondary = Some(Secondary::Dismiss("Позже"));
                    }
                    Phase::LinkFailed(LinkTarget::Release) => {
                        view.body.push_str("\nНе удалось открыть страницу выпуска в браузере.");
                    }
                    _ => {}
                }
                view
            }
            Phase::Installing(_, progress) => {
                let mut view = Presentation::message("Загрузка и установка", "Flatpak обновляет Karin. Вы можете отменить операцию.");
                view.progress = progress.filter(|value| (0.0..=1.0).contains(value));
                view.busy = view.progress.is_none();
                view.secondary = Some(Secondary::Dismiss("Отменить"));
                view.can_close = false;
                view
            }
            Phase::Verifying(completion) => {
                let (status, body) = match completion {
                    Completion::Installed => ("Установка завершена", "Проверяем, что Flatpak установил сборку этого выпуска…"),
                    Completion::NoOperations => ("Проверяем установленную версию", "Flatpak не выполнил установку. Проверяем, было ли обновление установлено ранее…"),
                };
                let mut view = Presentation::message(status, body);
                view.can_close = false;
                view.busy = true;
                view.secondary = None;
                view
            }
            Phase::Restarting => {
                let mut view = Presentation::message("Перезапуск Karin", "Проверяем новую сборку и перезапускаем приложение…");
                view.busy = true;
                view.can_close = false;
                view.secondary = None;
                view
            }
            Phase::InstallFailed(error) => portal_failure(*error),
            Phase::InstallTimeout => Presentation::message("Не удалось завершить обновление", "Flatpak давно не сообщал о ходе установки. Повторите проверку или обновите Karin в системном менеджере приложений.").with_action(Primary::Check),
        };
        if let Some(release) = release.as_ref() {
            view.available = release.version.to_string();
            view.notes.clone_from(&release.notes);
        }
        view
    }

    fn present_release(&self, release: &Release) -> Presentation {
        let mut view = match &release.deployment {
            DeploymentStatus::Native => Presentation::message("Доступно обновление Karin", "Откройте страницу выпуска, чтобы скачать пакет для ручной установки.").with_action(Primary::OpenRelease),
            DeploymentStatus::ChannelNotConfigured => Presentation::message("Требуется системное обновление", "Для этой установки не настроен подписанный канал Karin. Пакет можно скачать со страницы выпуска.").with_action(Primary::OpenRelease),
            DeploymentStatus::NotPublished => Presentation::message("Выпуск готовится для Flatpak", "Подписанный канал Flatpak пока не подтвердил этот выпуск. Проверьте снова после публикации в канале.").with_action(Primary::Check),
            DeploymentStatus::Unverified(error) => Presentation::message("Не удалось подтвердить сборку", error_text(*error)).with_action(Primary::Check),
            DeploymentStatus::Verified(target) => match &*self.monitor.borrow() {
                Monitor::Idle => flatpak_offer(Availability::Waiting(WaitReason::AwaitingOffer)),
                Monitor::Connecting => {
                    let mut view = flatpak_offer(Availability::Waiting(WaitReason::AwaitingOffer));
                    view.busy = true;
                    view
                }
                Monitor::Failed(error) => portal_failure(*error).with_system_manager(),
                Monitor::Ready(monitor) => flatpak_offer(monitor.availability(target)),
            },
        };
        if view.secondary != Some(Secondary::OpenRelease) {
            view.secondary = Some(Secondary::Dismiss("Позже"));
        }
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

fn flatpak_offer(availability: Availability) -> Presentation {
    match availability {
        Availability::Install => Presentation::message(
            "Обновление подтверждено",
            "Flatpak подтвердил эту сборку. Обновление начнётся после вашего согласия.",
        ).with_action(Primary::Install),
        Availability::Restart => Presentation::message(
            "Готово к перезапуску",
            "Перед перезапуском Karin сверит установленную сборку с выпуском.",
        ).with_action(Primary::Restart),
        Availability::Waiting(WaitReason::AwaitingOffer) => Presentation::message(
            "Доступно обновление Karin",
            "Flatpak ещё проверяет эту сборку для установки из Karin. Обновление можно открыть в системном менеджере приложений.",
        ).with_system_manager(),
        Availability::Waiting(WaitReason::DifferentDeployment) => Presentation::message(
            "Доступно обновление Karin",
            "Flatpak сообщил о другой сборке. Доступные обновления можно проверить в системном менеджере приложений.",
        ).with_system_manager(),
        Availability::Waiting(WaitReason::InvalidIdentity) => Presentation::message(
            "Установка недоступна",
            "Не удалось сопоставить установленный пакет с подписанным каналом Karin. Проверьте источник в системном менеджере приложений.",
        ).with_system_manager(),
        Availability::Waiting(WaitReason::Unavailable) => {
            portal_failure(PortalError::Unavailable).with_system_manager()
        }
        Availability::Waiting(WaitReason::SystemManagerRequired) => {
            portal_failure(PortalError::ManagerRequired).with_system_manager()
        }
    }
}

fn portal_failure(error: PortalError) -> Presentation {
    match error {
        PortalError::Unavailable => Presentation::message(
            "Портал Flatpak недоступен",
            "Karin не может получить подтверждение установки. Обновление можно открыть в системном менеджере приложений.",
        ).with_action(Primary::Check),
        PortalError::ManagerRequired => Presentation::message(
            "Требуется системное обновление",
            "Flatpak сообщил, что эту сборку нужно установить через системный менеджер приложений.",
        ).with_action(Primary::Check),
        _ => Presentation::message("Не удалось завершить обновление", error.to_string()).with_action(Primary::Check),
    }
}

impl UpdateDialog {
    fn new() -> Self {
        let dialog = adw::Dialog::builder()
            .title("Обновления Karin")
            .content_width(540)
            // The dialog opens before release notes and actions are known.
            // Reserve room for the footer in both the dialog and sheet layouts.
            .content_height(440)
            .width_request(300)
            .height_request(310)
            .build();
        dialog.set_widget_name("karin-updates");
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        super::margins(&content, 20);
        content.set_margin_top(10);
        content.set_margin_bottom(12);

        let versions = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        versions.add_css_class("card");
        let (installed, installed_version) = version_field("Установлена", config::VERSION);
        installed_version.set_widget_name("karin-updates-installed-version");
        let (available, available_version) = version_field("Доступна", "Не определена");
        available_version.set_widget_name("karin-updates-available-version");
        versions.append(&installed);
        versions.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        versions.append(&available);
        content.append(&versions);

        let status = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let heading = wrapped_label();
        heading.set_widget_name("karin-updates-status");
        heading.add_css_class("title-3");
        let body = wrapped_label();
        body.add_css_class("dim-label");
        let busy_heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        busy_heading.append(&spinner);
        busy_heading.append(&heading);
        let progress = gtk::ProgressBar::new();
        progress.set_show_text(true);
        progress.update_property(&[gtk::accessible::Property::Label("Ход установки Flatpak")]);
        status.append(&busy_heading);
        status.append(&body);
        status.append(&progress);
        content.append(&status);

        let notes_block = gtk::Box::new(gtk::Orientation::Vertical, 7);
        notes_block.set_vexpand(true);
        let notes_title = wrapped_label();
        notes_title.set_text("Что нового");
        notes_title.add_css_class("heading");
        let notes = wrapped_label();
        notes.set_selectable(true);
        let notes_scroll = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&notes)
            .build();
        notes_scroll.set_widget_name("karin-updates-notes");
        notes_block.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        notes_block.append(&notes_title);
        notes_block.append(&notes_scroll);
        content.append(&notes_block);
        // Only the release notes may scroll. Version, reason and actions keep
        // their allocations even when the host opens the dialog as a sheet.
        toolbar.set_content(Some(&content));

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_halign(gtk::Align::End);
        super::margins(&actions, 12);
        actions.set_margin_start(20);
        actions.set_margin_end(20);
        let primary = gtk::Button::new();
        primary.set_widget_name("karin-updates-primary-action");
        primary.add_css_class("suggested-action");
        let secondary = gtk::Button::new();
        actions.append(&secondary);
        actions.append(&primary);
        toolbar.add_bottom_bar(&actions);
        let compact = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            450.0,
            adw::LengthUnit::Px,
        ));
        compact.add_setter(
            &actions,
            "orientation",
            Some(&gtk::Orientation::Vertical.to_value()),
        );
        compact.add_setter(&actions, "halign", Some(&gtk::Align::Fill.to_value()));
        dialog.add_breakpoint(compact);
        dialog.set_child(Some(&toolbar));
        dialog.set_focus(Some(&secondary));
        Self {
            dialog,
            available_version,
            heading,
            body,
            notes,
            notes_block,
            progress,
            spinner,
            primary,
            secondary,
        }
    }

    fn render(&self, view: Presentation) {
        self.available_version.set_text(&view.available);
        self.heading.set_text(view.heading);
        self.body.set_text(&view.body);
        self.notes.set_text(&view.notes);
        self.notes_block.set_visible(!view.notes.is_empty());
        self.spinner.set_visible(view.busy);
        self.spinner.set_spinning(view.busy);
        self.progress.set_visible(view.progress.is_some());
        if let Some(progress) = view.progress {
            self.progress.set_fraction(progress);
        }
        self.primary.set_visible(view.primary.is_some());
        if let Some(primary) = view.primary {
            self.primary.set_label(primary.label());
        }
        self.secondary.set_visible(view.secondary.is_some());
        if let Some(secondary) = view.secondary {
            self.secondary.set_label(secondary.label());
        }
        self.dialog.set_can_close(view.can_close);
    }
}

fn version_field(title: &str, version: &str) -> (gtk::Box, gtk::Label) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_hexpand(true);
    super::margins(&column, 12);
    let title = gtk::Label::builder().label(title).xalign(0.0).build();
    title.add_css_class("dim-label");
    let value = wrapped_label();
    value.set_text(version);
    value.add_css_class("heading");
    column.append(&title);
    column.append(&value);
    (column, value)
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
        CheckError::Network => {
            "Нет связи с сервером обновлений. Проверьте подключение к сети.".into()
        }
        CheckError::Timeout => {
            "Сервер обновлений не ответил вовремя. Повторите попытку позже.".into()
        }
        CheckError::RateLimited => {
            "Сервер обновлений ограничил частоту запросов. Повторите попытку позже.".into()
        }
        CheckError::Http(status) => format!("Сервер обновлений вернул ошибку HTTP {status}."),
        CheckError::InvalidData => {
            "Получены неполные или некорректные сведения об обновлении.".into()
        }
        CheckError::Internal => "Не удалось подготовить проверку обновлений.".into(),
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, process::Command, time::Instant};

    use super::*;

    fn pump_until(mut ready: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(4);
        let context = glib::MainContext::default();
        while !ready() {
            while context.pending() {
                context.iteration(false);
            }
            assert!(
                Instant::now() < deadline,
                "update dialog layout or input did not settle"
            );
            std::thread::sleep(Duration::from_millis(8));
        }
    }

    fn settle_frames(window: &adw::ApplicationWindow) {
        let frames = Rc::new(Cell::new(0));
        let observed = frames.clone();
        window.add_tick_callback(move |_, _| {
            observed.set(observed.get() + 1);
            if observed.get() >= 24 {
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        pump_until(|| frames.get() >= 24);
    }

    fn descendant(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        std::iter::successors(widget.first_child(), gtk::prelude::WidgetExt::next_sibling)
            .find_map(|child| descendant(&child, name))
    }

    fn assert_in_window(child: &impl IsA<gtk::Widget>, window: &adw::ApplicationWindow) {
        assert!(child.is_mapped(), "{} is hidden", child.widget_name());
        let bounds = child
            .compute_bounds(window)
            .expect("widget belongs to dialog window");
        assert!(bounds.width() > 1.0 && bounds.height() > 1.0);
        assert!(
            bounds.x() >= 0.0
                && bounds.y() >= 0.0
                && bounds.x() + bounds.width() <= window.width() as f32 + 1.0
                && bounds.y() + bounds.height() <= window.height() as f32 + 1.0,
            "{} {:?} outside {}×{} dialog host",
            child.widget_name(),
            bounds,
            window.width(),
            window.height()
        );
    }

    fn capture(name: &str, xid: &str) {
        let Some(directory) = std::env::var_os("KARIN_TEST_CAPTURE_DIR") else {
            return;
        };
        assert!(
            Command::new("import")
                .args(["-window", xid])
                .arg(PathBuf::from(directory).join(format!("{name}.png")))
                .status()
                .expect("ImageMagick captures the actual GTK window")
                .success()
        );
    }

    fn click_action(button: &gtk::Button, window: &adw::ApplicationWindow, xid: &str) {
        assert_in_window(button, window);
        let bounds = button.compute_bounds(window).unwrap();
        let scale = window.scale_factor() as f32;
        let x = ((bounds.x() + bounds.width() / 2.0) * scale).round();
        let y = ((bounds.y() + bounds.height() / 2.0) * scale).round();
        let clicked = Rc::new(Cell::new(false));
        let observed = clicked.clone();
        let handler = button.connect_clicked(move |_| observed.set(true));
        assert!(
            Command::new("xdotool")
                .args([
                    "mousemove",
                    "--window",
                    xid,
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    "1",
                ])
                .status()
                .unwrap()
                .success()
        );
        pump_until(|| clicked.get());
        button.disconnect(handler);
    }

    fn note_fixture() -> String {
        notes::excerpt(&format!(
            "## Изменения\n\n- **Плейлисты** можно редактировать из контекстного меню.\n- Уточнён статус обновления [Flatpak](https://example.test/guide).\n- Очередь и просмотр альбомов используют привычную навигацию.\n\n{}\n<!-- karin-flatpak-update-sha256: fixture -->",
            "Проверяем перенос длинного текста и работу прокрутки в узком окне.\n\n".repeat(16),
        ))
    }

    #[test]
    fn flatpak_portal_failure_offers_fallback_before_install_and_retry_after_failure() {
        let offer = flatpak_offer(Availability::Waiting(WaitReason::Unavailable));
        assert_eq!(offer.primary, Some(Primary::OpenSystemManager));
        assert!(offer.secondary == Some(Secondary::OpenRelease));

        let failure = portal_failure(PortalError::Unavailable);
        assert_eq!(failure.primary, Some(Primary::Check));
    }

    #[test]
    #[ignore = "requires isolated X11, GTK display and xdotool"]
    fn native_update_dialog_adaptive_status_and_notes() {
        gtk::init().unwrap();
        adw::init().unwrap();
        crate::ui::prepare_typography().unwrap();
        let manager = adw::StyleManager::default();
        let fixture_notes = note_fixture();
        assert!(!fixture_notes.contains("example.test") && !fixture_notes.contains("**"));
        assert!(!fixture_notes.contains("sha256"));
        let short_notes = "Исправлено отображение текущего трека в Discord Rich Presence при воспроизведении напрямую из «Мне нравится». Название трека в карточке открывает его публичную страницу в Яндекс Музыке.";
        let expected_scale =
            std::env::var("GDK_SCALE").map_or(1, |value| value.parse::<i32>().unwrap());
        for (scheme, theme) in [
            (adw::ColorScheme::ForceLight, "light"),
            (adw::ColorScheme::ForceDark, "dark"),
        ] {
            manager.set_color_scheme(scheme);
            for (width, height, size) in [(880, 680, "wide"), (420, 520, "compact")] {
                let title = format!("Karin update fixture {size} {theme}");
                let window = adw::ApplicationWindow::builder()
                    .title(&title)
                    .default_width(width)
                    .default_height(height)
                    .build();
                window.set_content(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
                super::super::style::attach(&window);
                window.present();
                pump_until(|| window.is_mapped() && window.width() > 0);
                let dialog = UpdateDialog::new();
                let view = |availability, notes: &str| Presentation {
                    available: "0.8.6".into(),
                    notes: notes.to_owned(),
                    ..flatpak_offer(availability)
                };
                let mut checking = Presentation::message(
                    "Проверка обновлений",
                    "Получаем сведения о последнем стабильном выпуске…",
                );
                checking.busy = true;
                checking.secondary = Some(Secondary::Dismiss("Отменить"));
                dialog.render(checking);
                dialog.dialog.present(Some(&window));
                settle_frames(&window);
                let xid = Command::new("xdotool")
                    .args(["search", "--onlyvisible", "--name", &title])
                    .output()
                    .unwrap();
                let xid = String::from_utf8(xid.stdout).unwrap();
                let xid = xid.lines().last().expect("native window found");

                dialog.render(view(
                    Availability::Waiting(WaitReason::AwaitingOffer),
                    short_notes,
                ));
                settle_frames(&window);
                assert_eq!(
                    dialog.primary.label().as_deref(),
                    Some("Открыть в менеджере приложений")
                );
                assert_eq!(
                    dialog.secondary.label().as_deref(),
                    Some("Страница выпуска")
                );
                click_action(&dialog.primary, &window, xid);
                click_action(&dialog.secondary, &window, xid);

                dialog.render(view(Availability::Install, short_notes));
                settle_frames(&window);
                assert_eq!(
                    dialog.primary.label().as_deref(),
                    Some("Обновить и перезапустить")
                );
                click_action(&dialog.primary, &window, xid);
                capture(&format!("update-confirmed-short-{size}-{theme}"), xid);

                dialog.render(view(
                    Availability::Waiting(WaitReason::AwaitingOffer),
                    &fixture_notes,
                ));
                pump_until(|| {
                    dialog.notes.is_mapped()
                        && dialog.primary.is_mapped()
                        && dialog.notes.width() > 10
                        && dialog.primary.width() > 10
                });
                settle_frames(&window);
                assert_eq!(window.scale_factor(), expected_scale);
                let installed = descendant(
                    dialog.dialog.upcast_ref(),
                    "karin-updates-installed-version",
                )
                .unwrap()
                .downcast::<gtk::Label>()
                .unwrap();
                assert_eq!(installed.text(), config::VERSION);
                assert_eq!(dialog.available_version.text(), "0.8.6");
                assert_eq!(
                    dialog.primary.label().as_deref(),
                    Some("Открыть в менеджере приложений")
                );
                assert!(dialog.body.text().contains("Flatpak ещё проверяет"));
                assert_in_window(&installed, &window);
                assert_in_window(&dialog.available_version, &window);
                assert_in_window(&dialog.heading, &window);
                assert_in_window(&dialog.body, &window);
                assert_in_window(&dialog.primary, &window);
                let content_width = dialog.dialog.child().unwrap().width();
                if width == 880 {
                    assert!(
                        (520..=560).contains(&content_width),
                        "dialog content width = {content_width}"
                    );
                } else {
                    assert!(content_width <= window.width());
                }
                let scroller = descendant(dialog.dialog.upcast_ref(), "karin-updates-notes")
                    .unwrap()
                    .downcast::<gtk::ScrolledWindow>()
                    .unwrap();
                let scrollbar = scroller.vadjustment();
                assert!(scrollbar.upper() > scrollbar.page_size());
                assert!(
                    scroller
                        .parent()
                        .and_then(|parent| parent.ancestor(gtk::ScrolledWindow::static_type()))
                        .is_none()
                );
                capture(&format!("update-waiting-{size}-{theme}"), xid);

                dialog.render(view(Availability::Install, &fixture_notes));
                pump_until(|| {
                    dialog.primary.width() > 10
                        && dialog.primary.label().as_deref() == Some("Обновить и перезапустить")
                });
                assert_in_window(&dialog.primary, &window);
                assert_eq!(dialog.heading.text(), "Обновление подтверждено");
                settle_frames(&window);
                capture(&format!("update-confirmed-{size}-{theme}"), xid);
                if width == 420 {
                    let activations = Rc::new(Cell::new(0));
                    let clicked = activations.clone();
                    dialog
                        .primary
                        .connect_clicked(move |_| clicked.set(clicked.get() + 1));
                    assert!(dialog.primary.grab_focus());
                    assert!(
                        Command::new("xdotool")
                            .args(["windowfocus", "--sync", xid, "key", "Return"])
                            .status()
                            .unwrap()
                            .success()
                    );
                    pump_until(|| activations.get() == 1);
                }
                let unavailable = view(
                    Availability::Waiting(WaitReason::SystemManagerRequired),
                    &fixture_notes,
                );
                dialog.render(unavailable);
                assert_eq!(dialog.heading.text(), "Требуется системное обновление");
                assert_eq!(
                    dialog.primary.label().as_deref(),
                    Some("Открыть в менеджере приложений")
                );
                let mut installing =
                    Presentation::message("Загрузка и установка", "Установка продолжается…");
                installing.busy = true;
                dialog.render(installing);
                assert!(dialog.spinner.is_spinning() && !dialog.progress.is_visible());
                let measured = Presentation::message(
                    "Загрузка и установка",
                    "Flatpak сообщил о ходе операции.",
                );
                dialog.render(Presentation {
                    progress: Some(0.42),
                    ..measured
                });
                assert!(dialog.progress.is_visible() && !dialog.spinner.is_spinning());
                assert_eq!(dialog.progress.fraction(), 0.42);
                dialog.dialog.force_close();
                pump_until(|| window.dialogs().n_items() == 0);
                window.close();
                pump_until(|| !window.is_visible());
            }
        }
    }
}
