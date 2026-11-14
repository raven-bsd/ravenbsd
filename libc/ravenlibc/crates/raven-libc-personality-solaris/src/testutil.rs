// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Deterministic fakes for every seam (ticket C67 4.2).
//!
//! Test-only: the module is compiled under `#[cfg(test)]` and never ships. The integration
//! tests under `tests/` carry their own minimal copies because `cfg(test)` items are not
//! visible to them.

#![allow(
    missing_docs,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::seams::{
    AuxSrc, CondT, DoorbellOps, Errno, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, Handle, KeyDtor,
    MutexT, PosixThreads, RwlockT, SemaT, SigSet, StackPlan, StartFn, ThreadAttrPlan, Timespec,
};

/// A fake fd table: descriptors start at 3 and epochs increase on every install.
#[derive(Clone, Debug)]
pub struct FakeFdTable {
    entries: BTreeMap<i32, FakeEntry>,
    next_fd: i32,
    next_epoch: u32,
    /// When set, `install` returns this error.
    pub install_error: Option<Errno>,
}

/// One fake descriptor.
#[derive(Clone, Copy, Debug)]
pub struct FakeEntry {
    /// The kernel handle.
    pub handle: Handle,
    /// The descriptor kind.
    pub kind: FdKind,
    /// The generation.
    pub epoch: u32,
    /// `FD_CLOEXEC`.
    pub cloexec: bool,
}

impl FakeFdTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            next_fd: 3,
            next_epoch: 1,
            install_error: None,
        }
    }

    /// Install at a chosen number (test setup; reuses the number with a new epoch).
    pub fn insert_at(&mut self, fd: i32, handle: Handle, kind: FdKind) {
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.wrapping_add(1).max(1);
        self.entries.insert(
            fd,
            FakeEntry {
                handle,
                kind,
                epoch,
                cloexec: false,
            },
        );
        self.next_fd = self.next_fd.max(fd.saturating_add(1));
    }

    /// Install with the next free number.
    pub fn insert(&mut self, handle: Handle, kind: FdKind) -> i32 {
        let mut fd = self.next_fd;
        while self.entries.contains_key(&fd) {
            fd = fd.saturating_add(1);
        }
        self.insert_at(fd, handle, kind);
        fd
    }

    /// Close one descriptor (the number can then be reused).
    pub fn close(&mut self, fd: i32) -> bool {
        self.entries.remove(&fd).is_some()
    }

    /// Whether `fd` is open.
    #[must_use]
    pub fn is_open(&self, fd: i32) -> bool {
        self.entries.contains_key(&fd)
    }

    /// The number of open descriptors.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no descriptor is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for FakeFdTable {
    fn default() -> Self {
        Self::new()
    }
}

impl FdTable for FakeFdTable {
    fn handle_of(&self, fd: i32) -> Result<Handle, Errno> {
        self.entries
            .get(&fd)
            .map(|entry| entry.handle)
            .ok_or(Errno::EBADF)
    }

    fn install(&mut self, handle: Handle, kind: FdKind, cloexec: bool) -> Result<i32, Errno> {
        if let Some(error) = self.install_error {
            return Err(error);
        }
        let fd = self.insert(handle, kind);
        if let Some(entry) = self.entries.get_mut(&fd) {
            entry.cloexec = cloexec;
        }
        Ok(fd)
    }

    fn kind_of(&self, fd: i32) -> Result<FdKind, Errno> {
        self.entries
            .get(&fd)
            .map(|entry| entry.kind)
            .ok_or(Errno::EBADF)
    }

    fn epoch(&self, fd: i32) -> u32 {
        self.entries.get(&fd).map_or(0, |entry| entry.epoch)
    }
}

/// A scripted event queue.
#[derive(Clone, Debug)]
pub struct FakeEvq {
    /// Queue handle.
    pub queue: Handle,
    /// Pending events returned by `wait`.
    pub events: Vec<EvqEvent>,
    /// Every `ctl` call.
    pub ops: Vec<(EvqCtl, Handle, u32, u64)>,
    /// Every timeout passed to `wait`.
    pub timeouts: Vec<u64>,
    /// When set, `create` returns this error.
    pub create_error: Option<Errno>,
    /// When set, the next `wait` returns this error.
    pub wait_error: Option<Errno>,
    /// When set, `ctl` returns this error.
    pub ctl_error: Option<Errno>,
}

impl FakeEvq {
    /// A queue with no events.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            queue: Handle(100),
            events: Vec::new(),
            ops: Vec::new(),
            timeouts: Vec::new(),
            create_error: None,
            wait_error: None,
            ctl_error: None,
        }
    }

    /// Append one event with the given cookie/mask.
    pub fn push(&mut self, cookie: u64, events: u32) {
        self.events.push(EvqEvent {
            cookie,
            events,
            pad: 0,
        });
    }

    /// The last timeout passed to `wait`.
    #[must_use]
    pub fn last_timeout(&self) -> Option<u64> {
        self.timeouts.last().copied()
    }

    /// Number of registrations currently recorded (rough model for assertions).
    #[must_use]
    pub fn registrations(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _, _, _)| *op != EvqCtl::Del)
            .count()
    }

    /// Number of `DEL` operations seen.
    #[must_use]
    pub fn deletions(&self) -> usize {
        self.ops
            .iter()
            .filter(|(op, _, _, _)| *op == EvqCtl::Del)
            .count()
    }
}

impl Default for FakeEvq {
    fn default() -> Self {
        Self::new()
    }
}

impl EvqOps for FakeEvq {
    fn create(&mut self) -> Result<Handle, Errno> {
        if let Some(error) = self.create_error {
            return Err(error);
        }
        Ok(self.queue)
    }

    fn ctl(
        &mut self,
        q: Handle,
        op: EvqCtl,
        h: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno> {
        if q != self.queue {
            return Err(Errno::EBADF);
        }
        if let Some(error) = self.ctl_error {
            return Err(error);
        }
        self.ops.push((op, h, events, cookie));
        Ok(())
    }

    fn wait(&mut self, q: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
        if q != self.queue {
            return Err(Errno::EBADF);
        }
        self.timeouts.push(timeout_ns);
        if let Some(error) = self.wait_error {
            return Err(error);
        }
        let count = out.len().min(self.events.len());
        for (index, slot) in out.iter_mut().take(count).enumerate() {
            *slot = self.events[index];
        }
        self.events.drain(..count);
        Ok(count)
    }
}

/// A fake user-event source.
#[derive(Clone, Debug)]
pub struct FakeDoorbell {
    /// Number of created handles.
    pub created: usize,
    /// Handles signalled.
    pub signals: Vec<Handle>,
    /// When set, `signal` returns this error.
    pub signal_error: Option<Errno>,
    /// When set, `create` returns this error.
    pub create_error: Option<Errno>,
}

impl FakeDoorbell {
    /// A working source.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            created: 0,
            signals: Vec::new(),
            signal_error: None,
            create_error: None,
        }
    }
}

impl Default for FakeDoorbell {
    fn default() -> Self {
        Self::new()
    }
}

impl DoorbellOps for FakeDoorbell {
    fn create(&mut self) -> Result<Handle, Errno> {
        if let Some(error) = self.create_error {
            return Err(error);
        }
        self.created = self.created.saturating_add(1);
        Ok(Handle(200u32.saturating_add(self.created as u32)))
    }

    fn signal(&mut self, h: Handle, _value: u64) -> Result<(), Errno> {
        if let Some(error) = self.signal_error {
            return Err(error);
        }
        self.signals.push(h);
        Ok(())
    }
}

/// A fake auxiliary vector.
#[derive(Clone, Debug, Default)]
pub struct FakeAux {
    values: BTreeMap<u64, u64>,
}

impl FakeAux {
    /// An empty vector.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    /// Publish one key.
    pub fn set(&mut self, key: u64, value: u64) {
        self.values.insert(key, value);
    }
}

impl AuxSrc for FakeAux {
    fn get(&self, key: u64) -> Option<u64> {
        self.values.get(&key).copied()
    }
}

/// A recording `PosixThreads` fake: every wrapper test asserts the one call it makes.
#[derive(Clone, Debug)]
pub struct RecordingThreads {
    /// The names of the calls, in order.
    pub calls: Vec<&'static str>,
    /// Returned by `create`.
    pub next_thread: usize,
    /// Returned by `self_`.
    pub self_id: usize,
    /// Returned by `is_initial`.
    pub initial: bool,
    /// The priority `getprio` returns.
    pub priority: i32,
    /// The value `join` returns.
    pub join_value: usize,
    /// The next key `key_create` returns.
    pub next_key: u32,
    /// The TSD store.
    pub specific: BTreeMap<u32, usize>,
    /// When set, every operation returns this error.
    pub error: Option<Errno>,
    /// When set, `mutex_trylock` returns this error.
    pub trylock_error: Option<Errno>,
    /// When set, `cond_timedwait` returns this error.
    pub timedwait_error: Option<Errno>,
    /// When set, `rwlock_tryrdlock` returns this error.
    pub tryrdlock_error: Option<Errno>,
    /// When set, `rwlock_trywrlock` returns this error.
    pub trywrlock_error: Option<Errno>,
    /// When set, `sem_trywait` returns this error.
    pub sem_trywait_error: Option<Errno>,
    /// The plan of the last `create`.
    pub created_plan: Option<ThreadAttrPlan>,
    /// The argument of the last `create`.
    pub created_arg: Option<usize>,
    /// The last mask operation.
    pub mask: Option<(i32, Option<SigSet>)>,
    /// The removed status of `exit`, before it diverges.
    pub exit_status: Option<usize>,
    /// Mutex `pshared` values in order.
    pub mutex_pshared: Vec<bool>,
    /// Condition-variable `pshared` values in order.
    pub cond_pshared: Vec<bool>,
    /// Reader/writer-lock `pshared` values in order.
    pub rwlock_pshared: Vec<bool>,
    /// Semaphore `(pshared, count)` in order.
    pub semaphores: Vec<(bool, u32)>,
    /// `setprio` calls.
    pub priorities: Vec<(usize, i32)>,
    /// `yield` count.
    pub yields: usize,
}

impl RecordingThreads {
    /// A fake for the initial thread with no recorded calls.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            calls: Vec::new(),
            next_thread: 42,
            self_id: 1,
            initial: true,
            priority: 0,
            join_value: 7,
            next_key: 9,
            specific: BTreeMap::new(),
            error: None,
            trylock_error: None,
            timedwait_error: None,
            tryrdlock_error: None,
            trywrlock_error: None,
            sem_trywait_error: None,
            created_plan: None,
            created_arg: None,
            mask: None,
            exit_status: None,
            mutex_pshared: Vec::new(),
            cond_pshared: Vec::new(),
            rwlock_pshared: Vec::new(),
            semaphores: Vec::new(),
            priorities: Vec::new(),
            yields: 0,
        }
    }

    /// Push a call name and return the shared error, if any.
    fn enter(&mut self, name: &'static str) -> Result<(), Errno> {
        self.calls.push(name);
        match self.error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    /// How many times `name` was called.
    #[must_use]
    pub fn count(&self, name: &str) -> usize {
        self.calls.iter().filter(|call| **call == name).count()
    }
}

impl Default for RecordingThreads {
    fn default() -> Self {
        Self::new()
    }
}

impl PosixThreads for RecordingThreads {
    fn create(
        &mut self,
        attr: &ThreadAttrPlan,
        _start: StartFn,
        arg: usize,
    ) -> Result<usize, Errno> {
        self.enter("create")?;
        self.created_plan = Some(*attr);
        self.created_arg = Some(arg);
        Ok(self.next_thread)
    }

    fn join(&mut self, t: usize) -> Result<usize, Errno> {
        self.enter("join")?;
        self.calls
            .push(if t == 0 { "join-zero" } else { "join-id" });
        Ok(self.join_value)
    }

    fn self_(&self) -> usize {
        self.self_id
    }

    fn is_initial(&self) -> bool {
        self.initial
    }

    fn yield_(&mut self) {
        self.calls.push("yield");
        self.yields = self.yields.saturating_add(1);
    }

    fn kill(&mut self, _t: usize, _sig: i32) -> Result<(), Errno> {
        self.enter("kill")
    }

    fn sigmask(
        &mut self,
        how: i32,
        set: Option<&SigSet>,
        _old: Option<&mut SigSet>,
    ) -> Result<(), Errno> {
        self.enter("sigmask")?;
        self.mask = Some((how, set.copied()));
        Ok(())
    }

    fn exit(&mut self, status: usize) -> ! {
        self.calls.push("exit");
        self.exit_status = Some(status);
        panic!("RecordingThreads::exit");
    }

    fn mutex_init(&mut self, _m: &mut MutexT, pshared: bool) -> Result<(), Errno> {
        self.enter("mutex_init")?;
        self.mutex_pshared.push(pshared);
        Ok(())
    }

    fn mutex_lock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        self.enter("mutex_lock")
    }

    fn mutex_trylock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        self.enter("mutex_trylock")?;
        match self.trylock_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn mutex_unlock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        self.enter("mutex_unlock")
    }

    fn mutex_destroy(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        self.enter("mutex_destroy")
    }

    fn cond_init(&mut self, _c: &mut CondT, pshared: bool) -> Result<(), Errno> {
        self.enter("cond_init")?;
        self.cond_pshared.push(pshared);
        Ok(())
    }

    fn cond_wait(&mut self, _c: &mut CondT, _m: &mut MutexT) -> Result<(), Errno> {
        self.enter("cond_wait")
    }

    fn cond_timedwait(
        &mut self,
        _c: &mut CondT,
        _m: &mut MutexT,
        _abstime: &Timespec,
    ) -> Result<(), Errno> {
        self.enter("cond_timedwait")?;
        match self.timedwait_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn cond_signal(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        self.enter("cond_signal")
    }

    fn cond_broadcast(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        self.enter("cond_broadcast")
    }

    fn cond_destroy(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        self.enter("cond_destroy")
    }

    fn rwlock_init(&mut self, _r: &mut RwlockT, pshared: bool) -> Result<(), Errno> {
        self.enter("rwlock_init")?;
        self.rwlock_pshared.push(pshared);
        Ok(())
    }

    fn rwlock_rdlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        self.enter("rwlock_rdlock")
    }

    fn rwlock_wrlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        self.enter("rwlock_wrlock")
    }

    fn rwlock_tryrdlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        self.enter("rwlock_tryrdlock")?;
        match self.tryrdlock_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn rwlock_trywrlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        self.enter("rwlock_trywrlock")?;
        match self.trywrlock_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn rwlock_unlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        self.enter("rwlock_unlock")
    }

    fn rwlock_destroy(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        self.enter("rwlock_destroy")
    }

    fn sem_init(&mut self, _s: &mut SemaT, pshared: bool, count: u32) -> Result<(), Errno> {
        self.enter("sem_init")?;
        self.semaphores.push((pshared, count));
        Ok(())
    }

    fn sem_destroy(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        self.enter("sem_destroy")
    }

    fn sem_wait(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        self.enter("sem_wait")
    }

    fn sem_trywait(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        self.enter("sem_trywait")?;
        match self.sem_trywait_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn sem_post(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        self.enter("sem_post")
    }

    fn key_create(&mut self, _dtor: Option<KeyDtor>) -> Result<u32, Errno> {
        self.enter("key_create")?;
        Ok(self.next_key)
    }

    fn setspecific(&mut self, key: u32, value: usize) -> Result<(), Errno> {
        self.enter("setspecific")?;
        self.specific.insert(key, value);
        Ok(())
    }

    fn getspecific(&mut self, key: u32) -> Result<usize, Errno> {
        self.enter("getspecific")?;
        self.specific.get(&key).copied().ok_or(Errno::ENOENT)
    }

    fn getprio(&mut self, _t: usize) -> Result<i32, Errno> {
        self.enter("getprio")?;
        Ok(self.priority)
    }

    fn setprio(&mut self, t: usize, prio: i32) -> Result<(), Errno> {
        self.enter("setprio")?;
        self.priorities.push((t, prio));
        Ok(())
    }
}

/// A `PosixThreads` implementation that answers `ENOSYS` like the shell before C57.
///
/// This is the documented production shape (C67 4.2 fallback (b)); it never starts a thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoThreads;

impl PosixThreads for NoThreads {
    fn create(
        &mut self,
        _attr: &ThreadAttrPlan,
        _start: StartFn,
        _arg: usize,
    ) -> Result<usize, Errno> {
        Err(Errno::ENOSYS)
    }

    fn join(&mut self, _t: usize) -> Result<usize, Errno> {
        Err(Errno::ENOSYS)
    }

    fn self_(&self) -> usize {
        0
    }

    fn is_initial(&self) -> bool {
        true
    }

    fn yield_(&mut self) {}

    fn kill(&mut self, _t: usize, _sig: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sigmask(
        &mut self,
        _how: i32,
        _set: Option<&SigSet>,
        _old: Option<&mut SigSet>,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn exit(&mut self, _status: usize) -> ! {
        panic!("NoThreads::exit");
    }

    fn mutex_init(&mut self, _m: &mut MutexT, _pshared: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_lock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_trylock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_unlock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_destroy(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_init(&mut self, _c: &mut CondT, _pshared: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_wait(&mut self, _c: &mut CondT, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_timedwait(
        &mut self,
        _c: &mut CondT,
        _m: &mut MutexT,
        _abstime: &Timespec,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_signal(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_broadcast(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_destroy(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_init(&mut self, _r: &mut RwlockT, _pshared: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_rdlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_wrlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_tryrdlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_trywrlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_unlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_destroy(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_init(&mut self, _s: &mut SemaT, _pshared: bool, _count: u32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_destroy(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_wait(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_trywait(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_post(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn key_create(&mut self, _dtor: Option<KeyDtor>) -> Result<u32, Errno> {
        Err(Errno::ENOSYS)
    }

    fn setspecific(&mut self, _key: u32, _value: usize) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn getspecific(&mut self, _key: u32) -> Result<usize, Errno> {
        Err(Errno::ENOSYS)
    }

    fn getprio(&mut self, _t: usize) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    fn setprio(&mut self, _t: usize, _prio: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

/// A `StartFn` for tests (never started by the fake).
pub extern "C" fn no_start(_arg: usize) -> usize {
    0
}

/// A stack plan helper for assertions.
#[must_use]
pub fn plan(detached: bool, stack: StackPlan) -> ThreadAttrPlan {
    ThreadAttrPlan { detached, stack }
}
