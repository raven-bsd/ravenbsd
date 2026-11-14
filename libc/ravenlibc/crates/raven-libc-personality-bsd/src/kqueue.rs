// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `kqueue`/`kevent` over the Raven event queue (`B01`, `15` 4.3, `11` 3.6 decision 5).
//!
//! The supported filters are `EVFILT_READ`, `EVFILT_WRITE` (fd readiness), `EVFILT_TIMER`
//! (`TIMER_CREATE`/`TIMER_SET`), `EVFILT_USER` (`EVENT_CREATE`/`EVENT_SIGNAL`) and
//! `EVFILT_PROC` (`NOTE_EXIT` over a process handle, `PG-44`). `EVFILT_SIGNAL` needs `PG-7`,
//! `EVFILT_VNODE` needs `PG-38`; both report a per-change `EV_ERROR` with `ENOSYS`, as do AIO
//! and SENDFILE. Per-change errors are events while the output array has room; a global failure
//! (no room for the error event) returns `Err(errno)`.
//!
//! The constants and the 64-byte `struct kevent` are C38's installed `sys/event.h`; the unit
//! test parses that header and asserts every number and the field offsets.

use alloc::collections::BTreeMap;

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
/// `EVFILT_USER`.
pub const EVFILT_USER: i16 = -11;
/// `EVFILT_SENDFILE`.
pub const EVFILT_SENDFILE: i16 = -12;

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

/// `KQUEUE_CLOEXEC`.
pub const KQUEUE_CLOEXEC: i32 = 0x1;

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
    /// The action flags.
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
    /// EVQ mask at registration time.
    pub mask: u32,
    /// EV_DISABLE state.
    pub disabled: bool,
    /// EVQ cookie.
    pub token: u64,
}

/// One kqueue instance.
#[derive(Clone, Debug, Default)]
pub struct Instance {
    /// Registrations keyed by `(ident, filter)`.
    pub regs: BTreeMap<(usize, i16), Reg>,
    /// Cookie-to-key map for the EVQ.
    pub tokens: BTreeMap<u64, (usize, i16)>,
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

/// `kqueue1(flags)`; only `O_CLOEXEC` is accepted (NetBSD/OpenBSD spelling).
pub fn kqueue1<E: EvqOps, T: FdTable>(e: &mut E, t: &mut T, flags: i32) -> Result<i32, Errno> {
    if flags & !raven_libc_abi::fcntl::O_CLOEXEC != 0 {
        return Err(Errno::EINVAL);
    }
    kqueue(e, t, flags & raven_libc_abi::fcntl::O_CLOEXEC != 0)
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

fn unit_ns(fflags: u32, data: i64) -> Result<(u64, u64), Errno> {
    if fflags & NOTE_ABSTIME != 0 {
        return Err(Errno::EINVAL);
    }
    let units = fflags & (NOTE_SECONDS | NOTE_MSECONDS | NOTE_USECONDS | NOTE_NSECONDS);
    let factor = match units {
        0 | NOTE_MSECONDS => 1_000_000u64,
        NOTE_SECONDS => 1_000_000_000,
        NOTE_USECONDS => 1_000,
        NOTE_NSECONDS => 1,
        _ => return Err(Errno::EINVAL),
    };
    if data < 0 {
        return Err(Errno::EINVAL);
    }
    let value = u64::try_from(data).map_err(|_| Errno::EINVAL)?;
    Ok((value.saturating_mul(factor), factor))
}

fn known_filter(filter: i16) -> bool {
    matches!(
        filter,
        EVFILT_READ
            | EVFILT_WRITE
            | EVFILT_AIO
            | EVFILT_VNODE
            | EVFILT_PROC
            | EVFILT_SIGNAL
            | EVFILT_TIMER
            | EVFILT_USER
            | EVFILT_SENDFILE
    )
}

const KNOWN_FLAGS: u16 = EV_ADD
    | EV_DELETE
    | EV_ENABLE
    | EV_DISABLE
    | EV_ONESHOT
    | EV_CLEAR
    | EV_RECEIPT
    | EV_DISPATCH
    | EV_FORCEONESHOT
    | EV_KEEPUDATA;

fn push_event(out: &mut [Kevent], written: &mut usize, event: Kevent) -> Result<(), Errno> {
    let Some(slot) = out.get_mut(*written) else {
        return Err(Errno::ENOMEM);
    };
    *slot = event;
    *written = written.saturating_add(1);
    Ok(())
}

fn push_error(
    out: &mut [Kevent],
    written: &mut usize,
    change: &Kevent,
    error: Errno,
) -> Result<(), Errno> {
    push_event(
        out,
        written,
        Kevent {
            ident: change.ident,
            filter: change.filter,
            flags: EV_ERROR,
            fflags: 0,
            data: i64::from(error.0),
            udata: change.udata,
            ext: [0; 4],
        },
    )
}

fn receipt(out: &mut [Kevent], written: &mut usize, change: &Kevent) -> Result<(), Errno> {
    push_event(
        out,
        written,
        Kevent {
            ident: change.ident,
            filter: change.filter,
            flags: EV_ERROR,
            fflags: 0,
            data: 0,
            udata: change.udata,
            ext: [0; 4],
        },
    )
}

fn register(
    e: &mut impl EvqOps,
    t: &dyn FdTable,
    queue: Handle,
    s: &mut impl SrcOps,
    kqueue: &mut Kqueue,
    kqfd: i32,
    change: &Kevent,
) -> Result<(), Errno> {
    if kqueue.len(kqfd) >= MAX_REGISTRATIONS {
        return Err(Errno::ENOMEM);
    }
    let key = (change.ident, change.filter);
    let token = kqueue.next_token();
    let (handle, fd, epoch, mask) = match change.filter {
        EVFILT_READ => {
            let fd = i32::try_from(change.ident).map_err(|_| Errno::EINVAL)?;
            if !t.kind_of(fd)?.pollable() {
                return Err(Errno::EPERM);
            }
            let mut mask = EV_READABLE | EV_HUP;
            if change.flags & EV_CLEAR != 0 {
                mask |= EV_EDGE;
            }
            (t.handle_of(fd)?, Some(fd), t.epoch(fd), mask)
        }
        EVFILT_WRITE => {
            let fd = i32::try_from(change.ident).map_err(|_| Errno::EINVAL)?;
            if !t.kind_of(fd)?.pollable() {
                return Err(Errno::EPERM);
            }
            let mut mask = EV_WRITABLE | EV_HUP;
            if change.flags & EV_CLEAR != 0 {
                mask |= EV_EDGE;
            }
            (t.handle_of(fd)?, Some(fd), t.epoch(fd), mask)
        }
        EVFILT_TIMER => {
            let (period, _factor) = unit_ns(change.fflags, change.data)?;
            let interval = if change.flags & (EV_ONESHOT | EV_FORCEONESHOT) != 0 {
                0
            } else {
                period
            };
            let handle = s.timer_create(1)?;
            s.timer_set(handle, period, interval)?;
            (handle, None, 0, EV_SIGNALED | EV_HUP)
        }
        EVFILT_USER => {
            let handle = s.event_create()?;
            if change.fflags & NOTE_TRIGGER != 0 {
                s.event_signal(handle, 1)?;
            }
            (handle, None, 0, EV_SIGNALED | EV_HUP)
        }
        EVFILT_PROC => {
            let pid = i32::try_from(change.ident).map_err(|_| Errno::EINVAL)?;
            if change.fflags & NOTE_EXIT == 0 {
                return Err(Errno::EOPNOTSUPP);
            }
            let handle = s.process_handle(pid)?;
            (handle, None, 0, EV_HUP)
        }
        EVFILT_SIGNAL => return Err(Errno::ENOSYS),
        EVFILT_VNODE | EVFILT_AIO | EVFILT_SENDFILE => return Err(Errno::ENOSYS),
        _ => return Err(Errno::EINVAL),
    };
    let execute = match change.flags & (EV_ADD | EV_ENABLE | EV_DISABLE) {
        _ if change.flags & EV_DELETE != 0 => false,
        _ if change.flags & EV_DISABLE != 0 => true,
        _ => true,
    };
    if execute {
        e.ctl(queue, EvqCtl::Add, handle, mask, token)?;
    }
    let instance = kqueue.instances.entry(kqfd).or_default();
    let entry = Reg {
        token,
        flags: change.flags,
        fflags: change.fflags,
        data: change.data,
        udata: change.udata,
        handle,
        fd,
        epoch,
        mask,
        disabled: change.flags & EV_DISABLE != 0,
    };
    instance.regs.insert(key, entry);
    instance.tokens.insert(token, key);
    Ok(())
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
    for change in changes {
        if !known_filter(change.filter) || change.flags & !KNOWN_FLAGS != 0 {
            push_error(events, &mut written, change, Errno::EINVAL)?;
            continue;
        }
        let key = (change.ident, change.filter);
        if change.flags & EV_DELETE != 0 {
            let exists = kqueue
                .instances
                .get(&kqfd)
                .is_some_and(|instance| instance.regs.contains_key(&key));
            if !exists {
                push_error(events, &mut written, change, Errno::ENOENT)?;
                continue;
            }
            if let Some(instance) = kqueue.instances.get(&kqfd)
                && let Some(reg) = instance.regs.get(&key)
            {
                let _ = e.ctl(queue, EvqCtl::Del, reg.handle, 0, reg.token);
            }
            if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
                instance.regs.remove(&key);
                instance.tokens.retain(|_, value| *value != key);
            }
            if change.flags & EV_RECEIPT != 0 {
                receipt(events, &mut written, change)?;
            }
            continue;
        }
        let existing = kqueue
            .instances
            .get(&kqfd)
            .and_then(|instance| instance.regs.get(&key))
            .copied();
        if let Some(reg) = existing {
            if change.flags & EV_DISABLE != 0 {
                e.ctl(queue, EvqCtl::Mod, reg.handle, 0, reg.token)?;
                if let Some(instance) = kqueue.instances.get_mut(&kqfd)
                    && let Some(slot) = instance.regs.get_mut(&key)
                {
                    slot.disabled = true;
                }
            } else if change.flags & EV_ENABLE != 0 && change.flags & EV_ADD == 0 {
                e.ctl(queue, EvqCtl::Mod, reg.handle, reg.mask, reg.token)?;
                if let Some(instance) = kqueue.instances.get_mut(&kqfd)
                    && let Some(slot) = instance.regs.get_mut(&key)
                {
                    slot.disabled = false;
                }
            } else if change.flags & EV_ADD != 0 {
                // Re-adding is a modify: drop the old registration and create it again.
                if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
                    instance.regs.remove(&key);
                    instance.tokens.retain(|_, value| *value != key);
                }
                match register(e, t, queue, s, kqueue, kqfd, change) {
                    Ok(()) => {}
                    Err(error) => {
                        push_error(events, &mut written, change, error)?;
                        continue;
                    }
                }
            }
            if change.flags & EV_RECEIPT != 0 {
                receipt(events, &mut written, change)?;
            }
            continue;
        }
        match register(e, t, queue, s, kqueue, kqfd, change) {
            Ok(()) => {
                if change.flags & EV_RECEIPT != 0 {
                    receipt(events, &mut written, change)?;
                }
            }
            Err(error) => push_error(events, &mut written, change, error)?,
        }
    }
    if written >= events.len() {
        return Ok(written);
    }
    let mut window = [EvqEvent::default(); 64];
    loop {
        let count = e.wait(queue, &mut window, wait)?;
        if count == 0 {
            break;
        }
        let count = count.min(window.len());
        let mut index = 0usize;
        while index < count {
            let Some(event) = window.get(index) else {
                break;
            };
            index = index.saturating_add(1);
            let Some(instance) = kqueue.instances.get(&kqfd) else {
                continue;
            };
            let Some(key) = instance.tokens.get(&event.cookie).copied() else {
                continue;
            };
            let Some(reg) = instance.regs.get(&key).copied() else {
                continue;
            };
            if reg.disabled {
                continue;
            }
            if let Some(fd) = reg.fd
                && t.epoch(fd) != reg.epoch
            {
                continue;
            }
            let (flags, data) = match key.1 {
                EVFILT_READ => {
                    if event.events & (EV_READABLE | EV_HUP) == 0 {
                        continue;
                    }
                    let flags = if event.events & EV_HUP != 0 {
                        EV_EOF
                    } else {
                        0
                    };
                    (flags, 1)
                }
                EVFILT_WRITE => {
                    if event.events & (EV_WRITABLE | EV_HUP) == 0 {
                        continue;
                    }
                    let flags = if event.events & EV_HUP != 0 {
                        EV_EOF
                    } else {
                        0
                    };
                    (flags, 1)
                }
                EVFILT_TIMER => {
                    if event.events & (EV_SIGNALED | EV_HUP) == 0 {
                        continue;
                    }
                    (0, 1)
                }
                EVFILT_USER => {
                    if event.events & (EV_SIGNALED | EV_HUP) == 0 {
                        continue;
                    }
                    (0, 0)
                }
                EVFILT_PROC => {
                    if event.events & EV_HUP == 0 {
                        continue;
                    }
                    (EV_EOF, 0)
                }
                _ => continue,
            };
            let Some(slot) = events.get_mut(written) else {
                return Ok(written);
            };
            *slot = Kevent {
                ident: key.0,
                filter: key.1,
                flags,
                fflags: reg.fflags,
                data,
                udata: reg.udata,
                ext: [0; 4],
            };
            written = written.saturating_add(1);
            if reg.flags & (EV_ONESHOT | EV_FORCEONESHOT) != 0 {
                let _ = e.ctl(queue, EvqCtl::Del, reg.handle, 0, event.cookie);
                if let Some(instance) = kqueue.instances.get_mut(&kqfd) {
                    instance.regs.remove(&key);
                    instance.tokens.retain(|_, value| *value != key);
                }
            } else if reg.flags & EV_DISPATCH != 0 {
                let _ = e.ctl(queue, EvqCtl::Mod, reg.handle, 0, event.cookie);
                if let Some(instance) = kqueue.instances.get_mut(&kqfd)
                    && let Some(stored) = instance.regs.get_mut(&key)
                {
                    stored.disabled = true;
                }
            }
            if written >= events.len() {
                return Ok(written);
            }
        }
        if count < window.len() {
            break;
        }
    }
    Ok(written)
}

#[cfg(test)]
#[path = "kqueue_ut.rs"]
mod tests;
