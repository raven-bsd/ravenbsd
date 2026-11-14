// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `epoll` over the Raven event queue (`11` 3.6 decision 4, `15` 4.3).
//!
//! The event record is the Linux one: 12 bytes on x86_64 (packed) and 16 bytes elsewhere
//! (`A-abi-13`). The registration cookie is `(fd << 32) | fd_epoch`, so a descriptor number
//! that was closed and reused cannot deliver a stale event: `epoll_wait` drops a record whose
//! epoch no longer matches the table. `EPOLLET` becomes the queue's `EDGE` bit,
//! `EPOLLONESHOT` is emulated with a `MOD` to zero after the first delivery, and
//! `EPOLLEXCLUSIVE` is accepted and ignored.

use alloc::collections::BTreeMap;

use crate::seams::{Errno, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, SigSet};

/// `EPOLLIN`.
pub const EPOLLIN: u32 = 0x001;
/// `EPOLLPRI`.
pub const EPOLLPRI: u32 = 0x002;
/// `EPOLLOUT`.
pub const EPOLLOUT: u32 = 0x004;
/// `EPOLLERR`.
pub const EPOLLERR: u32 = 0x008;
/// `EPOLLHUP`.
pub const EPOLLHUP: u32 = 0x010;
/// `EPOLLNVAL`.
pub const EPOLLNVAL: u32 = 0x020;
/// `EPOLLRDNORM`.
pub const EPOLLRDNORM: u32 = 0x040;
/// `EPOLLRDBAND`.
pub const EPOLLRDBAND: u32 = 0x080;
/// `EPOLLWRNORM`.
pub const EPOLLWRNORM: u32 = 0x100;
/// `EPOLLWRBAND`.
pub const EPOLLWRBAND: u32 = 0x200;
/// `EPOLLMSG`.
pub const EPOLLMSG: u32 = 0x400;
/// `EPOLLRDHUP`.
pub const EPOLLRDHUP: u32 = 0x2000;
/// `EPOLLEXCLUSIVE`.
pub const EPOLLEXCLUSIVE: u32 = 1 << 28;
/// `EPOLLWAKEUP`.
pub const EPOLLWAKEUP: u32 = 1 << 29;
/// `EPOLLONESHOT`.
pub const EPOLLONESHOT: u32 = 1 << 30;
/// `EPOLLET`.
pub const EPOLLET: u32 = 1 << 31;

/// `EPOLL_CTL_ADD`.
pub const EPOLL_CTL_ADD: i32 = 1;
/// `EPOLL_CTL_DEL`.
pub const EPOLL_CTL_DEL: i32 = 2;
/// `EPOLL_CTL_MOD`.
pub const EPOLL_CTL_MOD: i32 = 3;

/// The Linux `struct epoll_event`.
///
/// `repr(packed)` only on x86_64 (`A-abi-13`); the derives stay valid because the fields are
/// integers and the type is never borrowed by reference.
#[repr(C)]
#[cfg_attr(target_arch = "x86_64", repr(packed))]
#[derive(Clone, Copy)]
pub struct EpollEvent {
    /// Requested or delivered event bits.
    pub events: u32,
    /// Caller data.
    pub data: u64,
}

/// `EVQ` readable bit.
pub const EV_READABLE: u32 = 1;
/// `EVQ` writable bit.
pub const EV_WRITABLE: u32 = 2;
/// `EVQ` hangup bit.
pub const EV_HUP: u32 = 4;
/// `EVQ` signal/timer/user-event bit.
pub const EV_SIGNALED: u32 = 8;
/// `EVQ` edge-triggered registration.
pub const EV_EDGE: u32 = 0x8000_0000;

/// Maximum registrations in one queue (`raven-evq-core` `MAX_REGISTRATIONS`).
pub const MAX_REGISTRATIONS: usize = 4096;

/// `EPOLL_CLOEXEC` (the Linux value equals `O_CLOEXEC`).
pub const EPOLL_CLOEXEC: i32 = raven_libc_abi::fcntl::O_CLOEXEC;

/// One registration in the libc-side table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Reg {
    /// The requested `EPOLL*` bits.
    pub events: u32,
    /// The caller's cookie.
    pub data: u64,
    /// The fd epoch at registration time.
    pub epoch: u32,
    /// `EPOLLONESHOT` fired; a `MOD` re-arms it.
    pub disabled: bool,
}

/// The registrations of every epoll instance, keyed by `epfd` and then by fd.
#[derive(Clone, Debug, Default)]
pub struct EpollTable {
    instances: BTreeMap<i32, BTreeMap<i32, Reg>>,
}

impl EpollTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            instances: BTreeMap::new(),
        }
    }

    /// The registrations of one instance.
    #[must_use]
    pub fn registrations(&self, epfd: i32) -> Option<&BTreeMap<i32, Reg>> {
        self.instances.get(&epfd)
    }

    /// Number of registrations of one instance.
    #[must_use]
    pub fn len(&self, epfd: i32) -> usize {
        self.instances.get(&epfd).map_or(0, BTreeMap::len)
    }

    /// Whether the instance has no registrations.
    #[must_use]
    pub fn is_empty(&self, epfd: i32) -> bool {
        self.len(epfd) == 0
    }

    /// Forget an instance (its epfd was closed).
    pub fn remove_instance(&mut self, epfd: i32) {
        self.instances.remove(&epfd);
    }
}

/// `translate_in`: request bits to `EVQ` registration bits (`5.G`).
#[must_use]
pub const fn translate_in(epoll_events: u32) -> u32 {
    let mut mask = 0u32;
    if epoll_events & (EPOLLIN | EPOLLPRI | EPOLLRDNORM | EPOLLRDBAND) != 0 {
        mask |= EV_READABLE;
    }
    if epoll_events & (EPOLLOUT | EPOLLWRNORM | EPOLLWRBAND) != 0 {
        mask |= EV_WRITABLE;
    }
    // HUP is always implied by Linux, whether or not the caller asked for it.
    mask |= EV_HUP;
    if epoll_events & EPOLLET != 0 {
        mask |= EV_EDGE;
    }
    mask
}

/// `translate_out`: delivered `EVQ` bits to reported `EPOLL*` bits (`5.G`).
///
/// `EPOLLERR` is never invented: `EVQ` has no error bit (documented gap).
#[must_use]
pub const fn translate_out(evq_events: u32, registered: u32) -> u32 {
    let mut out = 0u32;
    if evq_events & EV_READABLE != 0 {
        if registered & (EPOLLIN | EPOLLPRI | EPOLLRDNORM | EPOLLRDBAND) != 0 {
            out |= EPOLLIN;
        }
        if registered & EPOLLRDNORM != 0 {
            out |= EPOLLRDNORM;
        }
    }
    if evq_events & EV_WRITABLE != 0 {
        if registered & (EPOLLOUT | EPOLLWRNORM | EPOLLWRBAND) != 0 {
            out |= EPOLLOUT;
        }
        if registered & EPOLLWRNORM != 0 {
            out |= EPOLLWRNORM;
        }
    }
    if evq_events & EV_HUP != 0 {
        out |= EPOLLHUP;
        if registered & EPOLLRDHUP != 0 {
            out |= EPOLLRDHUP;
        }
    }
    out
}

/// The cookie of one registration.
#[must_use]
pub fn cookie(fd: i32, epoch: u32) -> u64 {
    let base = u64::from(fd as u32) << 32;
    base | u64::from(epoch)
}

/// The fd encoded in a cookie.
#[must_use]
pub fn cookie_fd(cookie: u64) -> i32 {
    (cookie >> 32) as u32 as i32
}

/// The epoch encoded in a cookie.
#[must_use]
pub fn cookie_epoch(cookie: u64) -> u32 {
    cookie as u32
}

/// `epoll_create1`.
pub fn create1<E: EvqOps, T: FdTable>(e: &mut E, t: &mut T, flags: i32) -> Result<i32, Errno> {
    if flags & !EPOLL_CLOEXEC != 0 {
        return Err(Errno::EINVAL);
    }
    let queue = e.create()?;
    t.install(queue, FdKind::Epoll, flags & EPOLL_CLOEXEC != 0)
}

/// `epoll_create`; `size <= 0` is `EINVAL` (the size is otherwise ignored, as on Linux).
pub fn create<E: EvqOps, T: FdTable>(e: &mut E, t: &mut T, size: i32) -> Result<i32, Errno> {
    if size <= 0 {
        return Err(Errno::EINVAL);
    }
    create1(e, t, 0)
}

/// `epoll_ctl`.
pub fn ctl<E: EvqOps, T: FdTable>(
    e: &mut E,
    t: &mut T,
    ep: &mut EpollTable,
    epfd: i32,
    op: i32,
    fd: i32,
    ev: Option<&EpollEvent>,
) -> Result<(), Errno> {
    let epoll_kind = t.kind_of(epfd)?;
    if epoll_kind != FdKind::Epoll {
        return Err(Errno::EINVAL);
    }
    let queue = t.handle_of(epfd)?;
    let target_kind = t.kind_of(fd)?;
    if fd == epfd {
        return Err(Errno::EINVAL);
    }
    match op {
        EPOLL_CTL_ADD => {
            if ep
                .instances
                .get(&epfd)
                .is_some_and(|regs| regs.contains_key(&fd))
            {
                return Err(Errno::EEXIST);
            }
            if !target_kind.pollable() {
                return Err(Errno::EPERM);
            }
            if ep.len(epfd) >= MAX_REGISTRATIONS {
                return Err(Errno::ENOSPC);
            }
            let event = ev.ok_or(Errno::EFAULT)?;
            let epoch = t.epoch(fd);
            let handle = t.handle_of(fd)?;
            let mask = translate_in(event.events);
            e.ctl(queue, EvqCtl::Add, handle, mask, cookie(fd, epoch))?;
            ep.instances.entry(epfd).or_default().insert(
                fd,
                Reg {
                    events: event.events,
                    data: event.data,
                    epoch,
                    disabled: false,
                },
            );
            Ok(())
        }
        EPOLL_CTL_MOD => {
            let Some(reg) = ep
                .instances
                .get(&epfd)
                .and_then(|regs| regs.get(&fd))
                .copied()
            else {
                return Err(Errno::ENOENT);
            };
            let event = ev.ok_or(Errno::EFAULT)?;
            if event.events & EPOLLEXCLUSIVE != 0 {
                return Err(Errno::EINVAL);
            }
            let handle = t.handle_of(fd)?;
            let mask = translate_in(event.events);
            e.ctl(queue, EvqCtl::Mod, handle, mask, cookie(fd, reg.epoch))?;
            if let Some(regs) = ep.instances.get_mut(&epfd)
                && let Some(slot) = regs.get_mut(&fd)
            {
                slot.events = event.events;
                slot.data = event.data;
                slot.disabled = false;
            }
            Ok(())
        }
        EPOLL_CTL_DEL => {
            let Some(reg) = ep
                .instances
                .get(&epfd)
                .and_then(|regs| regs.get(&fd))
                .copied()
            else {
                return Err(Errno::ENOENT);
            };
            let handle = t.handle_of(fd)?;
            e.ctl(queue, EvqCtl::Del, handle, 0, cookie(fd, reg.epoch))?;
            if let Some(regs) = ep.instances.get_mut(&epfd) {
                regs.remove(&fd);
            }
            Ok(())
        }
        _ => Err(Errno::EINVAL),
    }
}

/// The `EVQ` timeout in nanoseconds for a millisecond `epoll_wait` timeout.
///
/// `-1` means "forever" (the queue's 0), `0` means poll (the smallest representable timeout,
/// 1 ns), positive values are multiplied and saturated.
#[must_use]
pub fn timeout_ns(timeout_ms: i32) -> u64 {
    if timeout_ms < 0 {
        return 0;
    }
    if timeout_ms == 0 {
        return 1;
    }
    let ns = u64::from(timeout_ms as u32).saturating_mul(1_000_000);
    if ns == 0 { 1 } else { ns }
}

/// `epoll_wait`.
pub fn wait<E: EvqOps, T: FdTable>(
    e: &mut E,
    t: &T,
    ep: &mut EpollTable,
    epfd: i32,
    out: &mut [EpollEvent],
    timeout_ms: i32,
    sigmask: Option<&SigSet>,
) -> Result<usize, Errno> {
    if out.is_empty() {
        return Err(Errno::EINVAL);
    }
    if sigmask.is_some() {
        // `epoll_pwait` without `PG-8` (atomic mask-and-wait) is `ENOSYS`.
        return Err(Errno::ENOSYS);
    }
    let epoll_kind = t.kind_of(epfd)?;
    if epoll_kind != FdKind::Epoll {
        return Err(Errno::EINVAL);
    }
    let queue = t.handle_of(epfd)?;
    let wait_ns = timeout_ns(timeout_ms);
    let mut window = [EvqEvent::default(); 64];
    let mut written = 0usize;
    loop {
        let count = e.wait(queue, &mut window, wait_ns)?;
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
            let fd = cookie_fd(event.cookie);
            let epoch = cookie_epoch(event.cookie);
            let Some(reg) = ep
                .instances
                .get(&epfd)
                .and_then(|regs| regs.get(&fd))
                .copied()
            else {
                continue;
            };
            if t.epoch(fd) != epoch {
                continue;
            }
            if reg.disabled {
                continue;
            }
            let events = translate_out(event.events, reg.events);
            if events == 0 {
                continue;
            }
            let Some(slot) = out.get_mut(written) else {
                break;
            };
            *slot = EpollEvent {
                events,
                data: reg.data,
            };
            written = written.saturating_add(1);
            if reg.events & EPOLLONESHOT != 0 {
                if let Some(regs) = ep.instances.get_mut(&epfd)
                    && let Some(stored) = regs.get_mut(&fd)
                {
                    stored.disabled = true;
                }
                if let Ok(handle) = t.handle_of(fd) {
                    // The event was already delivered; a failing disarm leaves the
                    // registration armed, which is the safe direction.
                    let _ = e.ctl(queue, EvqCtl::Mod, handle, 0, event.cookie);
                }
            }
            if written == out.len() {
                return Ok(written);
            }
        }
        if count < window.len() {
            break;
        }
    }
    Ok(written)
}

/// `epoll_pwait`: `sigmask == NULL` behaves as [`wait`]; a non-NULL mask is `ENOSYS` until
/// `PG-8`.
pub fn pwait<E: EvqOps, T: FdTable>(
    e: &mut E,
    t: &T,
    ep: &mut EpollTable,
    epfd: i32,
    out: &mut [EpollEvent],
    timeout_ms: i32,
    sigmask: Option<&SigSet>,
) -> Result<usize, Errno> {
    wait(e, t, ep, epfd, out, timeout_ms, sigmask)
}

/// `epoll_pwait2`: a nanosecond timeout converted to the millisecond form with a ceiling.
pub fn pwait2<E: EvqOps, T: FdTable>(
    e: &mut E,
    t: &T,
    ep: &mut EpollTable,
    epfd: i32,
    out: &mut [EpollEvent],
    timeout_ns: Option<i64>,
    sigmask: Option<&SigSet>,
) -> Result<usize, Errno> {
    let millis = match timeout_ns {
        None => -1,
        Some(ns) if ns < 0 => return Err(Errno::EINVAL),
        Some(ns) => {
            let ms = ns.saturating_add(999_999) / 1_000_000;
            i32::try_from(ms).unwrap_or(i32::MAX)
        }
    };
    wait(e, t, ep, epfd, out, millis, sigmask)
}

#[cfg(test)]
#[path = "epoll_ut.rs"]
mod tests;
