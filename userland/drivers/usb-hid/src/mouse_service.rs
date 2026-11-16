// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Mouse service: boot reports in, pointer events out.

use raven_usb_hid::{BootMouse, MouseError, PointerEvent, TransferRing};

use crate::pointer::{PointerConsumer, PointerQueue};

/// A boot mouse bound to a transfer ring and a pointer event queue.
pub struct MouseService<R: TransferRing> {
    mouse: BootMouse<R>,
    pointer: PointerQueue,
}

impl<R: TransferRing> MouseService<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            mouse: BootMouse::new(ring),
            pointer: PointerQueue::new(),
        }
    }

    /// Select boot protocol and validate the report descriptor.
    ///
    /// # Errors
    ///
    /// The descriptor/transport errors from [`BootMouse::init`], including
    /// [`MouseError::NotAMouse`].
    pub fn init(&mut self, report_descriptor: &[u8]) -> Result<(), MouseError> {
        self.mouse.init(report_descriptor).map(|_| ())
    }

    /// Decode one completed report and queue its pointer event.
    ///
    /// Returns the number of events queued (zero when no report completed). The event describes
    /// button transitions and relative movement; it is queued for [`Self::read_pointer`] or
    /// [`Self::drain_pointer`].
    ///
    /// # Errors
    ///
    /// [`MouseError::Hid`] with [`raven_usb_hid::Error::NotInitialized`] before [`Self::init`],
    /// or a transport/report error.
    pub fn poll(&mut self) -> Result<usize, MouseError> {
        let Some(event) = self.mouse.poll()? else {
            return Ok(0);
        };
        Ok(usize::from(self.pointer.push(event)))
    }

    /// Drain up to `out.len()` pointer events; returns the count.
    pub fn read_pointer(&mut self, out: &mut [PointerEvent]) -> usize {
        self.pointer.read(out)
    }

    /// Hand every queued event to a pointer consumer; returns the count.
    pub fn drain_pointer<C: PointerConsumer>(&mut self, consumer: &mut C) -> usize {
        self.pointer.drain_into(consumer)
    }

    /// Events waiting in the pointer queue.
    #[must_use]
    pub fn pointer_len(&self) -> usize {
        self.pointer.len()
    }

    /// The pointer queue.
    #[must_use]
    pub const fn pointer(&self) -> &PointerQueue {
        &self.pointer
    }

    /// The transport, for the xHCI binding.
    #[must_use]
    pub const fn ring(&self) -> &R {
        self.mouse.ring()
    }

    /// The mouse model.
    #[must_use]
    pub const fn mouse(&self) -> &BootMouse<R> {
        &self.mouse
    }
}

#[cfg(test)]
#[path = "mouse_service_ut.rs"]
mod tests;
