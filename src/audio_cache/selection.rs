// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded cache work overlaps authenticated offer admission. Disk/integrity
//! checks run off GTK; source selection may await this single worker briefly
//! before deciding whether a fresh local representation avoids CDN resolution.
use super::*;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::SystemTime;

pub(crate) struct Lookup {
    result: Receiver<Result<PreparedSelection>>,
    abandoned: Arc<AtomicBool>,
    deadline: Instant,
}
impl Lookup {
    /// No wait or later switch: a slow/unavailable cache leaves this selection
    /// online. Dropping the receiver retires unused prepared work.
    #[cfg(test)]
    pub(crate) fn ready(self) -> Option<PreparedSelection> {
        self.result.try_recv().ok()?.ok()
    }

    /// Called only on the existing resolution worker after the API request.
    /// Most reads have already finished in parallel. A busy/slow cache has a
    /// small bounded allowance, then ordinary online resolution continues.
    pub(crate) fn wait(self, current: impl Fn() -> bool) -> Option<PreparedSelection> {
        loop {
            if !current() {
                return None;
            }
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            match self
                .result
                .recv_timeout(remaining.min(Duration::from_millis(10)))
            {
                Ok(result) => return result.ok(),
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(mpsc::RecvTimeoutError::Timeout) if Instant::now() >= self.deadline => {
                    return None;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
        }
    }
}
impl Drop for Lookup {
    fn drop(&mut self) {
        self.abandoned.store(true, Ordering::Release);
    }
}
pub(crate) struct PreparedSelection {
    cache: AudioCache,
    lease: Lease,
    prepared: VerifiedOnline,
    file: Option<Arc<File>>,
    http: Option<http_policy::Metadata>,
    http_verified: bool,
    admission_deadline: Option<Instant>,
    rebase: Option<http_policy::Metadata>,
    touch: Option<Sender<Used>>,
}
/// Distinguish a proved version change from merely uncomparable locators.
pub(crate) enum Revalidation {
    Same,
    Different(http_policy::Reason),
    Unproven(http_policy::Reason),
    RefreshFresh(http_policy::Reason),
}
struct Used {
    permit: StorageGrant,
    rebase: Option<http_policy::Metadata>,
}
struct Slot(Arc<AtomicBool>);
impl Drop for Slot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
impl AudioCache {
    #[cfg(test)]
    pub(crate) fn lookup_settled_for_test(&self) -> bool {
        !self.0.lookup_running.load(Ordering::Acquire)
    }

    /// One cache worker per namespace, including abandoned or slow reads. New
    /// selections skip a busy cache; they cannot accumulate threads/jobs.
    pub(crate) fn lookup(&self, lease: Lease, check: OnlineCheck) -> Option<Lookup> {
        let deadline = Instant::now() + Duration::from_millis(250);
        self.lease_current(&lease).ok()?;
        self.0
            .lookup_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        let slot = Slot(self.0.lookup_running.clone());
        let work = self.clone();
        let (sender, result) = mpsc::sync_channel(1);
        let abandoned = Arc::new(AtomicBool::new(false));
        let worker_abandoned = abandoned.clone();
        std::thread::Builder::new()
            .name("karin-cache-read".into())
            .spawn(move || {
                let _slot = slot;
                let cache = work;
                let prepared = (|| {
                    cache.lease_current(&lease)?;
                    let prepared = cache.0.authority.prepare_online(check)?;
                    cache.lease_current(&lease)?;
                    if worker_abandoned.load(Ordering::Acquire) {
                        return Err(CacheError::Cancelled);
                    }
                    let identity = prepared.permission.identity;
                    if identity != cache.identity(&lease, identity.track)? {
                        return Err(CacheError::Cancelled);
                    }
                    cache.0.authority.check_storage_prepared(&prepared)?;
                    let (file, http) = {
                        let _io = cache.io()?;
                        cache.lease_current(&lease)?;
                        let file = read_record(
                            &cache.path(identity.key()),
                            identity.key(),
                            Some(identity.scope()),
                            prepared.permission.content,
                        )?;
                        let http = if cache.0.authority.private_http() && file.is_some() {
                            read_http_record(&cache.path(identity.key()))?
                        } else {
                            None
                        };
                        let file = if cache.0.authority.private_http() && http.is_none() {
                            None
                        } else {
                            file
                        };
                        (file, http)
                    };
                    cache.lease_current(&lease)?;
                    if worker_abandoned.load(Ordering::Acquire) {
                        return Err(CacheError::Cancelled);
                    }
                    let (touch, used) = mpsc::channel();
                    Ok((
                        PreparedSelection {
                            cache: cache.clone(),
                            lease: lease.clone(),
                            prepared,
                            file: file.map(Arc::new),
                            http,
                            http_verified: false,
                            admission_deadline: None,
                            rebase: None,
                            touch: Some(touch),
                        },
                        used,
                        identity,
                    ))
                })();
                match prepared {
                    Ok((prepared, used, identity)) => {
                        let has_file = prepared.file.is_some();
                        let original = prepared.file.as_ref().zip(prepared.http.as_ref()).and_then(
                            |(file, metadata)| {
                                file.metadata()
                                    .ok()
                                    .map(|stamp| (Arc::clone(file), stamp, metadata.clone()))
                            },
                        );
                        let published = sender.send(Ok(prepared)).is_ok();
                        #[cfg(test)]
                        if published {
                            // A fixture may order its HTTP response after the
                            // actual selection message, never after a guessed
                            // elapsed delay or the later LRU worker shutdown.
                            cache.0.lookup_publications.fetch_add(1, Ordering::Release);
                        }
                        if published
                            && has_file
                            && let Ok(used) = used.recv_timeout(Duration::from_secs(35))
                            && cache.lease_current(&lease).is_ok()
                        {
                            if let (Some(current), Some((media, stamp, previous))) =
                                (used.rebase, original)
                                && cache.rebase_http_record(
                                    &lease,
                                    HttpRebase {
                                        permit: used.permit,
                                        media,
                                        stamp,
                                        previous,
                                        current,
                                    },
                                ) == Ok(true)
                            {
                                diagnostic::report(
                                    diagnostic::Event::Checking,
                                    diagnostic::Cause::BindingRebased,
                                );
                            }
                            // Bookkeeping follows admission on this existing worker.
                            // Even a stalled filesystem cannot delay the audio path.
                            let path = cache.path(identity.key());
                            if let Some(budget) = &cache.0.shared {
                                let _ = budget.try_touch(&path);
                            } else {
                                let _ = cache.touch(&path);
                            }
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(Err(error));
                    }
                }
                drop(cache);
            })
            .ok()?;
        Some(Lookup {
            result,
            abandoned,
            deadline,
        })
    }
}
impl PreparedSelection {
    pub(crate) fn has_file(&self) -> bool {
        self.file.is_some()
    }

    pub(crate) fn needs_initial_refresh(&self) -> bool {
        self.http
            .as_ref()
            .is_some_and(http_policy::Metadata::needs_initial_refresh)
    }

    /// Current authenticated offer and private HTTP freshness, not an old
    /// signed locator, admit this exact integrity-checked representation.
    pub(crate) fn reuse_fresh(
        &mut self,
        identity: http_policy::ApplicationIdentity,
        offer_expires: Instant,
    ) -> std::result::Result<http_policy::Freshness, http_policy::Reason> {
        let checked_at = Instant::now();
        self.cache
            .lease_current(&self.lease)
            .map_err(|_| http_policy::Reason::Cancelled)?;
        if !self.cache.0.authority.private_http() || self.file.is_none() {
            return Err(http_policy::Reason::ApplicationUnverified);
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| http_policy::Reason::ClockUncertain)?;
        // Round resident wall time upward, complementing the conservative
        // monotonic request-delay rounding at receipt.
        let now = now
            .as_secs()
            .checked_add(u64::from(now.subsec_nanos() != 0))
            .ok_or(http_policy::Reason::ClockUncertain)?;
        let result = self
            .http
            .as_ref()
            .ok_or(http_policy::Reason::FreshnessMissing)?
            .fresh_for(identity, now)?;
        self.http_verified = true;
        self.admission_deadline = Some(
            checked_at
                .checked_add(result.remaining)
                .map_or(offer_expires, |http| http.min(offer_expires)),
        );
        Ok(result.kind)
    }

    pub(crate) fn revalidate_http(
        &mut self,
        resolver: &crate::stream::StreamResolver,
        remote: &crate::playback::RemotePlaybackSource,
        token: &crate::session::AccessToken,
        cancellation: &crate::stream::DeliveryCancellation,
    ) -> std::result::Result<Revalidation, http_policy::Reason> {
        if !self.cache.0.authority.private_http() || self.file.is_none() {
            return Ok(Revalidation::Same);
        }
        self.cache
            .lease_current(&self.lease)
            .map_err(|_| http_policy::Reason::Cancelled)?;
        let metadata = self
            .http
            .as_ref()
            .ok_or(http_policy::Reason::ValidatorsMissing)?;
        super::diagnostic::identity(metadata, remote);
        let validated = resolver.revalidate_http(remote, metadata, token, cancellation)?;
        self.cache
            .lease_current(&self.lease)
            .map_err(|_| http_policy::Reason::Cancelled)?;
        Ok(match validated {
            http_policy::Validation::Same(fresh) => {
                self.http_verified = true;
                // Rebase current validators together with their URI. Never
                // attach a new locator to the old replica's opaque ETag.
                if metadata.uri_hash != fresh.uri_hash
                    || metadata.resource_hash != fresh.resource_hash
                    || metadata.binding != fresh.binding
                    || metadata.etag != fresh.etag
                    || metadata.last_modified != fresh.last_modified
                    || metadata.modified_unix != fresh.modified_unix
                    || metadata.directives != fresh.directives
                    || metadata.observed != fresh.observed
                {
                    self.rebase = Some(*fresh);
                }
                Revalidation::Same
            }
            http_policy::Validation::Different(reason) => Revalidation::Different(reason),
            http_policy::Validation::Unproven(reason) => Revalidation::Unproven(reason),
            http_policy::Validation::RefreshFresh(reason) => Revalidation::RefreshFresh(reason),
        })
    }

    pub(crate) fn discard_file(&mut self) {
        self.file = None;
        self.http = None;
        self.http_verified = false;
        self.admission_deadline = None;
        self.rebase = None;
    }

    pub(crate) fn retire_record(&mut self) -> Result<()> {
        self.cache.lease_current(&self.lease)?;
        self.cache
            .0
            .authority
            .check_storage_prepared(&self.prepared)?;
        // Fence refused-resource writes before any contended disk work.
        self.cache.revoke_resource_writes(
            &self.lease,
            self.prepared.permission.identity,
            self.prepared.check.sequence,
        );
        let _io = self.cache.io()?;
        self.cache.lease_current(&self.lease)?;
        self.cache
            .0
            .authority
            .check_storage_prepared(&self.prepared)?;
        remove_record(&self.cache.path(self.prepared.permission.identity.key()))?;
        drop(_io);
        self.discard_file();
        Ok(())
    }

    /// Fresh authenticated source resolution is required before this memory-only
    /// commit. No cached bytes become a playable Hit before it succeeds.
    pub(crate) fn admit(self, current: impl Fn() -> bool) -> Result<Option<Hit>> {
        let Self {
            cache,
            lease,
            prepared,
            file,
            touch,
            http: _,
            http_verified,
            admission_deadline,
            rebase,
        } = self;
        if cache.0.authority.private_http() && file.is_some() && !http_verified {
            return Err(CacheError::NeedsOnline);
        }
        let identity = prepared.permission.identity;
        {
            let state = cache.0.state.lock().map_err(|_| CacheError::Unavailable)?;
            cache.lease_current_locked(&lease, &state)?;
            if !current() {
                return Err(CacheError::Cancelled);
            }
            cache.0.authority.commit_online(prepared)?;
        }
        let permit = cache.0.authority.authorize(&identity)?;
        if permit.identity != identity {
            return Err(CacheError::Cancelled);
        }
        cache.check(&lease, permit)?;
        let hit = file.map(|file| Hit {
            owner: Arc::downgrade(&cache.0),
            lease,
            permit,
            file,
            admission_deadline,
        });
        if hit.is_some()
            && let Some(touch) = touch
        {
            let _ = touch.send(Used { permit, rebase });
        }
        Ok(hit)
    }
}

#[cfg(test)]
mod tests;
