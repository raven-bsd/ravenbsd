// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `epoll` 1:1 over `EVQ_CREATE/CTL/WAIT`.
//!
//! The cookie is `(fd << 32) | table_epoch`, so a descriptor number closed and reused cannot
//! deliver a stale event: `epoll_wait` drops a record whose epoch no longer matches. `EPOLLET`
//! maps to the event queue's `EDGE`, `EPOLLONESHOT` is emulated with `MOD` to zero after
//! delivery, and `EPOLLEXCLUSIVE` is accepted and ignored (it has no meaning for one process's
//! event queue).

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_sys::sys::EvqEvent;

pub use raven_libc_abi::structs::EpollEvent;

use crate::backend::SysBackend;
use crate::fd::ofd::OfdKind;
use crate::lock::Lock;
use crate::platform::Platform;
use crate::poll::{EV_HUP, EV_READABLE, EV_WRITABLE, MAX_POLL_FDS};

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

const EVQ_ADD: u32 = 1;
const EVQ_MOD: u32 = 2;
const EVQ_DEL: u32 = 3;
const EV_EDGE: u32 = 0x8000_0000;

/// `epoll_create1`.
pub fn epoll_create1<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    flags: i32,
) -> Result<i32, Errno> {
    if flags & !(fcntl::O_CLOEXEC) != 0 {
        return Err(Errno::EINVAL);
    }
    let queue = platform.backend().evq_create()?;
    let fd = platform.install(
        queue,
        OfdKind::Epoll,
        fcntl::O_RDWR as u32,
        None,
        flags & fcntl::O_CLOEXEC != 0,
    )?;
    Ok(fd)
}

/// `epoll_ctl`.
pub fn epoll_ctl<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    epfd: i32,
    op: i32,
    fd: i32,
    event: Option<&EpollEvent>,
) -> Result<(), Errno> {
    let epoll_ref = platform.get(epfd)?;
    if epoll_ref.kind() != OfdKind::Epoll {
        return Err(Errno::EINVAL);
    }
    let queue = epoll_ref.handle();
    match op {
        EPOLL_CTL_ADD | EPOLL_CTL_MOD => {
            let target = platform.get(fd)?;
            let event = event.ok_or(Errno::EFAULT)?;
            let (events, data) = decode_event(event);
            if events & (EPOLLERR | EPOLLHUP) != 0 {
                return Err(Errno::EINVAL);
            }
            let mut evq_events = 0u32;
            if events & (EPOLLIN | EPOLLPRI | EPOLLRDHUP) != 0 {
                evq_events |= EV_READABLE;
            }
            if events & EPOLLOUT != 0 {
                evq_events |= EV_WRITABLE;
            }
            evq_events |= EV_HUP;
            if events & EPOLLET != 0 {
                evq_events |= EV_EDGE;
            }
            let epoch = platform.ofd_epoch(fd).ok_or(Errno::EBADF)?;
            let cookie = (u64::from(fd as u32) << 32) | u64::from(epoch);
            let ctl_op = if op == EPOLL_CTL_ADD {
                EVQ_ADD
            } else {
                EVQ_MOD
            };
            platform
                .backend()
                .evq_ctl(queue, ctl_op, target.handle(), evq_events, cookie)?;
            platform
                .epoll_state()
                .borrow_mut()
                .insert((epfd, cookie), (events, data));
            Ok(())
        }
        EPOLL_CTL_DEL => {
            let target = platform.get(fd)?;
            let epoch = platform.ofd_epoch(fd).ok_or(Errno::EBADF)?;
            let cookie = (u64::from(fd as u32) << 32) | u64::from(epoch);
            platform
                .backend()
                .evq_ctl(queue, EVQ_DEL, target.handle(), 0, cookie)?;
            platform.epoll_state().borrow_mut().remove(&(epfd, cookie));
            Ok(())
        }
        _ => Err(Errno::EINVAL),
    }
}

/// `epoll_wait`.
pub fn epoll_wait<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    epfd: i32,
    events: &mut [EpollEvent],
    timeout_ms: i32,
) -> Result<usize, Errno> {
    if events.is_empty() {
        return Err(Errno::EINVAL);
    }
    let epoll_ref = platform.get(epfd)?;
    if epoll_ref.kind() != OfdKind::Epoll {
        return Err(Errno::EINVAL);
    }
    let timeout_ns = if timeout_ms < 0 {
        0
    } else if timeout_ms == 0 {
        // EVQ_WAIT treats 0 as "wait forever"; a zero poll timeout must not block.
        1
    } else {
        u64::try_from(timeout_ms)
            .unwrap_or(0)
            .saturating_mul(1_000_000)
    };
    let mut raw: Vec<EvqEvent> = alloc::vec![EvqEvent::default(); MAX_POLL_FDS];
    let count = platform
        .backend()
        .evq_wait(epoll_ref.handle(), &mut raw, timeout_ns)?;
    let mut delivered = 0usize;
    for record in raw.iter().take(count) {
        let fd = (record.cookie >> 32) as u32 as i32;
        let epoch = record.cookie as u32;
        if platform.ofd_epoch(fd) != Some(epoch) {
            continue;
        }
        let registration = platform
            .epoll_state()
            .borrow()
            .get(&(epfd, record.cookie))
            .copied();
        let Some((registered, data)) = registration else {
            continue;
        };
        let mut out = 0u32;
        if record.events & EV_READABLE != 0 {
            out |= EPOLLIN;
        }
        if record.events & EV_WRITABLE != 0 {
            out |= EPOLLOUT;
        }
        if record.events & EV_HUP != 0 {
            out |= EPOLLHUP;
        }
        out |= record.events & EPOLLERR;
        if out == 0 {
            continue;
        }
        if let Some(slot) = events.get_mut(delivered) {
            *slot = encode_event(out, data);
            delivered = delivered.saturating_add(1);
        }
        if registered & EPOLLONESHOT != 0 {
            let queue = epoll_ref.handle();
            if let Ok(target) = platform.get(fd) {
                let _ = platform.backend().evq_ctl(
                    queue,
                    EVQ_MOD,
                    target.handle(),
                    EV_HUP,
                    record.cookie,
                );
            }
        }
        if delivered == events.len() {
            break;
        }
    }
    Ok(delivered)
}

/// `epoll_pwait`: the signal mask argument needs PG-26.
pub fn epoll_pwait<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _epfd: i32,
    _events: &mut [EpollEvent],
    _timeout_ms: i32,
    _sigmask: Option<&[u64]>,
) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}

/// `epoll_pwait2`: absolute timeouts and the signal mask need PG-26.
pub fn epoll_pwait2<B: SysBackend, L: Lock>(
    _platform: &Platform<B, L>,
    _epfd: i32,
    _events: &mut [EpollEvent],
    _timeout: Option<&raven_libc_abi::structs::Timespec>,
    _sigmask: Option<&[u64]>,
) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn decode_event(event: &EpollEvent) -> (u32, u64) {
    let events = u32::from_ne_bytes([
        event.bytes[0],
        event.bytes[1],
        event.bytes[2],
        event.bytes[3],
    ]);
    let mut data = [0u8; 8];
    data.copy_from_slice(&event.bytes[4..12]);
    (events, u64::from_ne_bytes(data))
}

#[cfg(target_arch = "x86_64")]
pub(crate) fn encode_event(events: u32, data: u64) -> EpollEvent {
    let mut bytes = [0u8; 12];
    bytes[..4].copy_from_slice(&events.to_ne_bytes());
    bytes[4..].copy_from_slice(&data.to_ne_bytes());
    EpollEvent { bytes }
}

#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn decode_event(event: &EpollEvent) -> (u32, u64) {
    (event.bytes[0] as u32, event.bytes[1])
}

#[cfg(not(target_arch = "x86_64"))]
pub(crate) fn encode_event(events: u32, data: u64) -> EpollEvent {
    EpollEvent {
        bytes: [u64::from(events), data],
    }
}
