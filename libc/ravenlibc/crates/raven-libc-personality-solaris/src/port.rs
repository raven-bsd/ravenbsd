// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Event ports over the Raven event queue (`EVQ`).
//!
//! CLEANROOM: `06-solaris-design.md` A-sol-47 says a port is a descriptor that delivers records
//! of `{source, events, object, user cookie}` and that the EVQ is the Raven analogue; the seven
//! illumos sources are not copied. The values and the rules below are C67 4.3.
//!
//! A port owns one `EVQ` handle plus one doorbell (user-event) registration. Descriptor
//! associations are one-shot: the event for `(PORT_SOURCE_FD, fd)` is removed from the queue and
//! deleted from the EVQ when it is delivered, and the program associates again. A cookie is
//! `(fd << 32) | epoch`, so an event for a closed-and-reused descriptor number is dropped.

use alloc::collections::{BTreeMap, VecDeque};

use crate::consts::{
    DOORBELL_COOKIE, ETIME, PORT_MAX_ASSOCIATIONS, PORT_MAX_PORTS, PORT_MAX_QUEUED_USER_EVENTS,
    PORT_SOURCE_FD, PORT_SOURCE_USER, PortEvent, timeout_ns,
};
use crate::seams::{
    DoorbellOps, Errno, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, Handle, Timespec,
};

/// The EVQ readable bit (`raven-evq-core`).
pub const EV_READABLE: u32 = 1;
/// The EVQ writable bit (`raven-evq-core`).
pub const EV_WRITABLE: u32 = 2;
/// The EVQ hangup bit (`raven-evq-core`).
pub const EV_HUP: u32 = 4;
/// The EVQ notification bit (`raven-evq-core`).
pub const EV_SIGNALED: u32 = 8;

/// One association of a port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Assoc {
    /// The caller's `poll(2)` bits.
    pub events: i32,
    /// The EVQ registration mask derived from `events`.
    pub evq_events: u32,
    /// The caller's cookie.
    pub user: usize,
    /// The descriptor generation at association time.
    pub epoch: u32,
    /// The descriptor cookie `(fd << 32) | epoch`.
    pub cookie: u64,
    /// The kernel handle the descriptor had at association time.
    pub handle: Handle,
}

/// The state of one port.
#[derive(Clone, Debug)]
pub struct PortState {
    /// Associations keyed by `(source, object)`.
    pub assoc: BTreeMap<(i32, usize), Assoc>,
    /// Delivered records not yet returned by `port_get`/`port_getn`.
    pub queue: VecDeque<PortEvent>,
    /// The user-event registration that wakes the queue.
    pub doorbell: Handle,
    /// The event queue of the port.
    pub evq: Handle,
}

/// Every port of the process, keyed by descriptor.
#[derive(Clone, Debug, Default)]
pub struct PortTable {
    ports: BTreeMap<i32, PortState>,
}

impl PortTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ports: BTreeMap::new(),
        }
    }

    /// The number of open ports.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ports.len()
    }

    /// Whether no port is open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ports.is_empty()
    }

    /// The number of associations of `port`.
    #[must_use]
    pub fn association_count(&self, port: i32) -> Option<usize> {
        self.ports.get(&port).map(|state| state.assoc.len())
    }

    /// The number of queued records of `port`.
    #[must_use]
    pub fn queued(&self, port: i32) -> Option<usize> {
        self.ports.get(&port).map(|state| state.queue.len())
    }

    /// Forget a port (its descriptor was closed outside the personality).
    pub fn remove(&mut self, port: i32) -> Option<PortState> {
        self.ports.remove(&port)
    }

    /// The state of `port`.
    #[must_use]
    pub fn state(&self, port: i32) -> Option<&PortState> {
        self.ports.get(&port)
    }
}

/// The descriptor part of a cookie.
#[must_use]
pub fn cookie_fd(cookie: u64) -> i32 {
    cookie.wrapping_shr(32) as u32 as i32
}

/// The generation part of a cookie.
#[must_use]
pub fn cookie_epoch(cookie: u64) -> u32 {
    cookie as u32
}

/// The cookie of `(fd, epoch)`.
#[must_use]
pub fn cookie(fd: i32, epoch: u32) -> u64 {
    u64::from(fd as u32).wrapping_shl(32) | u64::from(epoch)
}

/// Map `poll(2)` bits to the EVQ registration mask; an empty result is `EINVAL`.
///
/// CLEANROOM: C67 4.3 rule 2.
///
/// # Errors
/// `EINVAL` when no bit maps to a Raven readiness bit.
pub fn poll_to_evq(events: i32) -> Result<u32, Errno> {
    use raven_libc_abi::poll::{POLLIN, POLLOUT, POLLPRI, POLLRDNORM, POLLWRNORM};
    let mut mask = 0u32;
    if events & (POLLIN | POLLPRI | POLLRDNORM) != 0 {
        mask |= EV_READABLE;
    }
    if events & (POLLOUT | POLLWRNORM) != 0 {
        mask |= EV_WRITABLE;
    }
    if events & (raven_libc_abi::poll::POLLHUP | raven_libc_abi::poll::POLLERR) != 0 {
        mask |= EV_HUP;
    }
    if mask == 0 {
        return Err(Errno::EINVAL);
    }
    Ok(mask)
}

/// Map delivered EVQ bits to `poll(2)` bits.
#[must_use]
pub fn evq_to_poll(events: u32) -> i32 {
    use raven_libc_abi::poll::{POLLHUP, POLLIN, POLLOUT};
    let mut out = 0i32;
    if events & EV_READABLE != 0 {
        out |= POLLIN;
    }
    if events & EV_WRITABLE != 0 {
        out |= POLLOUT;
    }
    if events & EV_HUP != 0 {
        out |= POLLHUP;
    }
    out
}

/// Validate that `port` is an open event-port descriptor.
fn check_port<T: FdTable>(table: &T, port: i32) -> Result<(), Errno> {
    if table.kind_of(port)? == FdKind::Port {
        Ok(())
    } else {
        Err(Errno::EBADF)
    }
}

/// Translate the "queue is full" EVQ error to the documented `ENOMEM`.
fn evq_error(error: Errno) -> Errno {
    if error == Errno::ENOSPC {
        Errno::ENOMEM
    } else {
        error
    }
}

/// The number of queued user events of one port.
fn queued_user_events(state: &PortState) -> usize {
    state
        .queue
        .iter()
        .filter(|event| event.portev_source == PORT_SOURCE_USER as u16)
        .count()
}

/// `port_create()`: one EVQ handle, one doorbell and one descriptor.
///
/// # Errors
/// `EMFILE` when the process already has 256 ports or the fd table is full; the EVQ,
/// doorbell and fd-table errors are passed through.
pub fn create<E: EvqOps, D: DoorbellOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    doorbell: &mut D,
    fds: &mut T,
) -> Result<i32, Errno> {
    if table.ports.len() >= PORT_MAX_PORTS {
        return Err(Errno::EMFILE);
    }
    let queue = evq.create()?;
    let bell = doorbell.create()?;
    evq.ctl(queue, EvqCtl::Add, bell, EV_SIGNALED, DOORBELL_COOKIE)?;
    let fd = fds.install(queue, FdKind::Port, false)?;
    table.ports.insert(
        fd,
        PortState {
            assoc: BTreeMap::new(),
            queue: VecDeque::new(),
            doorbell: bell,
            evq: queue,
        },
    );
    Ok(fd)
}

/// `port_associate(port, PORT_SOURCE_FD, fd, events, user)`.
///
/// Only `PORT_SOURCE_FD` is implemented; an already-associated descriptor replaces its events
/// and cookie with one EVQ `MOD`.
///
/// # Errors
/// `EBADF` for a bad port or descriptor, `EINVAL` for another source or an empty mapped mask,
/// `ENOMEM` above 4096 associations (C67 4.3 rules 2, 7 and 8).
#[allow(clippy::too_many_arguments)] // The C signature has five arguments plus the seams.
pub fn associate<E: EvqOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    fds: &T,
    port: i32,
    source: i32,
    object: usize,
    events: i32,
    user: usize,
) -> Result<(), Errno> {
    check_port(fds, port)?;
    if source != PORT_SOURCE_FD {
        return Err(Errno::EINVAL);
    }
    let evq_events = poll_to_evq(events)?;
    let fd = i32::try_from(object).map_err(|_| Errno::EBADF)?;
    let handle = fds.handle_of(fd)?;
    let epoch = fds.epoch(fd);
    let descriptor_cookie = cookie(fd, epoch);
    let state = table.ports.get_mut(&port).ok_or(Errno::EBADF)?;
    let key = (source, object);
    let replaced = state.assoc.contains_key(&key);
    if !replaced && state.assoc.len() >= PORT_MAX_ASSOCIATIONS {
        return Err(Errno::ENOMEM);
    }
    let op = if replaced { EvqCtl::Mod } else { EvqCtl::Add };
    evq.ctl(state.evq, op, handle, evq_events, descriptor_cookie)
        .map_err(evq_error)?;
    state.assoc.insert(
        key,
        Assoc {
            events,
            evq_events,
            user,
            epoch,
            cookie: descriptor_cookie,
            handle,
        },
    );
    Ok(())
}

/// `port_dissociate(port, source, object)`.
///
/// # Errors
/// `EBADF` for a bad port, `EINVAL` for another source, `ENOENT` when not associated (also
/// after the one-shot delivery removed it).
pub fn dissociate<E: EvqOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    fds: &T,
    port: i32,
    source: i32,
    object: usize,
) -> Result<(), Errno> {
    check_port(fds, port)?;
    if source != PORT_SOURCE_FD {
        return Err(Errno::EINVAL);
    }
    let state = table.ports.get_mut(&port).ok_or(Errno::EBADF)?;
    let key = (source, object);
    let Some(assoc) = state.assoc.remove(&key) else {
        return Err(Errno::ENOENT);
    };
    // The registration may already be gone (a stale descriptor); the association is removed
    // either way, which is the state the caller asked for.
    let _ = evq.ctl(state.evq, EvqCtl::Del, assoc.handle, 0, assoc.cookie);
    Ok(())
}

/// Turn EVQ records into port records, enforcing the epoch and the one-shot rule.
fn collect<E: EvqOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    fds: &T,
    port: i32,
    events: &[EvqEvent],
) -> Result<(), Errno> {
    for event in events {
        if event.cookie == DOORBELL_COOKIE {
            continue;
        }
        let fd = cookie_fd(event.cookie);
        let epoch = cookie_epoch(event.cookie);
        if fds.epoch(fd) != epoch {
            continue;
        }
        let Ok(object) = usize::try_from(fd) else {
            continue;
        };
        let key = (PORT_SOURCE_FD, object);
        let Some(assoc) = table
            .ports
            .get(&port)
            .and_then(|state| state.assoc.get(&key))
            .copied()
        else {
            continue;
        };
        if assoc.epoch != epoch {
            continue;
        }
        let reported = evq_to_poll(event.events);
        if reported == 0 {
            continue;
        }
        let Some(state) = table.ports.get_mut(&port) else {
            return Err(Errno::EBADF);
        };
        state.assoc.remove(&key);
        let queue = state.evq;
        state.queue.push_back(PortEvent {
            portev_events: reported,
            portev_source: PORT_SOURCE_FD as u16,
            portev_pad: 0,
            portev_object: object,
            portev_user: assoc.user,
        });
        let _ = evq.ctl(queue, EvqCtl::Del, assoc.handle, 0, assoc.cookie);
    }
    Ok(())
}

/// Wait once and collect everything the EVQ reports.
fn wait_and_collect<E: EvqOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    fds: &T,
    port: i32,
    window: &mut [EvqEvent],
    wait_ns: u64,
) -> Result<usize, Errno> {
    let queue = table.ports.get(&port).ok_or(Errno::EBADF)?.evq;
    let count = evq.wait(queue, window, wait_ns)?;
    let count = count.min(window.len());
    let Some(slice) = window.get(..count) else {
        return Ok(0);
    };
    collect(table, evq, fds, port, slice)?;
    Ok(count)
}

/// Move queued records into `out`, returning the number written.
fn drain(table: &mut PortTable, port: i32, out: &mut [PortEvent]) -> Result<usize, Errno> {
    let state = table.ports.get_mut(&port).ok_or(Errno::EBADF)?;
    let mut written = 0usize;
    while written < out.len() {
        let Some(event) = state.queue.pop_front() else {
            break;
        };
        let Some(slot) = out.get_mut(written) else {
            state.queue.push_front(event);
            break;
        };
        *slot = event;
        written = written.saturating_add(1);
    }
    Ok(written)
}

/// `port_get(port, pe, timeout)`.
///
/// A `None` timeout blocks, a zero `timespec` polls, a positive one waits that long. Queued
/// `port_send` records are returned before EVQ records (FIFO).
///
/// # Errors
/// `EBADF`, `EINVAL` (timeout), `ETIME` when nothing was delivered, or an EVQ error.
pub fn get<E: EvqOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    fds: &T,
    port: i32,
    timeout: Option<&Timespec>,
) -> Result<PortEvent, Errno> {
    check_port(fds, port)?;
    let wait_ns = timeout_ns(timeout)?;
    let mut window = [EvqEvent::default(); 8];
    loop {
        {
            let state = table.ports.get_mut(&port).ok_or(Errno::EBADF)?;
            if let Some(event) = state.queue.pop_front() {
                return Ok(event);
            }
        }
        let count = wait_and_collect(table, evq, fds, port, &mut window, wait_ns)?;
        if count == 0 {
            return Err(ETIME);
        }
    }
}

/// `port_getn(port, list, max, nget, timeout)`.
///
/// On input `*nget` is the minimum number of records to wait for (0 returns what is ready); on
/// output it is the number retrieved. A timeout after some records returns `Ok` with the count.
///
/// # Errors
/// `EBADF`, `EINVAL` when `*nget > max`, `ETIME` when nothing was retrieved, or an EVQ error.
pub fn getn<E: EvqOps, T: FdTable>(
    table: &mut PortTable,
    evq: &mut E,
    fds: &T,
    port: i32,
    out: &mut [PortEvent],
    nget: &mut u32,
    timeout: Option<&Timespec>,
) -> Result<(), Errno> {
    check_port(fds, port)?;
    let maximum = out.len();
    let minimum = usize::try_from(*nget).map_err(|_| Errno::EINVAL)?;
    if minimum > maximum {
        return Err(Errno::EINVAL);
    }
    let wait_ns = timeout_ns(timeout)?;
    let mut window = [EvqEvent::default(); 16];
    let mut written = drain(table, port, out)?;
    loop {
        if written >= maximum {
            break;
        }
        let block = written < minimum;
        let this_wait = if block { wait_ns } else { 1 };
        let count = wait_and_collect(table, evq, fds, port, &mut window, this_wait)?;
        if count == 0 {
            if written == 0 && minimum > 0 {
                return Err(ETIME);
            }
            break;
        }
        let added = drain(table, port, out.get_mut(written..).unwrap_or(&mut []))?;
        written = written.saturating_add(added);
    }
    *nget = u32::try_from(written).unwrap_or(u32::MAX);
    Ok(())
}

/// `port_send(port, events, user)`: queue a `PORT_SOURCE_USER` record and ring the doorbell.
///
/// # Errors
/// `EBADF`, `EAGAIN` when 1024 user records are already queued, or the doorbell error.
pub fn send<D: DoorbellOps, T: FdTable>(
    table: &mut PortTable,
    doorbell: &mut D,
    fds: &T,
    port: i32,
    events: i32,
    user: usize,
) -> Result<(), Errno> {
    check_port(fds, port)?;
    let state = table.ports.get_mut(&port).ok_or(Errno::EBADF)?;
    if queued_user_events(state) >= PORT_MAX_QUEUED_USER_EVENTS {
        return Err(Errno::EAGAIN);
    }
    let bell = state.doorbell;
    state.queue.push_back(PortEvent {
        portev_events: events,
        portev_source: PORT_SOURCE_USER as u16,
        portev_pad: 0,
        portev_object: 0,
        portev_user: user,
    });
    if let Err(error) = doorbell.signal(bell, 1) {
        // Do not leave a record the caller was told did not exist.
        state.queue.pop_back();
        return Err(error);
    }
    Ok(())
}

/// `port_alert(port, flags, events, user)`: no semantics are specified by doc 06 (C67 12.3).
///
/// The port descriptor is validated first; a valid port always answers `ENOSYS`, so a
/// configure probe that links the symbol succeeds (C67 5.A).
///
/// # Errors
/// `EBADF` for a bad port, otherwise `ENOSYS`.
pub fn alert<T: FdTable>(fds: &T, port: i32) -> Result<(), Errno> {
    check_port(fds, port)?;
    Err(Errno::ENOSYS)
}

#[cfg(test)]
#[path = "port_ut.rs"]
mod tests;
