// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, independent request lanes: one running and one latest pending job.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Operation {
    Account,
    InitialLibrary,
    Library,
    Search,
    Detail,
    Delivery,
    LikeRead,
    Mutation,
}
impl Operation {
    fn index(self) -> usize {
        self as usize
    }
    pub(super) fn category(self) -> &'static str {
        match self {
            Self::Account => "account",
            Self::InitialLibrary => "initial_library",
            Self::Library => "library",
            Self::Search => "search",
            Self::Detail => "detail",
            Self::Delivery => "delivery",
            Self::LikeRead => "like_read",
            Self::Mutation => "library_mutation",
        }
    }
}

struct Slot<J> {
    latest: u64,
    running: Option<u64>,
    pending: Option<(u64, J)>,
}
impl<J> Default for Slot<J> {
    fn default() -> Self {
        Self {
            latest: 0,
            running: None,
            pending: None,
        }
    }
}

pub(super) struct Requests<J> {
    slots: [Slot<J>; 8],
}
impl<J> Default for Requests<J> {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| Slot::default()),
        }
    }
}
impl<J> Requests<J> {
    pub(super) fn submit(&mut self, operation: Operation, job: J) -> Option<(u64, J)> {
        let slot = &mut self.slots[operation.index()];
        slot.latest = slot.latest.wrapping_add(1);
        let serial = slot.latest;
        if slot.running.is_some() {
            slot.pending = Some((serial, job));
            None
        } else {
            slot.running = Some(serial);
            Some((serial, job))
        }
    }
    pub(super) fn current(&self, operation: Operation, serial: u64) -> bool {
        let slot = &self.slots[operation.index()];
        slot.latest == serial && slot.running == Some(serial)
    }
    pub(super) fn finish(&mut self, operation: Operation, serial: u64) -> Option<(u64, J)> {
        let slot = &mut self.slots[operation.index()];
        if slot.running != Some(serial) {
            return None;
        }
        slot.running = None;
        if let Some((next, job)) = slot.pending.take() {
            slot.running = Some(next);
            Some((next, job))
        } else {
            None
        }
    }
    pub(super) fn invalidate(&mut self, operation: Operation) {
        let slot = &mut self.slots[operation.index()];
        slot.latest = slot.latest.wrapping_add(1);
        slot.pending = None;
    }
    pub(super) fn invalidate_all(&mut self) {
        for operation in [
            Operation::Account,
            Operation::InitialLibrary,
            Operation::Library,
            Operation::Search,
            Operation::Detail,
            Operation::Delivery,
            Operation::LikeRead,
            Operation::Mutation,
        ] {
            self.invalidate(operation);
        }
    }
    pub(super) fn busy(&self) -> bool {
        self.slots.iter().any(|slot| {
            slot.pending.is_some() || slot.running.is_some_and(|serial| serial == slot.latest)
        })
    }

    pub(super) fn busy_except(&self, excluded: Operation) -> bool {
        self.slots.iter().enumerate().any(|(index, slot)| {
            index != excluded.index()
                && (slot.pending.is_some()
                    || slot.running.is_some_and(|serial| serial == slot.latest))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_actions_dispatch_during_another_request() {
        let mut jobs = Requests::default();
        assert!(jobs.submit(Operation::Library, "library").is_some());
        assert!(jobs.submit(Operation::Search, "search").is_some());
        assert!(jobs.submit(Operation::Detail, "playlist").is_some());
    }
    #[test]
    fn same_lane_is_bounded_and_latest_query_wins() {
        let mut jobs = Requests::default();
        let (old, _) = jobs.submit(Operation::Search, "old").unwrap();
        assert!(jobs.submit(Operation::Search, "middle").is_none());
        assert!(jobs.submit(Operation::Search, "new").is_none());
        assert!(!jobs.current(Operation::Search, old));
        let (new, value) = jobs.finish(Operation::Search, old).unwrap();
        assert_eq!(value, "new");
        assert!(jobs.current(Operation::Search, new));
        assert!(jobs.finish(Operation::Search, new).is_none());
        assert!(!jobs.busy());
    }
    #[test]
    fn editing_query_or_logout_invalidates_late_work_and_pending_jobs() {
        let mut jobs = Requests::default();
        let (old, _) = jobs.submit(Operation::Search, "old").unwrap();
        jobs.submit(Operation::Search, "pending");
        jobs.invalidate_all();
        assert!(!jobs.current(Operation::Search, old));
        assert!(jobs.finish(Operation::Search, old).is_none());
        let (new, _) = jobs.submit(Operation::Search, "new session").unwrap();
        assert!(!jobs.current(Operation::Search, old));
        assert!(jobs.current(Operation::Search, new));
        assert!(jobs.finish(Operation::Search, old).is_none());
        assert!(jobs.current(Operation::Search, new));
    }
    #[test]
    fn invalidation_hides_obsolete_work_without_releasing_its_lane() {
        let mut jobs = Requests::default();
        let (old, _) = jobs.submit(Operation::Search, "old").unwrap();
        let (detail, _) = jobs.submit(Operation::Detail, "old detail").unwrap();
        jobs.invalidate_all();
        assert!(!jobs.busy());
        assert!(jobs.submit(Operation::Search, "new").is_none());
        assert!(jobs.busy());
        assert!(jobs.finish(Operation::Detail, detail).is_none());
        assert!(jobs.busy());
        let (fresh, value) = jobs.finish(Operation::Search, old).unwrap();
        assert_eq!(value, "new");
        assert!(jobs.current(Operation::Search, fresh));
    }
}
