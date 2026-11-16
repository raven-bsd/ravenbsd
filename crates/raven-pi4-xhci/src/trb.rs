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
pub const TRB_TYPE_MASK: u32 = 0x3f;
/// Control field: endpoint ID shift.
pub const TRB_EP_SHIFT: u32 = 16;
/// Control field: endpoint ID mask.
pub const TRB_EP_MASK: u32 = 0x1f;
/// Control field: slot ID shift.
pub const TRB_SLOT_SHIFT: u32 = 24;
/// Control field: slot ID mask.
pub const TRB_SLOT_MASK: u32 = 0xff;
/// Link TRB control bit: toggle cycle.
pub const TRB_LINK_TC: u32 = 1 << 1;
/// Status field: Normal/Data transfer length mask.
pub const TRB_LENGTH_MASK: u32 = 0x0001_ffff;
/// Status field: completion code shift.
pub const TRB_COMPLETION_SHIFT: u32 = 24;

/// Standard completion codes used by the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionCode {
    /// Success.
    Success,
    /// A short packet.
    ShortPacket,
    /// The endpoint was stalled.
    Stall,
    /// The transfer was cancelled.
    Cancelled,
    /// Any other code.
    Other(u8),
}

impl CompletionCode {
    /// The eight-bit code.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Success => 1,
            Self::ShortPacket => 13,
            Self::Stall => 6,
            Self::Cancelled => 10,
            Self::Other(code) => code,
        }
    }

    /// Decode an eight-bit code.
    #[must_use]
    pub const fn from_code(code: u8) -> Self {
        match code {
            1 => Self::Success,
            6 => Self::Stall,
            10 => Self::Cancelled,
            13 => Self::ShortPacket,
            other => Self::Other(other),
        }
    }
}

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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Trb {
    /// Parameter dword pair.
    pub parameter: u64,
    /// Status dword.
    pub status: u32,
    /// Control dword.
    pub control: u32,
}

impl Trb {
    /// Build a TRB from its four little-endian words.
    #[must_use]
    pub const fn from_words(words: [u32; 4]) -> Self {
        Self {
            parameter: (words[1] as u64) << 32 | words[0] as u64,
            status: words[2],
            control: words[3],
        }
    }

    /// The four little-endian words.
    #[must_use]
    pub const fn to_words(self) -> [u32; 4] {
        [
            self.parameter as u32,
            (self.parameter >> 32) as u32,
            self.status,
            self.control,
        ]
    }

    fn with_type(mut self, kind: TrbType) -> Self {
        self.control = (self.control & !(TRB_TYPE_MASK << TRB_TYPE_SHIFT))
            | ((kind.code() as u32) << TRB_TYPE_SHIFT);
        self
    }

    /// The TRB type.
    #[must_use]
    pub const fn kind(&self) -> TrbType {
        TrbType::from_code(((self.control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK) as u8)
    }

    /// The cycle bit.
    #[must_use]
    pub const fn cycle(&self) -> bool {
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
    pub const fn ioc(&self) -> bool {
        self.control & TRB_IOC != 0
    }

    /// The chain bit.
    #[must_use]
    pub const fn chain(&self) -> bool {
        self.control & TRB_CHAIN != 0
    }

    /// The endpoint ID field.
    #[must_use]
    pub const fn endpoint(&self) -> u8 {
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
    pub const fn slot(&self) -> u8 {
        ((self.control >> TRB_SLOT_SHIFT) & TRB_SLOT_MASK) as u8
    }

    /// Set the slot ID.
    #[must_use]
    pub const fn with_slot(self, slot: u8) -> Self {
        Self {
            control: (self.control & !(TRB_SLOT_MASK << TRB_SLOT_SHIFT))
                | ((slot as u32) << TRB_SLOT_SHIFT),
            ..self
        }
    }

    /// The transfer length field.
    #[must_use]
    pub const fn length(&self) -> u32 {
        self.status & TRB_LENGTH_MASK
    }

    /// The completion code.
    #[must_use]
    pub const fn completion(&self) -> CompletionCode {
        CompletionCode::from_code((self.status >> TRB_COMPLETION_SHIFT) as u8)
    }

    /// A Normal transfer TRB.
    #[must_use]
    pub fn normal(data: u64, length: u32, ioc: bool) -> Self {
        Self {
            parameter: data,
            status: length & TRB_LENGTH_MASK,
            control: if ioc { TRB_IOC } else { 0 },
        }
        .with_type(TrbType::Normal)
    }

    /// A Setup stage TRB with an 8-byte setup packet.
    #[must_use]
    pub fn setup(packet: u64) -> Self {
        Self {
            parameter: packet,
            status: 8,
            control: TRB_IOC,
        }
        .with_type(TrbType::SetupStage)
    }

    /// A Data stage TRB.
    #[must_use]
    pub fn data(data: u64, length: u32) -> Self {
        Self {
            parameter: data,
            status: length & TRB_LENGTH_MASK,
            control: TRB_IOC,
        }
        .with_type(TrbType::DataStage)
    }

    /// A Status stage TRB.
    #[must_use]
    pub fn status(control: u32) -> Self {
        Self {
            parameter: 0,
            status: 0,
            control: control | TRB_IOC,
        }
        .with_type(TrbType::StatusStage)
    }

    /// A Link TRB.
    #[must_use]
    pub fn link(address: u64, toggle_cycle: bool) -> Self {
        Self {
            parameter: address,
            status: 0,
            control: if toggle_cycle { TRB_LINK_TC } else { 0 },
        }
        .with_type(TrbType::Link)
    }

    /// An Enable Slot command.
    #[must_use]
    pub fn enable_slot() -> Self {
        Self::default().with_type(TrbType::EnableSlot)
    }

    /// A Disable Slot command.
    #[must_use]
    pub fn disable_slot() -> Self {
        Self::default().with_type(TrbType::DisableSlot)
    }

    /// An Address Device command.
    #[must_use]
    pub fn address_device(slot: u8, input_context: u64) -> Self {
        Self {
            parameter: input_context,
            status: 0,
            control: 0,
        }
        .with_type(TrbType::AddressDevice)
        .with_slot(slot)
    }

    /// A Configure Endpoint command.
    #[must_use]
    pub fn configure_endpoint(slot: u8, input_context: u64) -> Self {
        Self {
            parameter: input_context,
            status: 0,
            control: 0,
        }
        .with_type(TrbType::ConfigureEndpoint)
        .with_slot(slot)
    }

    /// A Transfer Event TRB.
    #[must_use]
    pub fn transfer_event(slot: u8, endpoint: u8, completion: CompletionCode, length: u32) -> Self {
        let mut trb = Self {
            parameter: 0,
            status: (completion.code() as u32) << TRB_COMPLETION_SHIFT | (length & TRB_LENGTH_MASK),
            control: 0,
        }
        .with_type(TrbType::TransferEvent)
        .with_slot(slot);
        trb = trb.with_endpoint(endpoint).unwrap_or(trb);
        trb
    }

    /// A Command Completion Event TRB.
    #[must_use]
    pub fn command_completion(slot: u8, command: u64, completion: CompletionCode) -> Self {
        Self {
            parameter: command,
            status: (completion.code() as u32) << TRB_COMPLETION_SHIFT,
            control: 0,
        }
        .with_type(TrbType::CommandCompletion)
        .with_slot(slot)
    }

    /// A Port Status Change Event TRB.
    #[must_use]
    pub fn port_status_change(port: u8) -> Self {
        Self {
            parameter: (port as u64) << 24,
            status: 0,
            control: 0,
        }
        .with_type(TrbType::PortStatusChange)
    }

    /// Whether the Link TRB toggles the cycle state.
    #[must_use]
    pub const fn toggle_cycle(&self) -> bool {
        self.control & TRB_LINK_TC != 0
    }
}

#[cfg(test)]
#[path = "trb_ut.rs"]
mod tests;
