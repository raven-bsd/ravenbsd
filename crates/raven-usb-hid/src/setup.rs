// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! USB control-transfer setup packets for the HID class requests.
//!
//! The HID driver needs only host-to-device class requests and the standard GET_DESCRIPTOR. The
//! eight-byte setup packet is the stable wire format the xHCI control-transfer builder consumes.

/// One USB setup packet.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SetupPacket {
    /// `bmRequestType`.
    pub request_type: u8,
    /// `bRequest`.
    pub request: u8,
    /// `wValue`.
    pub value: u16,
    /// `wIndex`.
    pub index: u16,
    /// `wLength`.
    pub length: u16,
}

impl SetupPacket {
    /// Build a packet from its fields.
    #[must_use]
    pub const fn new(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Self {
        Self {
            request_type,
            request,
            value,
            index,
            length,
        }
    }

    /// `SET_PROTOCOL` on an interface; `protocol == 0` selects boot protocol.
    #[must_use]
    pub const fn set_protocol(protocol: u16) -> Self {
        Self::new(0x21, 0x0b, protocol, 0, 0)
    }

    /// `SET_IDLE` with `duration` in 4 ms units and a report identifier.
    #[must_use]
    pub const fn set_idle(duration: u8, report_id: u8) -> Self {
        Self::new(
            0x21,
            0x0a,
            ((duration as u16) << 8) | report_id as u16,
            0,
            0,
        )
    }

    /// `SET_REPORT` for an output report (the keyboard LEDs); `report_type` 2 is output.
    #[must_use]
    pub const fn set_report(report_type: u16, report_id: u8, length: u16) -> Self {
        Self::new(0x21, 0x09, (report_type << 8) | report_id as u16, 0, length)
    }

    /// Standard `GET_DESCRIPTOR` from the interface.
    #[must_use]
    pub const fn get_descriptor(descriptor_type: u8, index: u8, length: u16) -> Self {
        Self::new(
            0x81,
            0x06,
            ((descriptor_type as u16) << 8) | index as u16,
            0,
            length,
        )
    }

    /// Encode the eight wire bytes.
    #[must_use]
    pub const fn encode(&self) -> [u8; 8] {
        [
            self.request_type,
            self.request,
            (self.value & 0xff) as u8,
            (self.value >> 8) as u8,
            (self.index & 0xff) as u8,
            (self.index >> 8) as u8,
            (self.length & 0xff) as u8,
            (self.length >> 8) as u8,
        ]
    }
}

#[cfg(test)]
#[path = "setup_ut.rs"]
mod tests;
