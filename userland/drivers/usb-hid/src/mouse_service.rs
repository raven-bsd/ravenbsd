// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Mouse service: boot reports in, pointer events out.

use raven_usb_hid::{BootMouse, Error, MouseError, PointerEvent, TransferRing};

use crate::pointer::{PointerConsumer, PointerQueue};

/// A boot mouse bound to a transfer ring and a pointer event queue.
pub struct MouseService<R: TransferRing> {
    mouse: BootMouse<R>,
    pointer: PointerQueue,
    attached: bool,
    buttons: u8,
}

impl<R: TransferRing> MouseService<R> {
    /// Wrap a transport.
    pub const fn new(ring: R) -> Self {
        Self {
            mouse: BootMouse::new(ring),
            pointer: PointerQueue::new(),
            attached: false,
            buttons: 0,
        }
    }

    /// Select boot protocol and validate the report descriptor.
    ///
    /// This is also the re-enumeration entry point after a detach: the service button state is
    /// reset and the descriptor is validated again. Call [`Self::detach`] first if the stuck
    /// button release event is needed.
    ///
    /// # Errors
    ///
    /// The descriptor/transport errors from [`BootMouse::init`], including
    /// [`MouseError::NotAMouse`].
    pub fn init(&mut self, report_descriptor: &[u8]) -> Result<(), MouseError> {
        self.attached = false;
        self.buttons = 0;
        self.mouse.init(report_descriptor)?;
        self.attached = true;
        Ok(())
    }

    /// Mark the device absent and release held buttons.
    ///
    /// When a button was held, a synthetic release event is queued for the pointer consumer and
    /// returned; it has zero movement. [`Self::poll`] returns [`MouseError::Hid`] with
    /// [`Error::NotInitialized`] until a new [`Self::init`]. Detaching twice is harmless.
    pub fn detach(&mut self) -> Option<PointerEvent> {
        self.attached = false;
        let released = self.buttons;
        self.buttons = 0;
        if released == 0 {
            return None;
        }
        let event = PointerEvent {
            released,
            ..PointerEvent::NONE
        };
        let _ = self.pointer.push(event);
        Some(event)
    }

    /// True while a device is attached and initialized.
    #[must_use]
    pub const fn is_attached(&self) -> bool {
        self.attached
    }

    /// The button bitmap tracked by the service.
    ///
    /// This is the state the consumer sees; it is cleared on detach even though the core mouse
    /// model is immutable.
    #[must_use]
    pub const fn buttons(&self) -> u8 {
        self.buttons
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
        if !self.attached {
            return Err(MouseError::Hid(Error::NotInitialized));
        }
        let Some(event) = self.mouse.poll()? else {
            return Ok(0);
        };
        let pressed = event.buttons & !self.buttons;
        let released = self.buttons & !event.buttons;
        self.buttons = event.buttons;
        let event = PointerEvent {
            buttons: event.buttons,
            pressed,
            released,
            dx: event.dx,
            dy: event.dy,
            wheel: event.wheel,
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

    /// The mutable transport, for hotplug and re-enumeration.
    pub const fn ring_mut(&mut self) -> &mut R {
        self.mouse.ring_mut()
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
