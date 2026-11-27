// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `kqueue`/`kevent` over the Raven event queue (`B01`, `15` 4.3, `11` 3.6 decision 5).
//!
//! S743 pins the declared subset to FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`:
//!
//! * `EVFILT_READ`/`EVFILT_WRITE` over a pollable descriptor (`EVQ` readiness),
//! * `EVFILT_TIMER` over `TIMER_CREATE`/`TIMER_SET` (relative timers; `NOTE_ABSTIME` is
//!   `EOPNOTSUPP` because the kernel timer takes a relative deadline only),
//! * `EVFILT_USER` over `EVENT_CREATE`/`EVENT_SIGNAL` without the `NOTE_FF*` state machine
//!   (`EOPNOTSUPP`, never a silently ignored control word),
//! * `EVFILT_PROC` with exactly `NOTE_EXIT` over a process handle (`PG-44`).
//!
//! `EVFILT_SIGNAL` needs `PG-7`, `EVFILT_VNODE` needs `PG-38`; the other pinned filter numbers
//! (`AIO`, `PROCDESC`, `FS`, `LIO`, `SENDFILE`, `EMPTY`, `JAIL`, `JAILDESC`) need services this
//! build does not have. Every unsupported pinned filter returns `EOPNOTSUPP` per change, and a
//! number outside `-EVFILT_SYSCOUNT..=-1` returns `EINVAL`, exactly as `kqueue_fo_find` does.
//!
//! The declared entry points are `kqueue`, `kqueue1` and `kevent` (the frozen `RAVEN_1.0`
//! symbols); `kqueuex` is outside this subset, so the installed header does not declare it.
//!
//! The Raven event queue keeps one registration per *handle* in a queue, while FreeBSD keeps one
//! knote per `(ident, filter)`. The personality therefore folds the descriptors's filters into
//! one readiness registration (the union of their filter masks) and dispatches one `kevent`
//! record per filter. `EV_CLEAR` is the queue's edge mode, so a descriptor whose filters mix
//! `EV_CLEAR` with level filters is registered edge for all of them; that is the one semantic
//! deviation from the donor and is asserted in the test matrix.
//!
//! The constants and the 64-byte `struct kevent` are C38's installed `sys/event.h`; the unit
//! test parses that header and asserts every number and the field offsets.

use alloc::collections::BTreeMap;

use raven_libc_abi::fcntl::{O_CLOEXEC, O_NONBLOCK};

use crate::seams::{Errno, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, Handle, SrcOps, Timespec};

/// `EVFILT_READ`.
pub const EVFILT_READ: i16 = -1;
/// `EVFILT_WRITE`.
pub const EVFILT_WRITE: i16 = -2;
/// `EVFILT_AIO`.
pub const EVFILT_AIO: i16 = -3;
/// `EVFILT_VNODE`.
pub const EVFILT_VNODE: i16 = -4;
/// `EVFILT_PROC`.
pub const EVFILT_PROC: i16 = -5;
/// `EVFILT_SIGNAL`.
pub const EVFILT_SIGNAL: i16 = -6;
/// `EVFILT_TIMER`.
pub const EVFILT_TIMER: i16 = -7;
/// `EVFILT_PROCDESC`.
pub const EVFILT_PROCDESC: i16 = -8;
/// `EVFILT_FS`.
pub const EVFILT_FS: i16 = -9;
/// `EVFILT_LIO`.
pub const EVFILT_LIO: i16 = -10;
/// `EVFILT_USER`.
pub const EVFILT_USER: i16 = -11;
/// `EVFILT_SENDFILE`.
pub const EVFILT_SENDFILE: i16 = -12;
/// `EVFILT_EMPTY`.
pub const EVFILT_EMPTY: i16 = -13;
/// `EVFILT_JAIL`.
pub const EVFILT_JAIL: i16 = -14;
/// `EVFILT_JAILDESC`.
pub const EVFILT_JAILDESC: i16 = -15;
/// `EVFILT_SYSCOUNT`.
pub const EVFILT_SYSCOUNT: i16 = 15;

/// `EV_ADD`.
pub const EV_ADD: u16 = 0x0001;
/// `EV_DELETE`.
pub const EV_DELETE: u16 = 0x0002;
/// `EV_ENABLE`.
pub const EV_ENABLE: u16 = 0x0004;
/// `EV_DISABLE`.
pub const EV_DISABLE: u16 = 0x0008;
/// `EV_ONESHOT`.
pub const EV_ONESHOT: u16 = 0x0010;
/// `EV_CLEAR`.
pub const EV_CLEAR: u16 = 0x0020;
/// `EV_RECEIPT`.
pub const EV_RECEIPT: u16 = 0x0040;
/// `EV_DISPATCH`.
pub const EV_DISPATCH: u16 = 0x0080;
/// `EV_FORCEONESHOT`.
pub const EV_FORCEONESHOT: u16 = 0x0100;
/// `EV_KEEPUDATA`.
pub const EV_KEEPUDATA: u16 = 0x0200;
/// `EV_SYSFLAGS`.
pub const EV_SYSFLAGS: u16 = 0xf000;
/// `EV_FLAG1`.
pub const EV_FLAG1: u16 = 0x2000;
/// `EV_FLAG2`.
pub const EV_FLAG2: u16 = 0x4000;
/// `EV_EOF`.
pub const EV_EOF: u16 = 0x8000;
/// `EV_ERROR`.
pub const EV_ERROR: u16 = 0x4000;

/// `NOTE_EXIT`.
pub const NOTE_EXIT: u32 = 0x8000_0000;
/// `NOTE_FORK`.
pub const NOTE_FORK: u32 = 0x4000_0000;
/// `NOTE_EXEC`.
pub const NOTE_EXEC: u32 = 0x2000_0000;
/// `NOTE_TRIGGER`.
pub const NOTE_TRIGGER: u32 = 0x0100_0000;
/// `NOTE_FFNOP`.
pub const NOTE_FFNOP: u32 = 0x0000_0000;
/// `NOTE_FFAND`.
pub const NOTE_FFAND: u32 = 0x4000_0000;
/// `NOTE_FFOR`.
pub const NOTE_FFOR: u32 = 0x8000_0000;
/// `NOTE_FFCOPY`.
pub const NOTE_FFCOPY: u32 = 0xc000_0000;
/// `NOTE_FFCTRLMASK`.
pub const NOTE_FFCTRLMASK: u32 = 0xc000_0000;
/// `NOTE_FFLAGSMASK`.
pub const NOTE_FFLAGSMASK: u32 = 0x00ff_ffff;

/// `NOTE_SECONDS`.
pub const NOTE_SECONDS: u32 = 0x1;
/// `NOTE_MSECONDS`.
pub const NOTE_MSECONDS: u32 = 0x2;
/// `NOTE_USECONDS`.
pub const NOTE_USECONDS: u32 = 0x4;
/// `NOTE_NSECONDS`.
pub const NOTE_NSECONDS: u32 = 0x8;
/// `NOTE_ABSTIME`.
pub const NOTE_ABSTIME: u32 = 0x10;

/// `NOTE_LOWAT` (EVFILT_READ/WRITE low-water mark).
pub const NOTE_LOWAT: u32 = 0x0001;
/// `NOTE_FILE_POLL` (EVFILT_READ/WRITE poll-compatible behaviour).
pub const NOTE_FILE_POLL: u32 = 0x0002;

/// `KQUEUE_CLOEXEC`.
pub const KQUEUE_CLOEXEC: i32 = 0x1;
/// `KQUEUE_CPONFORK`.
pub const KQUEUE_CPONFORK: i32 = 0x2;

/// The event queue bits.
pub const EV_READABLE: u32 = 1;
/// Writable bit.
pub const EV_WRITABLE: u32 = 2;
/// Hangup bit.
pub const EV_HUP: u32 = 4;
/// Signal/timer/user bit.
pub const EV_SIGNALED: u32 = 8;
/// Edge bit.
pub const EV_EDGE: u32 = 0x8000_0000;

/// Maximum registrations in one kqueue.
pub const MAX_REGISTRATIONS: usize = 4096;

/// The FreeBSD 12+ `struct kevent` (64 bytes on LP64).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct Kevent {
    /// Identifier (fd, timer id, pid, signal).
    pub ident: usize,
    /// Filter.
    pub filter: i16,
    /// Action/flags.
    pub flags: u16,
    /// Filter-specific flags.
    pub fflags: u32,
    /// Filter-specific data.
    pub data: i64,
    /// User data.
    pub udata: usize,
    /// Extension words.
    pub ext: [u64; 4],
}

const _: () = assert!(core::mem::size_of::<Kevent>() == 64);

/// One registration.
#[derive(Clone, Copy, Debug)]
pub struct Reg {
    /// Report flags: the creation flags without the action bits (donor `kn_kevent.flags`).
    pub flags: u16,
    /// Filter flags.
    pub fflags: u32,
    /// Data.
    pub data: i64,
    /// User data.
    pub udata: usize,
    /// Kernel source handle.
    pub handle: Handle,
    /// fd for the READ/WRITE filters; epoch checks use it.
    pub fd: Option<i32>,
    /// Registration epoch.
    pub epoch: u32,
    /// The filter readiness mask contributed to the queue registration.
    pub mask: u32,
    /// EV_DISABLE state.
    pub disabled: bool,
    /// The handle was created by this registration and must be closed on detach.
    pub owned: bool,
}

/// One readiness registration shared by every filter of one source handle.
#[derive(Clone, Copy, Debug)]
pub struct Watch {
    /// The cookie carried by this queue registration.
    pub cookie: u64,
    /// The union of the enabled filters' masks.
    pub mask: u32,
    /// Whether any enabled filter asked for `EV_CLEAR`.
    pub edge: bool,
}

/// One kqueue instance.
#[derive(Clone, Debug, Default)]
pub struct Instance {
    /// Registrations keyed by `(ident, filter)`.
    pub regs: BTreeMap<(usize, i16), Reg>,
    /// Per-handle queue registration.
    pub watches: BTreeMap<u32, Watch>,
    /// Cookie-to-handle map for delivered events.
    pub cookies: BTreeMap<u64, u32>,
}

/// The libc-side kqueue state.
#[derive(Clone, Debug, Default)]
pub struct Kqueue {
    instances: BTreeMap<i32, Instance>,
    next_token: u64,
}

impl Kqueue {
    /// An empty state.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            instances: BTreeMap::new(),
            next_token: 1,
        }
    }

    /// The registrations of one kqueue.
    #[must_use]
    pub fn registrations(&self, kqfd: i32) -> Option<&Instance> {
        self.instances.get(&kqfd)
    }

    /// Number of registrations of one kqueue.
    #[must_use]
    pub fn len(&self, kqfd: i32) -> usize {
        self.instances
            .get(&kqfd)
            .map_or(0, |instance| instance.regs.len())
    }

    /// Whether the kqueue has no registrations.
    #[must_use]
    pub fn is_empty(&self, kqfd: i32) -> bool {
        self.len(kqfd) == 0
    }

    fn next_token(&mut self) -> u64 {
        let token = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        token
    }
}

/// `kqueue()`.
pub fn kqueue<E: EvqOps, T: FdTable>(e: &mut E, t: &mut T, cloexec: bool) -> Result<i32, Errno> {
    let queue = e.create()?;
    t.install(queue, FdKind::Kqueue, cloexec)
}

/// `kqueue1(flags)`; `O_CLOEXEC` is honoured and `O_NONBLOCK` is accepted and ignored.
///
/// The flags are the FreeBSD-pin ABI constants that the installed C header
/// `bits/abi_fcntl.h` also carries.
pub fn kqueue1<E: EvqOps, T: FdTable>(e: &mut E, t: &mut T, flags: i32) -> Result<i32, Errno> {
    if flags & !(O_CLOEXEC | O_NONBLOCK) != 0 {
        return Err(Errno::EINVAL);
    }
    kqueue(e, t, flags & O_CLOEXEC != 0)
}

fn timeout_ns(timeout: Option<Timespec>) -> Result<u64, Errno> {
    let Some(value) = timeout else {
        return Ok(0);
    };
    if value.tv_sec < 0 || value.tv_nsec < 0 || value.tv_nsec >= 1_000_000_000 {
        return Err(Errno::EINVAL);
    }
    let sec = u64::try_from(value.tv_sec).map_err(|_| Errno::EINVAL)?;
    let nsec = u64::try_from(value.tv_nsec).map_err(|_| Errno::EINVAL)?;
    let total = sec.saturating_mul(1_000_000_000).saturating_add(nsec);
    Ok(if total == 0 { 1 } else { total })
}

/// `filt_timervalidate`: unit conversion and the donor's zero-period rule.
fn unit_ns(fflags: u32, data: i64) -> Result<(u64, u64), Errno> {
    let precision = fflags & (NOTE_SECONDS | NOTE_MSECONDS | NOTE_USECONDS | NOTE_NSECONDS);
    if fflags & !(precision | NOTE_ABSTIME) != 0 {
        return Err(Errno::EINVAL);
    }
    if fflags & NOTE_ABSTIME != 0 {
        // `TIMER_SET` takes a relative deadline; the pinned provider cannot express an
        // absolute one, so the declared subset reports it honestly instead of mis-arming.
        return Err(Errno::EOPNOTSUPP);
    }
    let factor = match precision {
        0 | NOTE_MSECONDS => 1_000_000u64,
        NOTE_SECONDS => 1_000_000_000,
        NOTE_USECONDS => 1_000,
        NOTE_NSECONDS => 1,
        _ => return Err(Errno::EINVAL),
    };
    if data < 0 {
        return Err(Errno::EINVAL);
    }
    // Donor: a zero period is one unit (`kn_sdata = 1`); only a zero `EV_ONESHOT` deadline
    // means "now", and the kernel timer defines a zero initial value as disarmed.
    let value = if data == 0 {
        1u64
    } else {
        u64::try_from(data).map_err(|_| Errno::EINVAL)?
    };
    Ok((value.saturating_mul(factor), factor))
}

fn known_filter(filter: i16) -> bool {
    (-EVFILT_SYSCOUNT..=-1).contains(&filter)
}

/// Validate the action flags; `EV_SYSFLAGS` are masked as `kqueue_kevent` does.
fn action_flags(raw: u16) -> Result<u16, Errno> {
    const KNOWN: u16 = EV_ADD
        | EV_DELETE
        | EV_ENABLE
        | EV_DISABLE
        | EV_ONESHOT
        | EV_CLEAR
        | EV_RECEIPT
        | EV_DISPATCH
        | EV_FORCEONESHOT
        | EV_KEEPUDATA;
    let flags = raw & !EV_SYSFLAGS;
    if flags & !KNOWN != 0 {
        return Err(Errno::EINVAL);
    }
    if flags & (EV_ENABLE | EV_DISABLE) == (EV_ENABLE | EV_DISABLE) {
        return Err(Errno::EINVAL);
    }
    if flags & EV_ADD != 0 && flags & EV_KEEPUDATA != 0 {
        return Err(Errno::EINVAL);
    }
    Ok(flags)
}

/// The report flags stored at creation: the action bits are dropped (donor `kn_kevent.flags`).
fn report_flags(flags: u16) -> u16 {
    flags & !(EV_ADD | EV_DELETE | EV_ENABLE | EV_DISABLE | EV_FORCEONESHOT)
}

/// Validate the filter-specific `fflags` for the declared subset.
fn validate_fflags(filter: i16, fflags: u32) -> Result<(), Errno> {
    match filter {
        // The queue reports readiness, not byte counts, so no watermark is expressible.
        EVFILT_READ | EVFILT_WRITE => {
            if fflags == 0 {
                Ok(())
            } else {
                Err(Errno::EOPNOTSUPP)
            }
        }
        // `filt_timervalidate` runs inside unit_ns.
        EVFILT_TIMER => Ok(()),
        // The `NOTE_FF*` stored-value machine is not implemented; only the trigger is.
        EVFILT_USER => {
            if fflags & NOTE_FFCTRLMASK == NOTE_FFNOP {
                Ok(())
            } else {
                Err(Errno::EOPNOTSUPP)
            }
        }
        EVFILT_PROC => {
            if fflags == NOTE_EXIT {
                Ok(())
            } else {
                Err(Errno::EOPNOTSUPP)
            }
        }
        _ => Ok(()),
    }
}

fn push_error(events: &mut [Kevent], written: &mut usize, change: &Kevent, error: Option<Errno>) {
    if let Some(slot) = events.get_mut(*written) {
        *slot = Kevent {
            ident: change.ident,
            filter: change.filter,
            flags: EV_ERROR,
            fflags: 0,
            data: error.map_or(0, |error| i64::from(error.0)),
            udata: change.udata,
            ext: change.ext,
        };
        *written = written.saturating_add(1);
    }
}

/// The union readiness plan of every enabled filter watching `handle`.
fn plan(instance: &Instance, handle: Handle) -> (u32, bool, usize) {
    let mut mask = 0u32;
    let mut edge = false;
    let mut count = 0usize;
    for reg in instance.regs.values() {
        if reg.handle != handle || reg.disabled {
            continue;
        }
        mask |= reg.mask;
        edge |= reg.flags & EV_CLEAR != 0;
        count = count.saturating_add(1);
    }
    (mask, edge, count)
}

/// Bring the queue registration for `handle` in line with the enabled local filters.
fn sync(
    e: &mut impl EvqOps,
    kqueue: &mut Kqueue,
    kqfd: i32,
    queue: Handle,
    handle: Handle,
) -> Result<(), Errno> {
    let (mask, edge, count) = kqueue
        .instances
        .get(&kqfd)
        .map_or((0, false, 0), |instance| plan(instance, handle));
    let bits = if edge { mask | EV_EDGE } else { mask };
    let existing = kqueue
        .instances
        .get(&kqfd)
        .and_then(|instance| instance.watches.get(&handle.0))
        .copied();
    match existing {
        Some(watch) if count == 0 => {
            match e.ctl(queue, EvqCtl::Del, handle, 0, watch.cookie) {
                Ok(()) | Err(Errno::ENOENT) => {}
                Err(error) => return Err(error),
            }
            if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
                instance.watches.remove(&handle.0);
                instance.cookies.remove(&watch.cookie);
            }
            Ok(())
        }
        Some(watch) => {
            if watch.mask == mask && watch.edge == edge {
                return Ok(());
            }
            match e.ctl(queue, EvqCtl::Mod, handle, bits, watch.cookie) {
                Ok(()) => {}
                // The kernel dropped the registration with the last close of the handle.
                Err(Errno::ENOENT) => e.ctl(queue, EvqCtl::Add, handle, bits, watch.cookie)?,
                Err(error) => return Err(error),
            }
            if let Some(instance) = kqueue.instances.get_mut(&kqfd)
                && let Some(slot) = instance.watches.get_mut(&handle.0)
            {
                slot.mask = mask;
                slot.edge = edge;
            }
            Ok(())
        }
        None if count > 0 => {
            let cookie = kqueue.next_token();
            e.ctl(queue, EvqCtl::Add, handle, bits, cookie)?;
            if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
                instance
                    .watches
                    .insert(handle.0, Watch { cookie, mask, edge });
                instance.cookies.insert(cookie, handle.0);
            }
            Ok(())
        }
        None => Ok(()),
    }
}

/// Remove one registration and its queue watch; close a source handle this registration owns.
fn detach(
    e: &mut impl EvqOps,
    s: &mut impl SrcOps,
    kqueue: &mut Kqueue,
    kqfd: i32,
    queue: Handle,
    key: (usize, i16),
) -> Result<(), Errno> {
    let Some(reg) = kqueue
        .instances
        .get_mut(&kqfd)
        .and_then(|instance| instance.regs.remove(&key))
    else {
        return Err(Errno::ENOENT);
    };
    if reg.owned {
        // The knote is gone; a failed close must not resurrect it or fail the delete.
        let _ = s.close_source(reg.handle);
    }
    sync(e, kqueue, kqfd, queue, reg.handle)
}

/// The readiness bits and the reported flag of one delivered filter.
fn delivery(filter: i16, events: u32) -> Option<(u16, i64)> {
    match filter {
        EVFILT_READ => (events & (EV_READABLE | EV_HUP) != 0).then(|| {
            let flags = if events & EV_HUP != 0 { EV_EOF } else { 0 };
            (flags, 1)
        }),
        EVFILT_WRITE => (events & (EV_WRITABLE | EV_HUP) != 0).then(|| {
            let flags = if events & EV_HUP != 0 { EV_EOF } else { 0 };
            (flags, 1)
        }),
        EVFILT_TIMER => (events & (EV_SIGNALED | EV_HUP) != 0).then_some((0, 1)),
        EVFILT_USER => (events & (EV_SIGNALED | EV_HUP) != 0).then_some((0, 0)),
        EVFILT_PROC => (events & EV_HUP != 0).then_some((EV_EOF, 0)),
        _ => None,
    }
}

/// Attach one new filter.
#[allow(clippy::too_many_arguments)]
fn attach(
    e: &mut impl EvqOps,
    t: &dyn FdTable,
    s: &mut impl SrcOps,
    kqueue: &mut Kqueue,
    kqfd: i32,
    queue: Handle,
    change: &Kevent,
    flags: u16,
) -> Result<(), Errno> {
    if kqueue.len(kqfd) >= MAX_REGISTRATIONS {
        return Err(Errno::ENOMEM);
    }
    validate_fflags(change.filter, change.fflags)?;
    let (handle, fd, epoch, mask, owned) = match change.filter {
        EVFILT_READ => {
            let fd = i32::try_from(change.ident).map_err(|_| Errno::EBADF)?;
            if !t.kind_of(fd)?.pollable() {
                return Err(Errno::EOPNOTSUPP);
            }
            (
                t.handle_of(fd)?,
                Some(fd),
                t.epoch(fd),
                EV_READABLE | EV_HUP,
                false,
            )
        }
        EVFILT_WRITE => {
            let fd = i32::try_from(change.ident).map_err(|_| Errno::EBADF)?;
            if !t.kind_of(fd)?.pollable() {
                return Err(Errno::EOPNOTSUPP);
            }
            (
                t.handle_of(fd)?,
                Some(fd),
                t.epoch(fd),
                EV_WRITABLE | EV_HUP,
                false,
            )
        }
        EVFILT_TIMER => {
            let (period, _factor) = unit_ns(change.fflags, change.data)?;
            let interval = if flags & (EV_ONESHOT | EV_FORCEONESHOT) != 0 {
                0
            } else {
                period
            };
            let handle = s.timer_create(1)?;
            if let Err(error) = s.timer_set(handle, period, interval) {
                let _ = s.close_source(handle);
                return Err(error);
            }
            (handle, None, 0, EV_SIGNALED, true)
        }
        EVFILT_USER => {
            let handle = s.event_create()?;
            if change.fflags & NOTE_TRIGGER != 0
                && let Err(error) = s.event_signal(handle, 1)
            {
                let _ = s.close_source(handle);
                return Err(error);
            }
            (handle, None, 0, EV_SIGNALED, true)
        }
        EVFILT_PROC => {
            let pid = i32::try_from(change.ident).map_err(|_| Errno::EINVAL)?;
            (s.process_handle(pid)?, None, 0, EV_HUP, true)
        }
        EVFILT_SIGNAL | EVFILT_VNODE | EVFILT_AIO | EVFILT_SENDFILE | EVFILT_PROCDESC
        | EVFILT_FS | EVFILT_LIO | EVFILT_EMPTY | EVFILT_JAIL | EVFILT_JAILDESC => {
            return Err(Errno::EOPNOTSUPP);
        }
        _ => return Err(Errno::EINVAL),
    };
    let reg = Reg {
        flags: report_flags(flags),
        fflags: change.fflags,
        data: change.data,
        udata: change.udata,
        handle,
        fd,
        epoch,
        mask,
        disabled: flags & EV_DISABLE != 0,
        owned,
    };
    if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
        instance.regs.insert((change.ident, change.filter), reg);
    }
    if let Err(error) = sync(e, kqueue, kqfd, queue, handle) {
        if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
            instance.regs.remove(&(change.ident, change.filter));
        }
        if owned {
            let _ = s.close_source(handle);
        }
        return Err(error);
    }
    Ok(())
}

/// Apply one change record (`kqueue_register`).
#[allow(clippy::too_many_arguments)]
fn apply(
    e: &mut impl EvqOps,
    t: &dyn FdTable,
    s: &mut impl SrcOps,
    kqueue: &mut Kqueue,
    kqfd: i32,
    queue: Handle,
    change: &Kevent,
) -> Result<(), Errno> {
    let flags = action_flags(change.flags)?;
    if !known_filter(change.filter) {
        return Err(Errno::EINVAL);
    }
    let key = (change.ident, change.filter);
    let existing = kqueue
        .instances
        .get(&kqfd)
        .and_then(|instance| instance.regs.get(&key))
        .copied();
    let existing = match existing {
        Some(reg) => match reg.fd {
            // The kernel drops a knote when the last reference to the file closes; a new
            // descriptor at the same number is a different object, never the old knote.
            Some(fd) if t.epoch(fd) != reg.epoch => {
                let _ = detach(e, s, kqueue, kqfd, queue, key);
                None
            }
            _ => Some(reg),
        },
        None => None,
    };
    match existing {
        Some(reg) => {
            if flags & EV_DELETE != 0 {
                return detach(e, s, kqueue, kqfd, queue, key);
            }
            let mut updated = reg;
            if flags & EV_FORCEONESHOT != 0 {
                updated.flags |= EV_ONESHOT;
            }
            if flags & EV_ENABLE != 0 {
                updated.disabled = false;
            } else if flags & EV_DISABLE != 0 {
                updated.disabled = true;
            }
            if flags & EV_KEEPUDATA == 0 {
                updated.udata = change.udata;
            }
            updated.data = change.data;
            updated.fflags = change.fflags;
            match change.filter {
                EVFILT_TIMER => {
                    validate_fflags(change.filter, change.fflags)?;
                    let (period, _factor) = unit_ns(change.fflags, change.data)?;
                    let interval = if updated.flags & (EV_ONESHOT | EV_FORCEONESHOT) != 0 {
                        0
                    } else {
                        period
                    };
                    s.timer_set(updated.handle, period, interval)?;
                }
                EVFILT_USER => {
                    validate_fflags(change.filter, change.fflags)?;
                    if change.fflags & NOTE_TRIGGER != 0 {
                        s.event_signal(updated.handle, 1)?;
                    }
                }
                _ => validate_fflags(change.filter, change.fflags)?,
            }
            if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
                instance.regs.insert(key, updated);
            }
            sync(e, kqueue, kqfd, queue, updated.handle)
        }
        None => {
            // `kqueue_register`: a matching knote is required unless EV_ADD creates one.
            if flags & EV_ADD == 0 {
                return Err(Errno::ENOENT);
            }
            attach(e, t, s, kqueue, kqfd, queue, change, flags)
        }
    }
}

/// `kevent(kq, changelist, nchanges, eventlist, nevents, timeout)`.
#[allow(clippy::too_many_arguments)]
pub fn kevent<E: EvqOps, T: FdTable, S: SrcOps>(
    e: &mut E,
    t: &T,
    s: &mut S,
    kqueue: &mut Kqueue,
    kqfd: i32,
    changes: &[Kevent],
    events: &mut [Kevent],
    timeout: Option<Timespec>,
) -> Result<usize, Errno> {
    if t.kind_of(kqfd)? != FdKind::Kqueue {
        return Err(Errno::EINVAL);
    }
    let queue = t.handle_of(kqfd)?;
    let wait = timeout_ns(timeout)?;
    kqueue.instances.entry(kqfd).or_default();
    let mut written = 0usize;
    let mut nerrors = 0usize;
    for change in changes {
        // `kqueue_kevent`: an unset filter is skipped, not an error.
        if change.filter == 0 {
            continue;
        }
        let result = apply(e, t, s, kqueue, kqfd, queue, change);
        if result.is_ok() && change.flags & EV_RECEIPT == 0 {
            continue;
        }
        // `kqueue_kevent`: an error or receipt is reported in the event list; with no room the
        // call returns that error (or 0 for a successful receipt) and applies no scan.
        if written >= events.len() {
            return match result {
                Ok(()) => Ok(0),
                Err(error) => Err(error),
            };
        }
        push_error(events, &mut written, change, result.err());
        nerrors = nerrors.saturating_add(1);
    }
    if nerrors > 0 {
        return Ok(nerrors);
    }
    if events.is_empty() {
        return Ok(0);
    }
    let mut window = [EvqEvent::default(); 64];
    let max = events.len().min(window.len());
    let Some(slice) = window.get_mut(..max) else {
        return Ok(0);
    };
    let count = e.wait(queue, slice, wait)?;
    let count = count.min(max);
    let mut index = 0usize;
    while index < count {
        let Some(event) = window.get(index) else {
            break;
        };
        index = index.saturating_add(1);
        let Some(handle_key) = kqueue
            .instances
            .get(&kqfd)
            .and_then(|instance| instance.cookies.get(&event.cookie))
            .copied()
        else {
            continue;
        };
        let keys: alloc::vec::Vec<(usize, i16)> = kqueue
            .instances
            .get(&kqfd)
            .map(|instance| {
                instance
                    .regs
                    .iter()
                    .filter(|(_, reg)| reg.handle.0 == handle_key && !reg.disabled)
                    .map(|(key, _)| *key)
                    .collect()
            })
            .unwrap_or_default();
        for key in keys {
            let Some(reg) = kqueue
                .instances
                .get(&kqfd)
                .and_then(|instance| instance.regs.get(&key))
                .copied()
            else {
                continue;
            };
            let Some((flags, data)) = delivery(key.1, event.events) else {
                continue;
            };
            if let Some(fd) = reg.fd
                && t.epoch(fd) != reg.epoch
            {
                continue;
            }
            let Some(slot) = events.get_mut(written) else {
                return Ok(written);
            };
            *slot = Kevent {
                ident: key.0,
                filter: key.1,
                flags: reg.flags | flags,
                fflags: reg.fflags,
                data,
                udata: reg.udata,
                ext: [0; 4],
            };
            written = written.saturating_add(1);
            if reg.flags & (EV_ONESHOT | EV_FORCEONESHOT) != 0 {
                let _ = detach(e, s, kqueue, kqfd, queue, key);
            } else if reg.flags & EV_DISPATCH != 0 {
                if let Some(instance) = kqueue.instances.get_mut(&kqfd)
                    && let Some(stored) = instance.regs.get_mut(&key)
                {
                    stored.disabled = true;
                }
                let _ = sync(e, kqueue, kqfd, queue, reg.handle);
            }
            if written >= events.len() {
                return Ok(written);
            }
        }
    }
    Ok(written)
}

#[cfg(test)]
#[path = "kqueue_ut.rs"]
mod tests;
