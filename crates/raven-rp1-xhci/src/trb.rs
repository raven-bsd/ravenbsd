// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Transfer Request Block encoding.
//!
//! A TRB is 16 bytes: a 64-bit parameter, a 32-bit status and a 32-bit control word. The field
//! positions below are the public xHCI 1.1 definition:
//!
//! * control bit 0: cycle;
//! * control bit 4: chain (transfer TRBs);
//! * control bit 5: interrupt-on-completion;
//! * control bits 10..15: TRB type;
//! * control bits 16..20: endpoint ID (transfer/event TRBs);
//! * control bits 24..31: slot ID (command/event TRBs);
//! * status bits 0..16: transfer length (Normal/Data TRBs);
//! * status bits 24..31: completion code (event TRBs).

/// Control bit: cycle.
pub const TRB_CYCLE: u32 = 1 << 0;
/// Control bit: chain.
pub const TRB_CHAIN: u32 = 1 << 4;
/// Control bit: interrupt-on-completion.
pub const TRB_IOC: u32 = 1 << 5;
/// Control field: TRB type shift.
pub const TRB_TYPE_SHIFT: u32 = 10;
/// Control field: TRB type mask.
pub const TRB_TYPE_MASK: u32 = 0x3F;
/// Control field: endpoint ID shift.
pub const TRB_EP_SHIFT: u32 = 16;
/// Control field: endpoint ID mask.
pub const TRB_EP_MASK: u32 = 0x1F;
/// Control field: slot ID shift.
pub const TRB_SLOT_SHIFT: u32 = 24;
/// Control field: slot ID mask.
pub const TRB_SLOT_MASK: u32 = 0xFF;
/// Parameter field: port ID shift of a Port Status Change Event.
pub const TRB_PORT_SHIFT: u32 = 24;
/// Link TRB control bit: toggle cycle.
pub const TRB_LINK_TC: u32 = 1 << 1;
/// Status field: Normal/Data transfer length mask.
pub const TRB_LENGTH_MASK: u32 = 0x0001_FFFF;
/// Status field: completion code shift.
pub const TRB_COMPLETION_SHIFT: u32 = 24;

/// A TRB type code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrbType {
    /// Normal transfer.
    Normal,
    /// Setup stage.
    SetupStage,
    /// Data stage.
    DataStage,
    /// Status stage.
    StatusStage,
    /// Link.
    Link,
    /// Enable Slot command.
    EnableSlot,
    /// Disable Slot command.
    DisableSlot,
    /// Address Device command.
    AddressDevice,
    /// Configure Endpoint command.
    ConfigureEndpoint,
    /// Reset Endpoint command.
    ResetEndpoint,
    /// Stop Endpoint command.
    StopEndpoint,
    /// Set TR Dequeue Pointer command.
    SetTrDequeue,
    /// Reset Device command.
    ResetDevice,
    /// Transfer Event.
    TransferEvent,
    /// Command Completion Event.
    CommandCompletion,
    /// Port Status Change Event.
    PortStatusChange,
    /// Any other type code.
    Other(u8),
}

impl TrbType {
    /// The six-bit code.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Normal => 1,
            Self::SetupStage => 2,
            Self::DataStage => 3,
            Self::StatusStage => 4,
            Self::Link => 6,
            Self::EnableSlot => 9,
            Self::DisableSlot => 10,
            Self::AddressDevice => 11,
            Self::ConfigureEndpoint => 12,
            Self::ResetEndpoint => 14,
            Self::StopEndpoint => 15,
            Self::SetTrDequeue => 16,
            Self::ResetDevice => 17,
            Self::TransferEvent => 32,
            Self::CommandCompletion => 33,
            Self::PortStatusChange => 34,
            Self::Other(code) => code,
        }
    }

    /// Decode a six-bit code.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Normal,
            2 => Self::SetupStage,
            3 => Self::DataStage,
            4 => Self::StatusStage,
            6 => Self::Link,
            9 => Self::EnableSlot,
            10 => Self::DisableSlot,
            11 => Self::AddressDevice,
            12 => Self::ConfigureEndpoint,
            14 => Self::ResetEndpoint,
            15 => Self::StopEndpoint,
            16 => Self::SetTrDequeue,
            17 => Self::ResetDevice,
            32 => Self::TransferEvent,
            33 => Self::CommandCompletion,
            34 => Self::PortStatusChange,
            other => Self::Other(other),
        }
    }
}

/// One Transfer Request Block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trb {
    /// Parameter dword pair.
    pub parameter: u64,
    /// Status dword.
    pub status: u32,
    /// Control dword.
    pub control: u32,
}

impl Trb {
    /// Build a TRB from its words.
    #[must_use]
    pub const fn from_words(words: [u32; 4]) -> Self {
        Self {
            parameter: (words[1] as u64) << 32 | words[0] as u64,
            status: words[2],
            control: words[3],
        }
    }

    /// The four words.
    #[must_use]
    pub const fn to_words(self) -> [u32; 4] {
        [
            self.parameter as u32,
            (self.parameter >> 32) as u32,
            self.status,
            self.control,
        ]
    }

    /// The TRB type.
    #[must_use]
    pub const fn kind(self) -> TrbType {
        TrbType::from_code(((self.control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK) as u8)
    }

    /// The cycle bit.
    #[must_use]
    pub const fn cycle(self) -> bool {
        self.control & TRB_CYCLE != 0
    }

    /// Set the cycle bit.
    #[must_use]
    pub const fn with_cycle(self, cycle: bool) -> Self {
        Self {
            control: (self.control & !TRB_CYCLE) | (if cycle { TRB_CYCLE } else { 0 }),
            ..self
        }
    }

    /// The interrupt-on-completion bit.
    #[must_use]
    pub const fn ioc(self) -> bool {
        self.control & TRB_IOC != 0
    }

    /// The chain bit.
    #[must_use]
    pub const fn chain(self) -> bool {
        self.control & TRB_CHAIN != 0
    }

    /// The endpoint ID field.
    #[must_use]
    pub const fn endpoint(self) -> u8 {
        ((self.control >> TRB_EP_SHIFT) & TRB_EP_MASK) as u8
    }

    /// Set the endpoint ID; `None` outside `1..=31`.
    #[must_use]
    pub const fn with_endpoint(self, endpoint: u8) -> Option<Self> {
        if endpoint == 0 || endpoint > TRB_EP_MASK as u8 {
            return None;
        }
        Some(Self {
            control: (self.control & !(TRB_EP_MASK << TRB_EP_SHIFT))
                | ((endpoint as u32) << TRB_EP_SHIFT),
            ..self
        })
    }

    /// The slot ID field.
    #[must_use]
    pub const fn slot(self) -> u8 {
        ((self.control >> TRB_SLOT_SHIFT) & TRB_SLOT_MASK) as u8
    }

    /// Set the slot ID field.
    #[must_use]
    pub const fn with_slot(self, slot: u8) -> Self {
        Self {
            control: (self.control & !(TRB_SLOT_MASK << TRB_SLOT_SHIFT))
                | ((slot as u32) << TRB_SLOT_SHIFT),
            ..self
        }
    }

    /// The transfer length field of a Normal/Data TRB.
    #[must_use]
    pub const fn transfer_length(self) -> u32 {
        self.status & TRB_LENGTH_MASK
    }

    /// The completion code field of an event TRB.
    #[must_use]
    pub const fn completion_code(self) -> u8 {
        (self.status >> TRB_COMPLETION_SHIFT) as u8
    }

    /// A Normal transfer TRB.
    #[must_use]
    pub const fn normal(buffer: u64, length: u16, ioc: bool) -> Self {
        Self {
            parameter: buffer,
            status: length as u32,
            control: (TrbType::Normal.code() as u32) << TRB_TYPE_SHIFT
                | (if ioc { TRB_IOC } else { 0 }),
        }
    }

    /// A Link TRB.
    #[must_use]
    pub const fn link(target: u64, toggle_cycle: bool) -> Self {
        Self {
            parameter: target,
            status: 0,
            control: (TrbType::Link.code() as u32) << TRB_TYPE_SHIFT
                | (if toggle_cycle { TRB_LINK_TC } else { 0 }),
        }
    }

    /// An Enable Slot command.
    #[must_use]
    pub const fn enable_slot() -> Self {
        Self {
            parameter: 0,
            status: 0,
            control: (TrbType::EnableSlot.code() as u32) << TRB_TYPE_SHIFT,
        }
    }

    /// An Address Device command; `None` for slot 0.
    #[must_use]
    pub const fn address_device(slot: u8, input_context: u64) -> Option<Self> {
        if slot == 0 {
            return None;
        }
        Some(Self {
            parameter: input_context,
            status: 0,
            control: (TrbType::AddressDevice.code() as u32) << TRB_TYPE_SHIFT
                | ((slot as u32) << TRB_SLOT_SHIFT),
        })
    }

    /// A Configure Endpoint command; `None` for slot 0.
    #[must_use]
    pub const fn configure_endpoint(slot: u8, input_context: u64) -> Option<Self> {
        if slot == 0 {
            return None;
        }
        Some(Self {
            parameter: input_context,
            status: 0,
            control: (TrbType::ConfigureEndpoint.code() as u32) << TRB_TYPE_SHIFT
                | ((slot as u32) << TRB_SLOT_SHIFT),
        })
    }

    /// A Transfer Event; `None` for slot 0 or endpoint 0.
    #[must_use]
    pub const fn transfer_event(
        slot: u8,
        endpoint: u8,
        completion_code: u8,
        trb_pointer: u64,
    ) -> Option<Self> {
        if slot == 0 || endpoint == 0 || endpoint > TRB_EP_MASK as u8 {
            return None;
        }
        Some(Self {
            parameter: trb_pointer,
            status: (completion_code as u32) << TRB_COMPLETION_SHIFT,
            control: (TrbType::TransferEvent.code() as u32) << TRB_TYPE_SHIFT
                | ((endpoint as u32) << TRB_EP_SHIFT)
                | ((slot as u32) << TRB_SLOT_SHIFT),
        })
    }

    /// A Command Completion Event; `None` for slot 0.
    #[must_use]
    pub const fn command_completion(
        slot: u8,
        completion_code: u8,
        command_pointer: u64,
    ) -> Option<Self> {
        if slot == 0 {
            return None;
        }
        Some(Self {
            parameter: command_pointer,
            status: (completion_code as u32) << TRB_COMPLETION_SHIFT,
            control: (TrbType::CommandCompletion.code() as u32) << TRB_TYPE_SHIFT
                | ((slot as u32) << TRB_SLOT_SHIFT),
        })
    }

    /// A Port Status Change Event for `port`.
    #[must_use]
    pub const fn port_status_change(port: u8) -> Self {
        Self {
            parameter: (port as u64) << TRB_PORT_SHIFT,
            status: 0,
            control: (TrbType::PortStatusChange.code() as u32) << TRB_TYPE_SHIFT,
        }
    }
}

#[cfg(test)]
#[path = "trb_ut.rs"]
mod tests;
