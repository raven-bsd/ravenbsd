// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Safe S26 wrappers: event queues, user-event doorbells and the scheme-server handle.
//!
//! The kernel's `EVQ_*`, `EVENT_*` and `SCHEME_REGISTER` calls take raw user pointers. These
//! wrappers keep those pointers inside `raven-rt`; callers pass slices and handles.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Handle;

/// Event source is readable / has data.
pub const READABLE: u32 = 1;
/// Event source is writable / has space.
pub const WRITABLE: u32 = 2;
/// Event source hung up or its server died.
pub const HUP: u32 = 4;
/// Event source signalled (timer, user event, IRQ).
pub const SIGNALED: u32 = 8;
/// Edge-triggered registration bit.
pub const EDGE: u32 = 0x8000_0000;

/// One record written by [`EventQueue::wait`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvqEvent {
    /// Caller cookie from the registration.
    pub cookie: u64,
    /// Bit mask of [`READABLE`], [`WRITABLE`], [`HUP`], [`SIGNALED`].
    pub events: u32,
    /// Reserved padding; written as zero.
    pub pad: u32,
}

const _: () = assert!(core::mem::size_of::<EvqEvent>() == 16);

/// One event queue or group of event sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventQueue {
    handle: Handle,
}

impl EventQueue {
    /// Create an empty event queue.
    pub fn create() -> Result<Self, Errno> {
        // SAFETY: EVQ_CREATE takes a zero flags word.
        let value = unsafe { crate::sys::syscall1(Syscall::EVQ_CREATE as usize, 0) };
        let handle = checked_handle(value)?;
        Ok(Self { handle })
    }

    /// The underlying handle, for transferring or adding to another queue.
    pub const fn handle(self) -> Handle {
        self.handle
    }

    /// Add `source` with an event mask and caller cookie.
    pub fn add(&self, source: Handle, events: u32, cookie: u64) -> Result<(), Errno> {
        self.ctl(1, source, events, cookie)
    }

    /// Modify an existing registration.
    pub fn modify(&self, source: Handle, events: u32, cookie: u64) -> Result<(), Errno> {
        self.ctl(2, source, events, cookie)
    }

    /// Remove a registration.
    pub fn delete(&self, source: Handle) -> Result<(), Errno> {
        self.ctl(3, source, 0, 0)
    }

    /// Wait for up to `timeout_ns` nanoseconds (`0` waits forever).
    ///
    /// Returns the number of records written to `out`; a timeout returns zero.
    pub fn wait(&self, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
        // SAFETY: EVQ_WAIT writes at most `out.len()` records to the valid slice and reads no
        // user data beyond it.
        let result = unsafe {
            crate::sys::syscall4(
                Syscall::EVQ_WAIT as usize,
                self.handle.0 as usize,
                out.as_mut_ptr() as usize,
                out.len(),
                timeout_ns as usize,
            )
        };
        Errno::from_ret(result)
    }

    fn ctl(&self, op: usize, source: Handle, events: u32, cookie: u64) -> Result<(), Errno> {
        // SAFETY: EVQ_CTL takes scalar queue/source handles, an operation, an event mask and a
        // cookie; the kernel validates the handles and operation.
        let result = unsafe {
            crate::sys::syscall5(
                Syscall::EVQ_CTL as usize,
                self.handle.0 as usize,
                op,
                source.0 as usize,
                events as usize,
                cookie as usize,
            )
        };
        Errno::from_ret(result).map(|_| ())
    }
}

/// Create a user-event doorbell.
pub fn event_create() -> Result<Handle, Errno> {
    // SAFETY: EVENT_CREATE takes a zero flags word.
    checked_handle(unsafe { crate::sys::syscall1(Syscall::EVENT_CREATE as usize, 0) })
}

/// Signal a user-event doorbell and wake its waiters.
pub fn event_signal(event: Handle, value: u64) -> Result<(), Errno> {
    // SAFETY: EVENT_SIGNAL takes the event handle and a scalar value.
    let result = unsafe {
        crate::sys::syscall2(
            Syscall::EVENT_SIGNAL as usize,
            event.0 as usize,
            value as usize,
        )
    };
    Errno::from_ret(result).map(|_| ())
}

/// Register a scheme and return the owned `SchemeServer` handle.
///
/// `raven-scheme`'s `Server::register` hides this handle because it only needs the name; netd
/// needs it as an S26 event source so the server loop can block on incoming requests.
pub fn scheme_register_handle(name: &str) -> Result<Handle, Errno> {
    // SAFETY: SCHEME_REGISTER reads exactly `name.len()` bytes and takes the scalar SELF_PROC
    // handle; the kernel validates the name and rights.
    let result = unsafe {
        crate::sys::syscall3(
            Syscall::SCHEME_REGISTER as usize,
            name.as_ptr() as usize,
            name.len(),
            Handle::SELF_PROC.0 as usize,
        )
    };
    checked_handle(result)
}

fn checked_handle(result: isize) -> Result<Handle, Errno> {
    let value = Errno::from_ret(result)?;
    u32::try_from(value).map(Handle).map_err(|_| Errno::EINVAL)
}

#[cfg(test)]
#[path = "event_ut.rs"]
mod tests;
