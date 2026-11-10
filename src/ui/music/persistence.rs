// SPDX-License-Identifier: GPL-3.0-or-later
//! GTK coordinator for protected session storage, private cache and preferences.
//! Workers own complete durable operations; window disposal only drops UI work.
use super::super::preferences::{PreferencesCallbacks, PreferencesSnapshot, PreferencesView};
use super::*;
use crate::{
    audio_cache::{
        AudioCache, CacheError as AudioError, CachePolicy as AudioPolicy, Lease as AudioLease,
        OnlineStreamAuthority, PrivateHttpCachePolicy, Stats as AudioStats, StorageAuthority,
    },
    cache::{AccountScope, Cache, CachePolicy, CacheStats},
    cache_budget::CacheBudget,
    credentials::{BackendKind, CredentialStore, StorageError},
    session_lifecycle::{PersistenceTicket, SessionPersistence},
    settings::{Appearance, LoadStatus, Preferences, SettingsError, SettingsLoad, SettingsStore},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

thread_local! {
    // A reopened native window may otherwise create independent stores while
    // a retired window is still finishing its required durable transaction.
    static WORK_IN_FLIGHT: Cell<usize> = const { Cell::new(0) };
}
struct WorkFence;
impl WorkFence {
    fn new() -> Self {
        WORK_IN_FLIGHT.with(|count| count.set(count.get() + 1));
        Self
    }
    fn active() -> bool {
        WORK_IN_FLIGHT.with(|count| count.get() != 0)
    }
}
impl Drop for WorkFence {
    fn drop(&mut self) {
        WORK_IN_FLIGHT.with(|count| count.set(count.get() - 1));
    }
}
struct DurableWork(Arc<AtomicUsize>);
impl DurableWork {
    fn new(count: Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::AcqRel);
        Self(count)
    }
}
impl Drop for DurableWork {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    Setup,
    Probe,
    Restore,
    Save,
    Forget,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Retry {
    None,
    Setup,
    Restore,
    Save,
    Forget,
    Settings,
}

#[derive(Clone, Copy)]
enum CacheTask {
    Usage,
    Trim,
    Clear,
}

pub(super) struct State {
    pub(super) banner: gtk::Box,
    row: adw::ActionRow,
    spinner: gtk::Spinner,
    retry_button: gtk::Button,
    enabled: bool,
    started: bool,
    retry_connected: bool,
    closed: bool,
    settings: Option<SettingsStore>,
    manager: Option<SessionPersistence>,
    injected: Option<(SettingsStore, CredentialStore)>,
    values: Preferences,
    ready: bool,
    supported: bool,
    restore_allowed: bool,
    phase: Phase,
    retry: Retry,
    operation: u64,
    restore_ticket: Option<PersistenceTicket>,
    critical: bool,
    durable_work: Arc<AtomicUsize>,
    queued_info: bool,
    settings_busy: bool,
    settings_dirty: bool,
    settings_operation: u64,
    cache: Option<Cache>,
    cache_budget: Option<Arc<CacheBudget>>,
    cache_policy_revision: u64,
    cache_policy_pending: bool,
    cache_retry: Option<CacheTask>,
    cache_message: Option<String>,
    audio_root: Option<PathBuf>,
    audio_cache: Option<AudioCache>,
    // Trusted assembly policy; service metadata/streaming never populates it.
    audio_permission: Arc<dyn StorageAuthority>,
    audio_cache_stats: Option<AudioStats>,
    audio_purge_pending: bool,
    audio_purge_generation: u64,
    scope: Option<AccountScope>,
    cache_busy: bool,
    cache_operation: u64,
    cache_trim_pending: bool,
    cache_clear_pending: bool,
    cache_stats: Option<CacheStats>,
    purge_after_forget: bool,
    barrier_confirmed: bool,
    status: String,
    message: Option<String>,
    view: Option<PreferencesView>,
}

impl State {
    pub(super) fn system() -> Self {
        Self::new(true)
    }
    pub(super) fn disabled() -> Self {
        Self::new(false)
    }
    fn new(enabled: bool) -> Self {
        let banner = gtk::Box::new(gtk::Orientation::Vertical, 0);
        banner.set_widget_name("session-persistence-banner");
        let group = adw::PreferencesGroup::new();
        let row = adw::ActionRow::builder()
            .title_lines(0)
            .subtitle_lines(0)
            .build();
        let spinner = gtk::Spinner::new();
        spinner.set_valign(gtk::Align::Center);
        let retry_button = gtk::Button::with_label("Повторить");
        retry_button.set_valign(gtk::Align::Center);
        retry_button.set_widget_name("session-persistence-retry");
        row.add_suffix(&spinner);
        row.add_suffix(&retry_button);
        group.add(&row);
        banner.append(&group);
        banner.set_visible(false);
        let root = glib::user_cache_dir().join(crate::config::APP_ID);
        let metadata_root = root.join("persistent");
        let audio_root = root.join("audio");
        let budget = enabled
            .then(|| {
                CacheBudget::new(
                    metadata_root.clone(),
                    audio_root.clone(),
                    true,
                    Preferences::default().cache_limit.bytes(),
                )
                .ok()
            })
            .flatten();
        let cache = budget.as_ref().and_then(|budget| {
            Cache::new_with_budget(metadata_root, CachePolicy::default(), budget.clone()).ok()
        });
        Self {
            banner,
            row,
            spinner,
            retry_button,
            enabled,
            started: false,
            retry_connected: false,
            closed: false,
            settings: None,
            manager: None,
            injected: None,
            values: Preferences::default(),
            ready: !enabled,
            supported: false,
            restore_allowed: false,
            phase: Phase::Idle,
            retry: Retry::None,
            operation: 0,
            restore_ticket: None,
            critical: false,
            durable_work: Arc::new(AtomicUsize::new(0)),
            queued_info: false,
            settings_busy: false,
            settings_dirty: false,
            settings_operation: 0,
            cache,
            cache_budget: budget,
            cache_policy_revision: 0,
            cache_policy_pending: false,
            cache_retry: None,
            cache_message: None,
            audio_root: enabled.then_some(audio_root),
            audio_cache: None,
            audio_permission: Arc::new(PrivateHttpCachePolicy),
            audio_cache_stats: None,
            audio_purge_pending: false,
            audio_purge_generation: 0,
            scope: None,
            cache_busy: false,
            cache_operation: 0,
            cache_trim_pending: false,
            cache_clear_pending: false,
            cache_stats: None,
            purge_after_forget: false,
            barrier_confirmed: false,
            status: if enabled {
                "Проверяем доступность сохранённого входа…"
            } else {
                "Локальная проверка: вход и настройки не записываются на диск."
            }
            .into(),
            message: None,
            view: None,
        }
    }
    #[cfg(test)]
    fn for_testing(
        settings: SettingsStore,
        credentials: CredentialStore,
        cache: Option<Cache>,
    ) -> Self {
        let mut state = Self::new(true);
        state.injected = Some((settings, credentials));
        state.cache_budget = cache.as_ref().and_then(Cache::shared_budget);
        state.cache = cache;
        state.audio_root = None;
        state
    }
    fn snapshot(&self) -> PreferencesSnapshot {
        PreferencesSnapshot {
            values: self.values.clone(),
            settings_ready: self.ready && !self.critical,
            storage_supported: self.supported && !self.values.forget_pending,
            session_status: self.status.clone(),
            session_busy: self.phase != Phase::Idle,
            can_retry_session: self.retry != Retry::None && !self.critical,
            cache_stats: self.cache_stats,
            cache_busy: self.cache_busy || self.cache_policy_pending || self.critical,
            can_retry_cache: self.cache_retry.is_some(),
            message: self.cache_message.clone().or_else(|| self.message.clone()),
        }
    }
    fn request_audio_purge(&mut self) {
        self.audio_purge_pending = true;
        self.audio_purge_generation = self.audio_purge_generation.wrapping_add(1);
        if let Some(audio) = &self.audio_cache {
            let _ = audio.fence();
        }
    }
    fn cache_admission_enabled(&self) -> bool {
        self.values.cache_enabled
            && self.ready
            && !self.values.forget_pending
            && !self.audio_purge_pending
            && !self.closed
    }
    fn audio_policy(&self) -> AudioPolicy {
        // The shared coordinator owns the byte budget. This fallback policy
        // exists only for isolated standalone stores used by diagnostics.
        AudioPolicy::default()
    }
    fn policy(&self) -> CachePolicy {
        CachePolicy {
            enabled: self.cache_admission_enabled(),
            ..CachePolicy::default()
        }
    }
}

struct Initial {
    audio_cache: Result<Option<AudioCache>, AudioError>,
    settings: Option<SettingsStore>,
    loaded: Result<SettingsLoad, SettingsError>,
    manager: SessionPersistence,
}
struct Forgotten {
    barrier: bool,
    deletion: Result<(), StorageError>,
    cache: Result<(), ()>,
    settings: Result<(), SettingsError>,
}

/// Called only by persistence workers. Opening/scanning the namespace never
/// occupies GTK, and an unavailable existing namespace is not reported as empty.
fn open_audio_store(
    current: Option<AudioCache>,
    root: Option<PathBuf>,
    policy: AudioPolicy,
    budget: Option<Arc<CacheBudget>>,
    permission: Arc<dyn StorageAuthority>,
) -> Result<Option<AudioCache>, AudioError> {
    if let Some(cache) = current {
        cache.configure(policy)?;
        return Ok(Some(cache));
    }
    root.map(|root| match budget {
        Some(budget) => AudioCache::open_with_budget(
            root,
            Arc::new(OnlineStreamAuthority::with_storage_permission(permission)),
            policy,
            budget,
        ),
        None => AudioCache::open(
            root,
            Arc::new(OnlineStreamAuthority::with_storage_permission(permission)),
            policy,
        ),
    })
    .transpose()
}

struct CacheOutcome {
    total: Result<CacheStats, ()>,
    metadata: Result<CacheStats, ()>,
    audio: Result<Option<(AudioCache, AudioStats)>, AudioError>,
}

impl Controller {
    fn persistence_job<T: Send + 'static>(
        self: &Rc<Self>,
        work: impl FnOnce() -> T + Send + 'static,
        completed: impl FnOnce(&Rc<Self>, Result<T, ()>) + 'static,
    ) {
        let weak = Rc::downgrade(self);
        let app = self.app.upgrade();
        // Reserve before yielding: the last native window may close before
        // the spawned future receives its first main-context poll.
        let hold = app.as_ref().map(|app| app.hold());
        let fence = WorkFence::new();
        glib::spawn_future_local(async move {
            let (_app, _hold, _fence) = (app, hold, fence);
            let result = gio::spawn_blocking(work).await.map_err(|_| ());
            if let Some(this) = weak.upgrade()
                && !this.model.borrow().closed
                && !this.persistence.borrow().closed
            {
                completed(&this, result);
            }
        });
    }
    pub(super) fn start_persistence(self: &Rc<Self>) {
        {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled || p.started || p.closed {
                return;
            }
            if WorkFence::active() {
                p.started = true;
                p.phase = Phase::Setup;
                p.status = "Завершаем сохранение предыдущего окна…".into();
                drop(p);
                self.render_persistence();
                let weak = Rc::downgrade(self);
                glib::timeout_add_local(std::time::Duration::from_millis(25), move || {
                    let Some(this) = weak.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    if this.persistence.borrow().closed || this.model.borrow().closed {
                        return glib::ControlFlow::Break;
                    }
                    if WorkFence::active() {
                        return glib::ControlFlow::Continue;
                    }
                    this.persistence.borrow_mut().started = false;
                    this.start_persistence();
                    glib::ControlFlow::Break
                });
                return;
            }
        }
        let inputs = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled || p.started || p.closed {
                return;
            }
            p.started = true;
            p.phase = Phase::Setup;
            p.operation = p.operation.wrapping_add(1);
            p.retry = Retry::None;
            p.status = "Загружаем настройки…".into();
            (
                p.operation,
                p.injected.take(),
                p.audio_root.clone(),
                p.cache_budget.clone(),
                p.audio_permission.clone(),
            )
        };
        if !self.persistence.borrow().retry_connected {
            self.persistence.borrow_mut().retry_connected = true;
            let weak = Rc::downgrade(self);
            self.persistence
                .borrow()
                .retry_button
                .connect_clicked(move |_| {
                    if let Some(this) = weak.upgrade() {
                        this.retry_persistence();
                    }
                });
        }
        self.render_persistence();
        self.persistence_job(
            move || {
                let (settings, credentials) = match inputs.1 {
                    Some((settings, credentials)) => (Ok(settings), credentials),
                    None => (SettingsStore::for_application(), CredentialStore::system()),
                };
                let loaded = match &settings {
                    Ok(settings) => settings.load(),
                    Err(error) => Err(*error),
                };
                let values = loaded.as_ref().map(|loaded| loaded.preferences.clone()).unwrap_or_default();
                if let Some(budget) = &inputs.3 {
                    let _ = budget.configure(values.cache_enabled, values.cache_limit.bytes());
                }
                let audio_policy = AudioPolicy::default();
                let audio_cache = open_audio_store(None, inputs.2, audio_policy, inputs.3, inputs.4);
                Initial {
                    audio_cache,
                    settings: settings.ok(),
                    loaded,
                    manager: SessionPersistence::new(credentials),
                }
            },
            move |this, result| {
                if this.persistence.borrow().operation != inputs.0 {
                    return;
                }
                match result {
                    Ok(initial) => {
                        let loaded = initial.loaded;
                        {
                            let mut p = this.persistence.borrow_mut();
                            p.settings = initial.settings;
                            p.manager = Some(initial.manager);
                            match initial.audio_cache {
                                Ok(cache) => p.audio_cache = cache,
                                Err(_) => {
                                    p.cache_message = Some("Кеш недоступен. Проверку можно повторить в настройках; онлайн-воспроизведение продолжает работать.".into());
                                    p.cache_retry = Some(CacheTask::Usage);
                                }
                            }
                            p.phase = Phase::Idle;
                            match loaded {
                                Ok(loaded) => {
                                    p.values = loaded.preferences;
                                    p.ready = true;
                                    p.restore_allowed = loaded.status != LoadStatus::RecoveredCorrupt;
                                    if loaded.status == LoadStatus::RecoveredCorrupt {
                                        p.values.remember_login = false;
                                        p.values.forget_pending = true;
                                        p.message = Some("Повреждённые настройки восстановлены. Старый сохранённый вход не используется; завершаем его безопасную очистку.".into());
                                    }
                                }
                                Err(_) => {
                                    p.ready = false;
                                    p.restore_allowed = false;
                                    p.retry = Retry::Setup;
                                    p.status = "Настройки недоступны; автоматическое восстановление входа отключено.".into();
                                    p.message = Some("Можно войти на время текущего запуска. Проверьте доступ к приватному каталогу настроек и повторите попытку.".into());
                                }
                            }
                        }
                        this.apply_persistence_policy();
                        this.apply_appearance();
                        this.sync_info_visibility();
                        if !this.persistence.borrow().ready {
                            this.render_persistence();
                            return;
                        }
                        if this.persistence.borrow().values.forget_pending {
                            this.forget_persistent_session(true);
                        } else {
                            this.probe_persistence();
                        }
                        if this.persistence.borrow().values.cache_enabled {
                            this.trim_persistent_cache();
                        } else {
                            this.reload_cache_stats();
                        }
                    }
                    _ => {
                        let mut p = this.persistence.borrow_mut();
                        p.phase = Phase::Idle;
                        p.ready = false;
                        p.retry = Retry::Setup;
                        p.status = "Настройки недоступны; вход действует только в этом запуске.".into();
                        p.message = Some("Постоянное хранение отключено до успешной загрузки настроек.".into());
                        drop(p);
                        this.apply_persistence_policy();
                        this.render_persistence();
                    }
                }
            },
        );
    }
    fn probe_persistence(self: &Rc<Self>) {
        let task = {
            let mut p = self.persistence.borrow_mut();
            let Some(manager) = p.manager.clone() else {
                return;
            };
            let ticket = manager.begin();
            p.phase = Phase::Probe;
            p.operation = p.operation.wrapping_add(1);
            p.retry = Retry::None;
            p.status = "Проверяем защищённое хранилище…".into();
            (manager, ticket, p.operation)
        };
        self.render_persistence();
        let work = task.0.clone();
        let ticket = task.1.clone();
        self.persistence_job(
            move || work.probe(&ticket),
            move |this, result| {
                if this.persistence.borrow().operation != task.2 || !task.0.is_current(&task.1) {
                    return;
                }
                let ready = match result {
                    Ok(Ok(kind)) => {
                        let mut p = this.persistence.borrow_mut();
                        p.supported = true;
                        p.phase = Phase::Idle;
                        p.status = match kind {
                            BackendKind::SecretService => "Защищённое хранилище системы доступно.",
                            BackendKind::SecretPortal => "Интерфейс Secret Portal доступен; сохранение проверяется отдельно.",
                        }.into();
                        true
                    }
                    Ok(Err(error)) => {
                        let mut p = this.persistence.borrow_mut();
                        p.supported = matches!(error, StorageError::Locked | StorageError::Permission);
                        p.phase = Phase::Idle;
                        p.retry = Retry::Restore;
                        p.status = error.user_message().into();
                        false
                    }
                    Err(()) => {
                        let mut p = this.persistence.borrow_mut();
                        p.supported = false;
                        p.phase = Phase::Idle;
                        p.retry = Retry::Restore;
                        p.status = "Не удалось проверить защищённое хранилище. Можно войти на время этого запуска.".into();
                        false
                    }
                };
                this.render_persistence();
                if ready {
                    let active = this.model.borrow().account_valid;
                    if active {
                        if this.persistence.borrow().values.remember_login {
                            this.persist_current_session();
                        }
                    } else if this.persistence.borrow().values.remember_login && this.persistence.borrow().restore_allowed {
                        this.restore_persistent_session();
                    } else {
                        let mut p = this.persistence.borrow_mut();
                        p.status = "Вход не восстанавливается автоматически. Войдите в свой аккаунт.".into();
                        drop(p);
                        this.render_persistence();
                    }
                }
            },
        );
    }
    fn restore_persistent_session(self: &Rc<Self>) {
        let task = {
            let mut p = self.persistence.borrow_mut();
            if !p.ready || p.values.forget_pending || !p.values.remember_login {
                return;
            }
            let Some(manager) = p.manager.clone() else {
                return;
            };
            let ticket = manager.begin();
            p.operation = p.operation.wrapping_add(1);
            p.phase = Phase::Restore;
            p.retry = Retry::None;
            p.status = "Восстанавливаем вход…".into();
            (manager, ticket, p.operation)
        };
        self.render_persistence();
        let work = task.0.clone();
        let ticket = task.1.clone();
        self.persistence_job(
            move || work.load(&ticket),
            move |this, result| {
                if this.persistence.borrow().operation != task.2 || !task.0.is_current(&task.1) {
                    return;
                }
                match result {
                    Ok(Ok(Some(token))) => {
                        let mut p = this.persistence.borrow_mut();
                        p.restore_ticket = Some(task.1);
                        p.status = "Проверяем сохранённый вход в Яндекс Музыке…".into();
                        drop(p);
                        this.render_persistence();
                        this.authenticate(token);
                    }
                    Ok(Ok(None)) => {
                        let mut p = this.persistence.borrow_mut();
                        p.phase = Phase::Idle;
                        p.retry = Retry::None;
                        p.status = "Сохранённого входа пока нет. Войдите в свой аккаунт.".into();
                        drop(p);
                        this.render_persistence();
                    }
                    Ok(Err(error)) => {
                        let mut p = this.persistence.borrow_mut();
                        p.phase = Phase::Idle;
                        p.retry = Retry::Restore;
                        p.status = error.user_message().into();
                        p.supported = !matches!(error, StorageError::Unavailable);
                        drop(p);
                        if error == StorageError::Unconfirmed {
                            this.forget_persistent_session(false);
                        } else {
                            this.render_persistence();
                        }
                    }
                    Err(()) => {
                        let mut p = this.persistence.borrow_mut();
                        p.phase = Phase::Idle;
                        p.retry = Retry::Restore;
                        p.status = "Восстановление входа не завершилось. Повторите попытку или войдите снова.".into();
                        drop(p);
                        this.render_persistence();
                    }
                }
            },
        );
    }
    pub(super) fn session_validated(self: &Rc<Self>, scope: AccountScope) {
        let restored = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled || p.closed {
                return;
            }
            if p.scope.as_ref().is_some_and(|old| old != &scope) {
                p.request_audio_purge();
            }
            p.scope = Some(scope.clone());
            if let Some(cache) = &p.cache {
                cache.bind(Some(scope));
            }
            let restored = p.restore_ticket.take();
            p.phase = Phase::Idle;
            if !p.values.forget_pending {
                p.retry = Retry::None;
            }
            restored.map(|ticket| (p.manager.clone(), ticket))
        };
        self.apply_persistence_policy();
        if let Some((Some(manager), ticket)) = restored {
            let mut p = self.persistence.borrow_mut();
            if manager.accept_restored(&ticket).is_ok() {
                p.status = "Вход восстановлен и проверен Яндекс Музыкой.".into();
            } else {
                p.status = "Текущий вход проверен; сохранение сессии не подтверждено.".into();
                p.retry = Retry::Save;
            }
            drop(p);
            self.render_persistence();
        } else if self.persistence.borrow().values.remember_login {
            self.persist_current_session();
        } else {
            let mut p = self.persistence.borrow_mut();
            p.status = "Вход действует до закрытия Karin. Запоминание входа выключено.".into();
            drop(p);
            self.render_persistence();
        }
        self.reload_cache_stats();
    }
    pub(super) fn session_validation_failed(self: &Rc<Self>, error: MusicError) {
        let revoked = matches!(
            error,
            MusicError::Unauthenticated | MusicError::AccessDenied | MusicError::Http(401)
        );
        let restored = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled {
                return;
            }
            let restored = p.restore_ticket.take().is_some();
            if restored {
                p.phase = Phase::Idle;
            }
            if let Some(cache) = &p.cache {
                cache.bind(None);
            }
            if revoked {
                p.request_audio_purge();
            } else if let Some(audio) = &p.audio_cache {
                let _ = audio.bind(None);
            }
            p.scope = None;
            p.cache_operation = p.cache_operation.wrapping_add(1);
            p.cache_busy = false;
            p.cache_trim_pending = false;
            p.cache_clear_pending = false;
            restored
        };
        if !restored {
            if revoked {
                self.reload_cache_stats();
            }
            return;
        }
        if revoked {
            let mut p = self.persistence.borrow_mut();
            p.status =
                "Сохранённый вход больше не принят Яндекс Музыкой. Удаляем его с устройства."
                    .into();
            drop(p);
            self.forget_persistent_session(false);
        } else {
            let mut p = self.persistence.borrow_mut();
            p.retry = Retry::Restore;
            p.status = if matches!(error, MusicError::Network | MusicError::Timeout) {
                "Не удалось проверить сохранённый вход из-за сети. Он сохранён; можно повторить проверку или войти снова."
            } else {
                "Яндекс Музыка не подтвердила доступ для сохранённого входа. Повторите проверку или войдите снова."
            }.into();
            drop(p);
            self.render_persistence();
        }
    }
    fn persist_current_session(self: &Rc<Self>) {
        let token = {
            let model = self.model.borrow();
            if !model.account_valid || model.closed {
                return;
            }
            let Some(token) = model.token.as_ref().filter(|token| !token.is_expired()) else {
                return;
            };
            let Some(token) = token.clone_for_storage() else {
                return;
            };
            token
        };
        let task = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled
                || p.closed
                || !p.ready
                || p.critical
                || !p.values.remember_login
                || !p.supported
                || p.purge_after_forget
            {
                if p.enabled && !p.values.forget_pending {
                    p.phase = Phase::Idle;
                    p.status="Вход действует в этом запуске. Защищённое сохранение недоступно или выключено.".into();
                }
                drop(p);
                self.render_persistence();
                return;
            }
            let (Some(manager), Some(settings)) = (p.manager.clone(), p.settings.clone()) else {
                return;
            };
            let Ok(save) = settings.reserve_save() else {
                p.message = Some("Не удалось подготовить защиту сохранённого входа.".into());
                return;
            };
            let ticket = manager.begin();
            p.values.forget_pending = true;
            p.barrier_confirmed = false;
            p.critical = true;
            p.settings_operation = p.settings_operation.wrapping_add(1);
            p.settings_busy = false;
            p.settings_dirty = false;
            p.operation = p.operation.wrapping_add(1);
            p.phase = Phase::Save;
            p.retry = Retry::None;
            p.status = "Сохраняем проверенный вход в защищённом хранилище…".into();
            (
                manager,
                ticket,
                settings,
                save,
                p.values.clone(),
                p.operation,
            )
        };
        self.apply_persistence_policy();
        self.render_persistence();
        let manager = task.0.clone();
        let ticket = task.1.clone();
        let durable = DurableWork::new(self.persistence.borrow().durable_work.clone());
        self.persistence_job(
            move || {
                let _durable = durable;
                let barrier = task.2.save_if_current(&task.3, &task.4).unwrap_or(false);
                let stored = if !barrier {
                    Err(StorageError::Unconfirmed)
                } else {
                    // Remove an older remembered account before replacing it.
                    // The pending barrier suppresses restore if this fails.
                    task.0.delete(&task.1).and_then(|()| {
                        task.0.save_validated(&task.1, &token)
                    })
                };
                let mut final_values = task.4;
                final_values.forget_pending = false;
                let settings = if stored.is_ok() && task.0.is_current(&task.1) {
                    task.2.save_if_current(&task.3, &final_values).and_then(|current| {
                        if current {
                            Ok(())
                        } else {
                            Err(SettingsError::Superseded)
                        }
                    })
                } else {
                    Err(SettingsError::Unavailable)
                };
                (barrier, stored, settings)
            },
            move |this, result| {
                if this.persistence.borrow().operation != task.5 || !manager.is_current(&ticket) {
                    return;
                }
                let mut p = this.persistence.borrow_mut();
                p.phase = Phase::Idle;
                p.critical = false;
                match result {
                    Ok((barrier, Ok(()), Ok(()))) => {
                        p.barrier_confirmed = barrier;
                        p.values.forget_pending = false;
                        p.supported = true;
                        p.retry = Retry::None;
                        p.status = "Вход сохранён в защищённом хранилище. При следующем запуске он будет проверен заново.".into();
                    }
                    Ok((barrier, stored, settings)) => {
                        p.barrier_confirmed = barrier;
                        p.values.forget_pending = true;
                        p.retry = Retry::Save;
                        p.status = if !barrier {
                            "Вход работает в памяти, но защиту сохранения не удалось записать. Постоянный вход не подтверждён.".into()
                        } else if let Err(error) = stored {
                            error.user_message().into()
                        } else if settings.is_err() {
                            "Вход сохранён, но завершение операции не удалось записать. Автоматическое восстановление приостановлено до проверки.".into()
                        } else {
                            "Сохранение входа не подтверждено.".into()
                        };
                    }
                    Err(()) => {
                        p.values.forget_pending = true;
                        p.retry = Retry::Save;
                        p.status = "Не удалось подтвердить сохранение входа. Текущий сеанс продолжает работать.".into();
                    }
                }
                drop(p);
                this.apply_persistence_policy();
                this.render_persistence();
                this.flush_preferences();
            },
        );
    }
    pub(super) fn prepare_interactive_login(self: &Rc<Self>) -> bool {
        if self.persistence_blocks_login() {
            return false;
        }
        let mut p = self.persistence.borrow_mut();
        if p.enabled {
            p.operation = p.operation.wrapping_add(1);
            if let Some(manager) = &p.manager {
                manager.cancel_pending();
            }
            p.restore_ticket = None;
            p.phase = Phase::Idle;
        }
        true
    }
    pub(super) fn persistence_blocks_login(&self) -> bool {
        let p = self.persistence.borrow();
        p.enabled && (p.critical || matches!(p.phase, Phase::Setup | Phase::Probe | Phase::Restore))
    }
    pub(super) fn begin_forget_session(self: &Rc<Self>) {
        {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled {
                return;
            }
            p.scope = None;
            p.request_audio_purge();
            if let Some(cache) = &p.cache {
                cache.bind(None);
            }
        }
        self.forget_persistent_session(true);
    }
    fn forget_persistent_session(self: &Rc<Self>, purge: bool) {
        let task = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled || p.closed || (p.critical && (!purge || p.purge_after_forget)) {
                return;
            }
            p.values.forget_pending = true;
            p.barrier_confirmed = false;
            p.purge_after_forget |= purge;
            if purge {
                p.request_audio_purge();
            }
            p.critical = true;
            p.operation = p.operation.wrapping_add(1);
            p.settings_operation = p.settings_operation.wrapping_add(1);
            p.settings_busy = false;
            p.settings_dirty = false;
            p.restore_ticket = None;
            p.cache_operation = p.cache_operation.wrapping_add(1);
            p.cache_busy = false;
            p.cache_trim_pending = false;
            p.cache_clear_pending = false;
            p.phase = Phase::Forget;
            p.retry = Retry::None;
            p.status = if purge {
                "Удаляем сохранённый вход и личный кеш с устройства…"
            } else {
                "Удаляем сохранённый вход. Текущий сеанс может продолжать работу."
            }
            .into();
            if let Some(cache) = &p.cache {
                let _ = cache.configure(p.policy());
            }
            if let Some(audio) = &p.audio_cache {
                if p.audio_purge_pending {
                    let _ = audio.fence();
                } else {
                    let _ = audio.bind(None);
                }
            }
            let manager = p.manager.clone();
            let ticket = manager.as_ref().map(SessionPersistence::begin);
            let settings = p.settings.clone();
            let save = settings.as_ref().and_then(|s| s.reserve_save().ok());
            (
                manager,
                ticket,
                settings,
                save,
                p.values.clone(),
                p.cache.clone(),
                p.purge_after_forget,
                p.operation,
                p.audio_cache.clone(),
                p.audio_root.clone(),
                p.audio_policy(),
                p.audio_purge_pending,
                p.cache_budget.clone(),
                p.audio_permission.clone(),
            )
        };
        self.render_persistence();
        let expected_operation = task.7;
        let durable = DurableWork::new(self.persistence.borrow().durable_work.clone());
        self.persistence_job(
            move || {
                let _durable = durable;
                let barrier = match (&task.2, &task.3) {
                    (Some(settings), Some(save)) => settings.save_if_current(save, &task.4).unwrap_or(false),
                    _ => false,
                };
                let deletion = match (&task.0, &task.1) {
                    (Some(manager), Some(ticket)) => manager.delete(ticket),
                    _ => Err(StorageError::Unavailable),
                };
                let metadata = if task.6 {
                    task.5.as_ref().map_or(Ok(()), |cache| {
                        cache.clear().map(|_| ()).map_err(|_| ())
                    })
                } else {
                    Ok(())
                };
                // A remember-off barrier can supersede a pending Cache Clear.
                // Complete that audio intent rather than leaving its fence
                // stranded or reporting an unfinished deletion as successful.
                let audio = if task.6 || task.11 {
                    open_audio_store(task.8, task.9, task.10, task.12, task.13)
                        .and_then(|cache| cache.map_or(Ok(()), |cache| cache.logout().map(|_| ())))
                        .map_err(|_| ())
                } else {
                    Ok(())
                };
                let cache = metadata.and(audio);
                let mut final_values = task.4;
                final_values.forget_pending = false;
                let settings = if deletion.is_ok() && cache.is_ok() {
                    match (&task.2, &task.3) {
                        (Some(settings), Some(save)) => settings.save_if_current(save, &final_values).and_then(|current| {
                            if current {
                                Ok(())
                            } else {
                                Err(SettingsError::Superseded)
                            }
                        }),
                        _ => Err(SettingsError::Unavailable),
                    }
                } else {
                    Err(SettingsError::Unavailable)
                };
                Forgotten {barrier, deletion, cache, settings}
            },
            move |this, result| {
                if this.persistence.borrow().operation != expected_operation {
                    return;
                }
                let result = match result {
                    Ok(value) => value,
                    Err(()) => Forgotten {
                        barrier: false,
                        deletion: Err(StorageError::Unconfirmed),
                        cache: Err(()),
                        settings: Err(SettingsError::Unavailable),
                    },
                };
                let success = result.deletion.is_ok() && result.cache.is_ok() && result.settings.is_ok();
                {
                    let mut p = this.persistence.borrow_mut();
                    p.critical = false;
                    p.phase = Phase::Idle;
                    p.barrier_confirmed = result.barrier;
                    p.values.forget_pending = !success;
                    if success {
                        p.purge_after_forget = false;
                        p.audio_purge_pending = false;
                        p.audio_cache_stats = None;
                        p.retry = if this.model.borrow().account_valid && p.values.remember_login {
                            Retry::Save
                        } else {
                            Retry::None
                        };
                        p.status = if this.model.borrow().account_valid {
                            "Сохранённый вход удалён. Текущий сеанс продолжает работу."
                        } else {
                            "Сохранённый вход и запрошенные личные данные удалены с устройства."
                        }.into();
                    } else {
                        p.retry = Retry::Forget;
                        p.status = if !result.barrier {
                            "Не удалось сохранить защиту от повторного входа. Окончательный выход не подтверждён."
                        } else if result.deletion.is_err() {
                            "Не удалось подтвердить удаление сохранённого входа. Он не будет восстанавливаться автоматически; повторите очистку."
                        } else if result.cache.is_err() {
                            "Вход удалён, но очистка личного кеша не завершена. Повторите очистку."
                        } else {
                            "Данные удалены, но завершение операции не удалось записать. Повторите проверку."
                        }.into();
                    }
                }
                this.apply_persistence_policy();
                this.render_persistence();
                this.reload_cache_stats();
                this.flush_preferences();
                if success && !this.persistence.borrow().supported {
                    this.probe_persistence();
                }
            },
        );
    }
    fn retry_persistence(self: &Rc<Self>) {
        let action = {
            let p = self.persistence.borrow();
            if p.critical || p.phase != Phase::Idle {
                return;
            }
            p.retry
        };
        match action {
            Retry::Setup => {
                self.persistence.borrow_mut().started = false;
                self.start_persistence();
            }
            Retry::Restore => {
                if self.persistence.borrow().values.forget_pending {
                    let purge = self.persistence.borrow().purge_after_forget;
                    self.forget_persistent_session(purge);
                } else {
                    self.probe_persistence();
                }
            }
            Retry::Save => self.persist_current_session(),
            Retry::Forget => {
                let purge = self.persistence.borrow().purge_after_forget;
                self.forget_persistent_session(purge);
            }
            Retry::Settings => {
                self.persistence.borrow_mut().settings_dirty = true;
                self.flush_preferences();
            }
            Retry::None => {}
        }
    }
    pub(super) fn persistent_cache(&self) -> Option<Cache> {
        self.persistence.borrow().cache.clone()
    }
    #[cfg(test)]
    pub(super) fn install_audio_cache_for_test(&self, cache: AudioCache, scope: AccountScope) {
        let mut p = self.persistence.borrow_mut();
        assert!(!p.enabled && p.settings.is_none() && p.manager.is_none());
        assert!(!p.closed && p.audio_cache.is_none() && self.model.borrow().account_valid);
        cache.bind(Some(scope.clone())).unwrap();
        // Enable the real logout/close fences without constructing a host
        // wallet, XDG store or setup worker. Tests own the injected namespace.
        p.enabled = true;
        p.started = true;
        p.ready = true;
        p.scope = Some(scope);
        p.audio_cache = Some(cache);
    }
    pub(super) fn audio_cache_binding(&self) -> Option<(AudioCache, AudioLease)> {
        let p = self.persistence.borrow();
        if p.closed
            || !p.ready
            || !p.values.cache_enabled
            || p.values.forget_pending
            || p.audio_purge_pending
        {
            return None;
        }
        let cache = p.audio_cache.clone()?;
        let lease = cache.lease()?;
        lease
            .matches_scope(p.scope.as_ref()?)
            .then_some((cache, lease))
    }
    fn apply_persistence_policy(&self) {
        let p = self.persistence.borrow();
        if let Some(budget) = &p.cache_budget {
            let _ = budget.configure(p.cache_admission_enabled(), p.values.cache_limit.bytes());
        }
        if let Some(cache) = &p.cache {
            let _ = cache.configure(p.policy());
            if !p.values.forget_pending
                && let Some(scope) = &p.scope
                && cache.lease().is_none()
            {
                cache.bind(Some(scope.clone()));
            }
        }
        if let Some(cache) = &p.audio_cache {
            let _ = cache.configure(p.audio_policy());
            if p.audio_purge_pending {
                // request_audio_purge owns this fence. Reapplying appearance
                // or other preferences must not refence a completed worker
                // before GTK receives its successful cleanup result.
            } else if !p.ready || !p.values.cache_enabled || p.values.forget_pending {
                let _ = cache.bind(None);
            } else {
                let current = match (cache.lease(), p.scope.as_ref()) {
                    (Some(lease), Some(scope)) => lease.matches_scope(scope),
                    (None, None) => true,
                    _ => false,
                };
                if !current {
                    let _ = cache.bind(p.scope.clone());
                }
            }
        }
    }
    fn apply_appearance(&self) {
        let appearance = self.persistence.borrow().values.appearance;
        adw::StyleManager::default().set_color_scheme(match appearance {
            Appearance::System => adw::ColorScheme::Default,
            Appearance::Light => adw::ColorScheme::ForceLight,
            Appearance::Dark => adw::ColorScheme::ForceDark,
        });
    }
    fn render_persistence(&self) {
        let p = self.persistence.borrow();
        let visible = p.enabled
            && (p.phase != Phase::Idle || p.retry != Retry::None || p.values.forget_pending);
        p.banner.set_visible(visible);
        p.row.set_title(&p.status);
        p.row.set_subtitle(if p.values.forget_pending && !p.barrier_confirmed {
            "Постоянное хранение приостановлено. Завершение операции на устройстве ещё не подтверждено."
        } else if p.values.forget_pending {
            "Автоматическое восстановление и постоянный кеш приостановлены до подтверждения очистки."
        } else {
            ""
        });
        p.spinner.set_spinning(p.phase != Phase::Idle);
        p.spinner.set_visible(p.phase != Phase::Idle);
        p.retry_button.set_visible(p.retry != Retry::None);
        p.retry_button
            .set_sensitive(p.phase == Phase::Idle && !p.critical);
        if let Some(view) = &p.view {
            view.update(&p.snapshot());
        }
        drop(p);
        self.render_controls();
    }
    pub(super) fn present_preferences(self: &Rc<Self>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if self.persistence.borrow().view.is_none() {
            let changed = Rc::downgrade(self);
            let clear = Rc::downgrade(self);
            let retry = Rc::downgrade(self);
            let stats = Rc::downgrade(self);
            let cache_retry = Rc::downgrade(self);
            let callbacks = PreferencesCallbacks {
                changed: Rc::new(move |values| {
                    if let Some(this) = changed.upgrade() {
                        this.preferences_changed(values);
                    }
                }),
                clear_cache: Rc::new(move || {
                    if let Some(this) = clear.upgrade() {
                        this.clear_persistent_cache();
                    }
                }),
                retry_session: Rc::new(move || {
                    if let Some(this) = retry.upgrade() {
                        this.retry_persistence();
                    }
                }),
                reload_stats: Rc::new(move || {
                    if let Some(this) = stats.upgrade() {
                        this.reload_cache_stats();
                    }
                }),
                retry_cache: Rc::new(move || {
                    if let Some(this) = cache_retry.upgrade() {
                        this.retry_cache_operation();
                    }
                }),
            };
            let snapshot = self.persistence.borrow().snapshot();
            let view = PreferencesView::new(&snapshot, &callbacks);
            let weak = Rc::downgrade(self);
            view.dialog.connect_closed(move |dialog| {
                if let Some(this) = weak.upgrade() {
                    let mut p = this.persistence.borrow_mut();
                    if p.view.as_ref().is_some_and(|view| view.dialog == *dialog) {
                        p.view = None;
                    }
                }
            });
            self.persistence.borrow_mut().view = Some(view);
        }
        if let Some(view) = &self.persistence.borrow().view {
            view.update(&self.persistence.borrow().snapshot());
            view.present(&window);
        }
        self.reload_cache_stats();
    }
    fn preferences_changed(self: &Rc<Self>, values: Preferences) {
        let (remember_changed, cache_changed, limit_only, enabled) = {
            let mut p = self.persistence.borrow_mut();
            if p.closed || p.critical || !p.ready || values.validate().is_err() {
                return;
            }
            let previous = p.values.clone();
            p.values.appearance = values.appearance;
            p.values.track_info_open = values.track_info_open;
            p.values.cache_enabled = values.cache_enabled;
            p.values.cache_limit = values.cache_limit;
            if !values.remember_login || (p.supported && !p.values.forget_pending) {
                p.values.remember_login = values.remember_login;
            }
            p.settings_dirty = p.enabled;
            let cache_changed = previous.cache_limit != p.values.cache_limit
                || previous.cache_enabled != p.values.cache_enabled;
            let limit_only = previous.cache_limit != p.values.cache_limit
                && previous.cache_enabled == p.values.cache_enabled
                && previous.appearance == p.values.appearance
                && previous.remember_login == p.values.remember_login
                && previous.track_info_open == p.values.track_info_open;
            if cache_changed {
                if let Some(budget) = &p.cache_budget {
                    let _ =
                        budget.configure(p.cache_admission_enabled(), p.values.cache_limit.bytes());
                }
                // Local generations must retire even when a shared quota
                // change leaves the child's legacy standalone policy equal.
                if let Some(cache) = &p.cache {
                    cache.bind(p.scope.clone());
                }
                if !p.audio_purge_pending
                    && let Some(audio) = &p.audio_cache
                {
                    let _ = audio.bind(
                        p.cache_admission_enabled()
                            .then(|| p.scope.clone())
                            .flatten(),
                    );
                }
            }
            (
                previous.remember_login != p.values.remember_login,
                cache_changed,
                limit_only,
                p.enabled,
            )
        };
        if cache_changed && !self.persistence.borrow().values.cache_enabled {
            self.cancel_music_cache(crate::audio_cache::diagnostic::Cause::CacheDisabled);
        }
        self.apply_appearance();
        // Admission changes are memory-only and immediate; no in-flight writer
        // may bypass Disable while the latest disk maintenance is debounced.
        self.apply_persistence_policy();
        self.sync_info_visibility();
        if !enabled {
            self.render_persistence();
            return;
        }
        if cache_changed {
            self.schedule_cache_policy();
        }
        let remember = self.persistence.borrow().values.remember_login;
        if remember_changed && !remember {
            self.forget_persistent_session(false);
        } else {
            if !limit_only {
                self.flush_preferences();
            }
            if remember_changed && remember && self.model.borrow().account_valid {
                self.persist_current_session();
            }
            self.render_persistence();
        }
    }
    fn schedule_cache_policy(self: &Rc<Self>) {
        let revision = {
            let mut p = self.persistence.borrow_mut();
            p.cache_policy_revision = p.cache_policy_revision.wrapping_add(1);
            p.cache_policy_pending = true;
            p.cache_policy_revision
        };
        let weak = Rc::downgrade(self);
        glib::timeout_add_local_once(std::time::Duration::from_millis(200), move || {
            let Some(this) = weak.upgrade() else {
                return;
            };
            let critical = {
                let mut p = this.persistence.borrow_mut();
                if p.closed || p.cache_policy_revision != revision {
                    return;
                }
                if !p.critical {
                    p.cache_policy_pending = false;
                }
                p.critical
            };
            if critical {
                this.schedule_cache_policy();
                return;
            }
            this.flush_preferences();
            this.trim_persistent_cache();
            this.render_persistence();
        });
    }
    fn retry_cache_operation(self: &Rc<Self>) {
        let task = self.persistence.borrow().cache_retry;
        if let Some(task) = task {
            self.cache_operation(task);
        }
    }
    fn flush_preferences(self: &Rc<Self>) {
        let task = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled
                || p.closed
                || !p.ready
                || p.critical
                || p.settings_busy
                || !p.settings_dirty
            {
                return;
            }
            let Some(settings) = p.settings.clone() else {
                return;
            };
            let Ok(ticket) = settings.reserve_save() else {
                p.message = Some("Не удалось подготовить сохранение настроек.".into());
                return;
            };
            p.settings_dirty = false;
            p.queued_info = false;
            p.settings_busy = true;
            p.settings_operation = p.settings_operation.wrapping_add(1);
            (settings, ticket, p.values.clone(), p.settings_operation)
        };
        self.persistence_job(
            move || task.0.save_if_current(&task.1, &task.2),
            move |this, result| {
                let mut p = this.persistence.borrow_mut();
                if p.settings_operation != task.3 {
                    return;
                }
                p.settings_busy = false;
                if !matches!(result, Ok(Ok(true))) {
                    p.message = Some("Настройки не сохранены. Изменения действуют в этом запуске; повторите попытку после проверки каталога настроек.".into());
                    p.retry = Retry::Settings;
                } else {
                    p.message = None;
                    if p.retry == Retry::Settings {
                        p.retry = Retry::None;
                    }
                }
                drop(p);
                this.render_persistence();
                this.flush_preferences();
            },
        );
    }
    fn reload_cache_stats(self: &Rc<Self>) {
        self.cache_operation(CacheTask::Usage);
    }
    fn trim_persistent_cache(self: &Rc<Self>) {
        self.cache_operation(CacheTask::Trim);
    }
    fn clear_persistent_cache(self: &Rc<Self>) {
        self.cache_operation(CacheTask::Clear);
    }
    fn cache_operation(self: &Rc<Self>, kind: CacheTask) {
        let task = {
            let mut p = self.persistence.borrow_mut();
            if !p.enabled || p.closed || p.critical {
                return;
            }
            if matches!(kind, CacheTask::Clear) {
                self.cancel_music_cache(crate::audio_cache::diagnostic::Cause::CacheCleared);
                // Fence admission before scheduling filesystem work. An old
                // writer or returned hit cannot race a successful UI clear.
                p.request_audio_purge();
                if let Some(cache) = &p.cache {
                    let _ = cache.configure(p.policy());
                }
                if let Some(budget) = &p.cache_budget {
                    let _ = budget.configure(false, p.values.cache_limit.bytes());
                }
            }
            if p.cache_busy {
                p.cache_clear_pending |= matches!(kind, CacheTask::Clear);
                if matches!(kind, CacheTask::Trim) || p.audio_purge_pending {
                    p.cache_trim_pending = true;
                }
                return;
            }
            if p.cache.is_none() && p.audio_root.is_none() && p.audio_cache.is_none() {
                return;
            }
            p.cache_busy = true;
            p.cache_retry = None;
            p.cache_operation = p.cache_operation.wrapping_add(1);
            if matches!(kind, CacheTask::Clear) {
                p.cache_message =
                    Some("Очищаем кеш. Текущий вход и воспроизведение сохраняются.".into());
            }
            (
                p.cache.clone(),
                p.cache_operation,
                p.audio_cache.clone(),
                p.audio_root.clone(),
                p.audio_policy(),
                p.audio_purge_pending,
                p.audio_purge_generation,
                p.cache_budget.clone(),
                p.audio_permission.clone(),
            )
        };
        let operation = task.1;
        let audio_clear = task.5;
        let audio_purge_generation = task.6;
        self.render_persistence();
        self.persistence_job(
            move || {
                let metadata = task.0.as_ref().ok_or(()).and_then(|cache| match kind {
                    CacheTask::Usage => cache.usage().map_err(|_| ()),
                    CacheTask::Trim => cache.trim().map_err(|_| ()),
                    CacheTask::Clear => cache.clear().map_err(|_| ()),
                });
                let audio = open_audio_store(task.2, task.3, task.4, task.7.clone(), task.8).and_then(|cache| {
                    cache.map(|cache| {
                        let stats = if audio_clear {
                            cache.clear()?
                        } else {
                            match kind {
                                CacheTask::Usage => cache.stats()?,
                                CacheTask::Trim => cache.trim()?,
                                CacheTask::Clear => cache.clear()?,
                            }
                        };
                        Ok((cache, stats))
                    }).transpose()
                });
                let total = if let (Ok(metadata_stats), Ok(audio_result)) = (&metadata, &audio) {
                    if let Some(budget) = task.7 {
                        match kind {
                            CacheTask::Trim => budget.trim(),
                            _ => budget.usage(),
                        }.map_err(|_| ())
                    } else {
                        // Isolated injected legacy stores still report the
                        // measured sum, never a separate user-facing quota.
                        let metadata = metadata_stats;
                        let audio = audio_result.as_ref().map(|(_, stats)| *stats);
                        metadata.bytes.checked_add(audio.map_or(0, |s| s.bytes))
                            .zip(metadata.entries.checked_add(audio.map_or(0, |s| s.entries)))
                            .map(|(bytes, entries)| CacheStats { bytes, entries }).ok_or(())
                    }
                } else { Err(()) };
                CacheOutcome { total, metadata, audio }
            },
            move |this, result| {
                let mut p = this.persistence.borrow_mut();
                if p.cache_operation != operation {
                    return;
                }
                p.cache_busy = false;
                let success = match result {
                    Ok(outcome) => {
                        let success = outcome.total.is_ok() && outcome.metadata.is_ok() && outcome.audio.is_ok();
                        p.cache_stats = outcome.total.ok();
                        match outcome.audio {
                            Ok(value) => {
                                if audio_clear && p.audio_purge_generation == audio_purge_generation {
                                    p.audio_purge_pending = false;
                                }
                                if let Some((cache, stats)) = value {
                                    p.audio_cache = Some(cache);
                                    p.audio_cache_stats = Some(stats);
                                } else {
                                    p.audio_cache_stats = None;
                                }
                            }
                            Err(_) => p.audio_cache_stats = None,
                        }
                        success
                    }
                    Err(()) => {
                        p.cache_stats = None;
                        p.audio_cache_stats = None;
                        false
                    }
                };
                p.cache_retry = (!success).then_some(kind);
                if success {
                    p.cache_message = if audio_clear && !p.audio_purge_pending {
                        Some("Кеш очищен. Текущий вход и воспроизведение продолжаются.".into())
                    } else { None };
                } else {
                    p.cache_message = Some(match kind {
                        CacheTask::Usage => "Не удалось определить размер приватного кеша. Повторите проверку хранилища.",
                        CacheTask::Trim => "Не удалось применить ограничение размера кеша. Повторите проверку хранилища.",
                        CacheTask::Clear => "Не удалось полностью очистить кеш. Повторите попытку.",
                    }.into());
                }
                let trim = p.cache_trim_pending;
                let clear = p.cache_clear_pending;
                p.cache_trim_pending = false;
                p.cache_clear_pending = false;
                drop(p);
                this.apply_persistence_policy();
                this.render_persistence();
                if clear {
                    this.clear_persistent_cache();
                } else if trim {
                    this.trim_persistent_cache();
                }
            },
        );
    }
    pub(super) fn track_info_preference(&self) -> bool {
        self.persistence.borrow().values.track_info_open
    }
    pub(super) fn set_track_info_preference(self: &Rc<Self>, open: bool) {
        let mut p = self.persistence.borrow_mut();
        if p.closed {
            return;
        }
        p.values.track_info_open = open;
        p.settings_dirty = p.enabled && p.ready;
        p.queued_info |= p.critical;
        drop(p);
        self.flush_preferences();
    }
    pub(super) fn close_persistence(&self) {
        let (flush, queued_info, view) = {
            let mut p = self.persistence.borrow_mut();
            if p.closed {
                return;
            }
            p.closed = true;
            if let Some(cache) = p.audio_cache.take() {
                let _ = cache.fence();
            }
            if let Some(cache) = &p.cache {
                cache.bind(None);
            }
            if !matches!(p.phase, Phase::Save | Phase::Forget)
                && let Some(manager) = &p.manager
            {
                manager.cancel_pending();
            }
            let view = p.view.take();
            let flush = if p.enabled && p.settings_dirty && !p.critical {
                p.settings.clone().and_then(|settings| {
                    settings
                        .reserve_save()
                        .ok()
                        .map(|ticket| (settings, ticket, p.values.clone()))
                })
            } else {
                None
            };
            let queued_info = if p.critical && p.queued_info {
                p.settings
                    .clone()
                    .map(|settings| (settings, p.values.track_info_open, p.durable_work.clone()))
            } else {
                None
            };
            (flush, queued_info, view)
        };
        if let Some(view) = view {
            view.dialog.close();
        }
        if let Some((settings, ticket, values)) = flush {
            let app = self.app.upgrade();
            let hold = app.as_ref().map(|app| app.hold());
            let fence = WorkFence::new();
            glib::spawn_future_local(async move {
                let (_app, _hold, _fence) = (app, hold, fence);
                let _ =
                    gio::spawn_blocking(move || settings.save_if_current(&ticket, &values)).await;
            });
        }
        if let Some((settings, open, durable)) = queued_info {
            let app = self.app.upgrade();
            let hold = app.as_ref().map(|app| app.hold());
            let fence = WorkFence::new();
            glib::spawn_future_local(async move {
                let (_app, _hold, _fence) = (app, hold, fence);
                while durable.load(Ordering::Acquire) != 0 {
                    glib::timeout_future(std::time::Duration::from_millis(25)).await;
                }
                // Merge only this nonsecret field after the durable worker.
                // Never overwrite its final forget barrier with a UI snapshot.
                let _ = gio::spawn_blocking(move || {
                    settings.update(|values| values.track_info_open = open)
                })
                .await;
            });
        }
    }
}

#[cfg(test)]
mod tests;
