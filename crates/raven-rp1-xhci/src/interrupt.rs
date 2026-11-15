// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The xHCI interrupter: pending/enable bits, ERDP and EHB.
//!
//! The interrupter register set has an Interrupt Management register (`IMAN`, with the pending
//! bit cleared by writing one and the enable bit), and the Event Ring Dequeue Pointer (`ERDP`,
//! whose bit 3 is the Event Handler Busy flag). The controller sets the pending bit and EHB when
//! an event arrives; the host consumes the event ring, writes the new dequeue pointer and clears
//! EHB in the same write. Only the model state is tested; the real register access is the
//! kernel/driver's job.

use crate::regs::{ERDP_EHB, IMAN_IE, IMAN_IP};

/// One interrupter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interrupter {
    pending: bool,
    enabled: bool,
    erdp: u64,
    ehb: bool,
}

impl Interrupter {
    /// A disabled interrupter with a zero dequeue pointer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: false,
            enabled: false,
            erdp: 0,
            ehb: false,
        }
    }

    /// True when an event is pending (the IMAN IP bit).
    #[must_use]
    pub const fn pending(&self) -> bool {
        self.pending
    }

    /// True when interrupts are enabled (the IMAN IE bit).
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    /// True when an enabled interrupter has a pending event.
    #[must_use]
    pub const fn active(&self) -> bool {
        self.pending && self.enabled
    }

    /// The IMAN register value (IP and IE bits).
    #[must_use]
    pub const fn iman(&self) -> u32 {
        (if self.pending { IMAN_IP } else { 0 }) | (if self.enabled { IMAN_IE } else { 0 })
    }

    /// Write the IMAN register. Writing one to IP clears it; IE is written directly.
    pub fn write_iman(&mut self, value: u32) {
        if value & IMAN_IP != 0 {
            self.pending = false;
        }
        self.enabled = value & IMAN_IE != 0;
    }

    /// The controller raised an event: set IP and EHB.
    pub fn raise(&mut self) {
        self.pending = true;
        self.ehb = true;
    }

    /// Acknowledge the pending bit without touching EHB.
    pub fn acknowledge(&mut self) {
        self.pending = false;
    }

    /// The ERDP register value, including the EHB bit.
    #[must_use]
    pub const fn erdp(&self) -> u64 {
        (self.erdp & !0x7) | (if self.ehb { ERDP_EHB } else { 0 })
    }

    /// True when the event handler is busy.
    #[must_use]
    pub const fn ehb(&self) -> bool {
        self.ehb
    }

    /// Write ERDP: store the dequeue pointer (low three bits reserved) and clear EHB.
    pub fn write_erdp(&mut self, value: u64) {
        self.erdp = value & !0x7;
        self.ehb = false;
    }

    /// The stored dequeue pointer without the EHB bit.
    #[must_use]
    pub const fn dequeue_pointer(&self) -> u64 {
        self.erdp
    }
}

impl Default for Interrupter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "interrupt_ut.rs"]
mod tests;
