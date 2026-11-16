// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Standard xHCI 1.1 register map and field decoding.
//!
//! The offsets and field positions come from the public xHCI 1.1 specification. The RP1 USB node
//! is a Synopsys DesignWare USB3 core; where its xHCI register block starts inside the DWC3
//! window is a DesignWare/rp1 integration detail that is **UNVERIFIED** here, so the model takes
//! the controller base as data and never assumes an offset.

/// Capability register: capability length (byte).
pub const CAPLENGTH: u16 = 0x00;
/// Capability register: interface version (16-bit BCD).
pub const HCIVERSION: u16 = 0x02;
/// Capability register: structural parameters 1.
pub const HCSPARAMS1: u16 = 0x04;
/// Capability register: structural parameters 2.
pub const HCSPARAMS2: u16 = 0x08;
/// Capability register: structural parameters 3.
pub const HCSPARAMS3: u16 = 0x0C;
/// Capability register: capability parameters 1.
pub const HCCPARAMS1: u16 = 0x10;
/// Capability register: doorbell array offset.
pub const DBOFF: u16 = 0x14;
/// Capability register: runtime register space offset.
pub const RTSOFF: u16 = 0x18;
/// Capability register: capability parameters 2.
pub const HCCPARAMS2: u16 = 0x1C;

/// Operational register: USB command.
pub const USBCMD: u16 = 0x00;
/// Operational register: USB status.
pub const USBSTS: u16 = 0x04;
/// Operational register: page size.
pub const PAGESIZE: u16 = 0x08;
/// Operational register: device notification control.
pub const DNCTRL: u16 = 0x14;
/// Operational register: command ring control (64-bit).
pub const CRCR: u16 = 0x18;
/// Operational register: device context base address array pointer (64-bit).
pub const DCBAAP: u16 = 0x30;
/// Operational register: configure.
pub const CONFIG: u16 = 0x38;

/// First port register set, relative to the operational base.
pub const PORT_BASE: u16 = 0x400;
/// Distance between port register sets.
pub const PORT_STRIDE: u16 = 0x10;
/// Port register: port status and control.
pub const PORTSC: u16 = 0x00;
/// Port register: port power management status and control.
pub const PORTPMSC: u16 = 0x04;
/// Port register: port link info.
pub const PORTLI: u16 = 0x08;

/// Runtime register: microframe index.
pub const MFINDEX: u16 = 0x00;
/// Distance between interrupter register sets.
pub const INTERRUPTER_STRIDE: u16 = 0x20;
/// Interrupter register: interrupt management.
pub const IMAN: u16 = 0x00;
/// Interrupter register: interrupt moderation.
pub const IMOD: u16 = 0x04;
/// Interrupter register: event ring segment table size.
pub const ERSTSZ: u16 = 0x08;
/// Interrupter register: event ring segment table base address (64-bit).
pub const ERSTBA: u16 = 0x10;
/// Interrupter register: event ring dequeue pointer (64-bit, EHB in bit 3).
pub const ERDP: u16 = 0x18;

/// USBCMD bit: run.
pub const USBCMD_RUN: u32 = 1 << 0;
/// USBCMD bit: host controller reset.
pub const USBCMD_RESET: u32 = 1 << 1;
/// USBCMD bit: interrupt enable.
pub const USBCMD_INTE: u32 = 1 << 2;
/// USBCMD bit: host system error enable.
pub const USBCMD_HSEE: u32 = 1 << 3;

/// USBSTS bit: host controller halted.
pub const USBSTS_HCH: u32 = 1 << 0;
/// USBSTS bit: host system error.
pub const USBSTS_HSE: u32 = 1 << 2;
/// USBSTS bit: event interrupt.
pub const USBSTS_EINT: u32 = 1 << 3;
/// USBSTS bit: port change detect.
pub const USBSTS_PCD: u32 = 1 << 4;
/// USBSTS bit: controller not ready.
pub const USBSTS_CNR: u32 = 1 << 11;
/// USBSTS bit: host controller error.
pub const USBSTS_HCE: u32 = 1 << 12;

/// PORTSC bit: current connect status.
pub const PORTSC_CCS: u32 = 1 << 0;
/// PORTSC bit: port enabled/disabled.
pub const PORTSC_PED: u32 = 1 << 1;
/// PORTSC bit: over-current active.
pub const PORTSC_OCA: u32 = 1 << 3;
/// PORTSC bit: port reset.
pub const PORTSC_PR: u32 = 1 << 4;
/// PORTSC field: port link state shift.
pub const PORTSC_PLS_SHIFT: u32 = 5;
/// PORTSC field: port link state mask.
pub const PORTSC_PLS_MASK: u32 = 0xF;
/// PORTSC bit: port power.
pub const PORTSC_PP: u32 = 1 << 9;
/// PORTSC bit: port indicator control shift.
pub const PORTSC_PIC_SHIFT: u32 = 14;
/// PORTSC bit: link state write strobe.
pub const PORTSC_LWS: u32 = 1 << 16;
/// PORTSC bit: connect status change.
pub const PORTSC_CSC: u32 = 1 << 17;
/// PORTSC bit: port enabled/disabled change.
pub const PORTSC_PEC: u32 = 1 << 18;
/// PORTSC bit: warm port reset change.
pub const PORTSC_WRC: u32 = 1 << 19;
/// PORTSC bit: over-current change.
pub const PORTSC_OCC: u32 = 1 << 20;
/// PORTSC bit: port reset change.
pub const PORTSC_PRC: u32 = 1 << 21;
/// PORTSC bit: port link state change.
pub const PORTSC_PLC: u32 = 1 << 22;
/// PORTSC bit: port config error change.
pub const PORTSC_CEC: u32 = 1 << 23;

/// IMAN bit: interrupt pending (write 1 to clear).
pub const IMAN_IP: u32 = 1 << 0;
/// IMAN bit: interrupt enable.
pub const IMAN_IE: u32 = 1 << 1;

/// ERDP bit: event handler busy.
pub const ERDP_EHB: u64 = 1 << 3;

/// Decoded capability registers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capability {
    /// Interface version (16-bit BCD, for example `0x0110`).
    pub hci_version: u16,
    /// Number of device slots.
    pub max_slots: u8,
    /// Number of interrupters (11 bits).
    pub max_interrupters: u16,
    /// Number of ports.
    pub max_ports: u8,
    /// 64-bit addressing capability.
    pub ac64: bool,
    /// Context size: true for 64-byte contexts, false for 32-byte.
    pub csz: bool,
    /// Port power control.
    pub ppc: bool,
    /// Port indicator.
    pub pind: bool,
    /// Maximum primary stream array size.
    pub max_psa_size: u8,
    /// Extended capabilities pointer.
    pub xecp: u16,
}

impl Capability {
    /// Decode HCSPARAMS1 and HCCPARAMS1.
    #[must_use]
    pub const fn decode(hci_version: u16, hcsparams1: u32, hccparams1: u32) -> Self {
        Self {
            hci_version,
            max_slots: (hcsparams1 & 0xFF) as u8,
            max_interrupters: ((hcsparams1 >> 8) & 0x07FF) as u16,
            max_ports: ((hcsparams1 >> 24) & 0xFF) as u8,
            ac64: hccparams1 & 1 != 0,
            csz: hccparams1 & (1 << 2) != 0,
            ppc: hccparams1 & (1 << 3) != 0,
            pind: hccparams1 & (1 << 4) != 0,
            max_psa_size: ((hccparams1 >> 12) & 0xF) as u8,
            xecp: (hccparams1 >> 16) as u16,
        }
    }

    /// The operational register offset (the capability length, 32-bit aligned by the spec).
    #[must_use]
    pub const fn operational_offset(caplength: u8) -> u16 {
        (caplength as u16) & !0x3
    }

    /// The runtime register offset from `RTSOFF` (the low five bits are reserved).
    #[must_use]
    pub const fn runtime_offset(rtsoff: u32) -> u32 {
        rtsoff & !0x1F
    }

    /// The doorbell array offset from `DBOFF` (the low two bits are reserved).
    #[must_use]
    pub const fn doorbell_offset(dboff: u32) -> u32 {
        dboff & !0x3
    }
}

/// Decoded HCSPARAMS2.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hcsparams2 {
    /// Isochronous scheduling threshold.
    pub ist: u8,
    /// Maximum event ring segment table size (the encoded value is 2^N; the raw field is kept).
    pub erst_max: u8,
}

/// Decode HCSPARAMS2.
#[must_use]
pub const fn decode_hcsparams2(value: u32) -> Hcsparams2 {
    Hcsparams2 {
        ist: (value & 0xF) as u8,
        erst_max: ((value >> 4) & 0xF) as u8,
    }
}

/// The decoded PORTSC state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortStatus {
    /// Current connect status.
    pub connected: bool,
    /// Port enabled.
    pub enabled: bool,
    /// Over-current active.
    pub overcurrent: bool,
    /// Port reset in progress.
    pub reset: bool,
    /// Port link state (0..=15).
    pub link_state: u8,
    /// Port power.
    pub powered: bool,
    /// Connect status change.
    pub connect_change: bool,
    /// Enabled change.
    pub enable_change: bool,
    /// Reset change.
    pub reset_change: bool,
    /// Link state change.
    pub link_change: bool,
    /// Over-current change.
    pub overcurrent_change: bool,
}

impl PortStatus {
    /// Decode a PORTSC value.
    #[must_use]
    pub const fn decode(value: u32) -> Self {
        Self {
            connected: value & PORTSC_CCS != 0,
            enabled: value & PORTSC_PED != 0,
            overcurrent: value & PORTSC_OCA != 0,
            reset: value & PORTSC_PR != 0,
            link_state: ((value >> PORTSC_PLS_SHIFT) & PORTSC_PLS_MASK) as u8,
            powered: value & PORTSC_PP != 0,
            connect_change: value & PORTSC_CSC != 0,
            enable_change: value & PORTSC_PEC != 0,
            reset_change: value & PORTSC_PRC != 0,
            link_change: value & PORTSC_PLC != 0,
            overcurrent_change: value & PORTSC_OCC != 0,
        }
    }

    /// Encode the state and change bits. `LWS` is not set.
    #[must_use]
    pub const fn encode(self) -> u32 {
        (if self.connected { PORTSC_CCS } else { 0 })
            | (if self.enabled { PORTSC_PED } else { 0 })
            | (if self.overcurrent { PORTSC_OCA } else { 0 })
            | (if self.reset { PORTSC_PR } else { 0 })
            | (((self.link_state & 0xF) as u32) << PORTSC_PLS_SHIFT)
            | (if self.powered { PORTSC_PP } else { 0 })
            | (if self.connect_change { PORTSC_CSC } else { 0 })
            | (if self.enable_change { PORTSC_PEC } else { 0 })
            | (if self.reset_change { PORTSC_PRC } else { 0 })
            | (if self.link_change { PORTSC_PLC } else { 0 })
            | (if self.overcurrent_change {
                PORTSC_OCC
            } else {
                0
            })
    }
}

#[cfg(test)]
#[path = "regs_ut.rs"]
mod tests;
