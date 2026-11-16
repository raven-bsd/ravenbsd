// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Standard xHCI 1.1 register offsets and decoded values.
//!
//! The Pi 4 host function is an xHCI controller behind the BCM2711 PCIe bridge; the function's BAR
//! supplies the base address. The offsets below are the public xHCI 1.1 layout and are not
//! Pi-specific. The firmware description names the VL805 USB 3 function; its PCI IDs are recorded
//! for the platform probe, not used by the model.

/// VIA Technologies PCI vendor ID for the Pi 4 USB 3 function.
pub const XHCI_VENDOR_VIA: u16 = 0x1106;
/// VL805 xHCI device ID.
pub const XHCI_DEVICE_VL805: u16 = 0x3483;

/// Capability: capability length.
pub const CAPLENGTH: u64 = 0x00;
/// Capability: interface version.
pub const HCIVERSION: u64 = 0x02;
/// Capability: structural parameters 1.
pub const HCSPARAMS1: u64 = 0x04;
/// Capability: structural parameters 2.
pub const HCSPARAMS2: u64 = 0x08;
/// Capability: structural parameters 3.
pub const HCSPARAMS3: u64 = 0x0c;
/// Capability: capability parameters 1.
pub const HCCPARAMS1: u64 = 0x10;
/// Capability: doorbell offset.
pub const DBOFF: u64 = 0x14;
/// Capability: runtime register offset.
pub const RTSOFF: u64 = 0x18;
/// Capability: capability parameters 2.
pub const HCCPARAMS2: u64 = 0x1c;

/// Operational: USB command.
pub const USBCMD: u64 = 0x00;
/// Operational: USB status.
pub const USBSTS: u64 = 0x04;
/// Operational: page size.
pub const PAGESIZE: u64 = 0x08;
/// Operational: device notification control.
pub const DNCTRL: u64 = 0x14;
/// Operational: command ring control.
pub const CRCR: u64 = 0x18;
/// Operational: device context base address array pointer.
pub const DCBAAP: u64 = 0x30;
/// Operational: configure.
pub const CONFIG: u64 = 0x38;
/// Operational: first port status/control register.
pub const PORTSC_BASE: u64 = 0x400;
/// Operational: stride between port register sets.
pub const PORTSC_STRIDE: u64 = 0x10;
/// Maximum number of ports the standard layout exposes in this model.
pub const MAX_PORTS: u8 = 8;

/// Runtime: interrupter management.
pub const IMAN: u64 = 0x20;
/// Runtime: interrupter moderation.
pub const IMOD: u64 = 0x24;
/// Runtime: event ring segment table size.
pub const ERSTSZ: u64 = 0x28;
/// Runtime: event ring segment table base address.
pub const ERSTBA: u64 = 0x30;
/// Runtime: event ring dequeue pointer.
pub const ERDP: u64 = 0x38;
/// Runtime: stride between interrupters.
pub const INTERRUPTER_STRIDE: u64 = 0x20;

/// USBCMD: run/stop.
pub const USBCMD_RUN: u32 = 1 << 0;
/// USBCMD: host controller reset.
pub const USBCMD_RESET: u32 = 1 << 1;
/// USBSTS: host controller halted.
pub const USBSTS_HCH: u32 = 1 << 0;
/// USBSTS: event interrupt.
pub const USBSTS_EINT: u32 = 1 << 3;
/// PORTSC: current connect status.
pub const PORTSC_CCS: u32 = 1 << 0;
/// PORTSC: port enabled/disabled.
pub const PORTSC_PED: u32 = 1 << 1;
/// PORTSC: over-current active.
pub const PORTSC_OCA: u32 = 1 << 3;
/// PORTSC: port reset.
pub const PORTSC_PR: u32 = 1 << 4;
/// PORTSC: port link state shift.
pub const PORTSC_PLS_SHIFT: u32 = 5;
/// PORTSC: port link state mask.
pub const PORTSC_PLS_MASK: u32 = 0xf;
/// PORTSC: port power.
pub const PORTSC_PP: u32 = 1 << 9;
/// PORTSC: connect status change.
pub const PORTSC_CSC: u32 = 1 << 17;
/// PORTSC: port enabled change.
pub const PORTSC_PEC: u32 = 1 << 18;
/// PORTSC: warm port reset change.
pub const PORTSC_WRC: u32 = 1 << 19;
/// PORTSC: over-current change.
pub const PORTSC_OCC: u32 = 1 << 20;
/// PORTSC: port reset change.
pub const PORTSC_PRC: u32 = 1 << 21;
/// PORTSC: port link state change.
pub const PORTSC_PLC: u32 = 1 << 22;
/// PORTSC: port config error change.
pub const PORTSC_CEC: u32 = 1 << 23;

/// The capability registers this model needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    /// Operational register offset (capability length).
    pub cap_length: u8,
    /// xHCI interface version.
    pub version: u16,
    /// Number of device slots.
    pub max_slots: u8,
    /// Number of interrupters.
    pub max_interrupters: u16,
    /// Number of root-hub ports.
    pub max_ports: u8,
    /// Number of scratchpad buffers.
    pub scratchpad: u16,
    /// Doorbell register offset.
    pub doorbell_offset: u32,
    /// Runtime register offset.
    pub runtime_offset: u32,
}

impl Capability {
    /// Decode the first capability dwords.
    #[must_use]
    pub const fn from_regs(
        caplength_version: u32,
        hcsparams1: u32,
        hcsparams2: u32,
        dboff: u32,
        rtsoff: u32,
    ) -> Self {
        let scratchpad_hi = (hcsparams2 >> 21) & 0x1f;
        let scratchpad_lo = (hcsparams2 >> 27) & 0x1f;
        Self {
            cap_length: caplength_version as u8,
            version: (caplength_version >> 16) as u16,
            max_slots: hcsparams1 as u8,
            max_interrupters: ((hcsparams1 >> 8) & 0x7ff) as u16,
            max_ports: (hcsparams1 >> 24) as u8,
            scratchpad: ((scratchpad_hi << 5) | scratchpad_lo) as u16,
            doorbell_offset: dboff & !0x3,
            runtime_offset: rtsoff & !0x1f,
        }
    }

    /// The operational register base.
    #[must_use]
    pub const fn operational_base(&self) -> u64 {
        self.cap_length as u64
    }

    /// The doorbell register base.
    #[must_use]
    pub const fn doorbell_base(&self) -> u64 {
        self.doorbell_offset as u64
    }

    /// The runtime register base.
    #[must_use]
    pub const fn runtime_base(&self) -> u64 {
        self.runtime_offset as u64
    }

    /// Whether the port count is inside this model's bound.
    #[must_use]
    pub const fn ports_supported(&self) -> bool {
        self.max_ports > 0 && self.max_ports <= MAX_PORTS
    }

    /// Port status register offset (1-based port number).
    #[must_use]
    pub const fn portsc(&self, port: u8) -> u64 {
        PORTSC_BASE + PORTSC_STRIDE * (port as u64 - 1)
    }

    /// Interrupter register offset (0-based interrupter index).
    #[must_use]
    pub const fn interrupter(&self, index: u8) -> u64 {
        INTERRUPTER_STRIDE * index as u64
    }
}

/// A decoded `PORTSC` word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortStatus {
    /// Raw register value.
    pub bits: u32,
}

impl PortStatus {
    /// Wrap a raw value.
    #[must_use]
    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    /// Current connect status.
    #[must_use]
    pub const fn connected(self) -> bool {
        self.bits & PORTSC_CCS != 0
    }

    /// Port enabled.
    #[must_use]
    pub const fn enabled(self) -> bool {
        self.bits & PORTSC_PED != 0
    }

    /// Port power.
    #[must_use]
    pub const fn powered(self) -> bool {
        self.bits & PORTSC_PP != 0
    }

    /// Port link state.
    #[must_use]
    pub const fn link_state(self) -> u8 {
        ((self.bits >> PORTSC_PLS_SHIFT) & PORTSC_PLS_MASK) as u8
    }

    /// Any change bit is set.
    #[must_use]
    pub const fn changed(self) -> bool {
        self.bits
            & (PORTSC_CSC
                | PORTSC_PEC
                | PORTSC_WRC
                | PORTSC_OCC
                | PORTSC_PRC
                | PORTSC_PLC
                | PORTSC_CEC)
            != 0
    }

    /// Set or clear one bit.
    #[must_use]
    pub const fn with_bit(self, bit: u32, set: bool) -> Self {
        Self {
            bits: if set {
                self.bits | bit
            } else {
                self.bits & !bit
            },
        }
    }

    /// Set the link state field.
    #[must_use]
    pub const fn with_link_state(self, state: u8) -> Self {
        Self {
            bits: (self.bits & !(PORTSC_PLS_MASK << PORTSC_PLS_SHIFT))
                | (((state as u32) & PORTSC_PLS_MASK) << PORTSC_PLS_SHIFT),
        }
    }
}

#[cfg(test)]
#[path = "regs_ut.rs"]
mod tests;
