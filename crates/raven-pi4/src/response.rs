// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SD command responses: kind, decoding, card status, CID and CSD.
//!
//! The four SDHCI response registers hold a 32-bit response in register 0 or a 128-bit response
//! spread over registers 0..3, always least-significant word first. [`RawLong::bits`] addresses
//! the specification's bit numbers (bit 127 is the most significant bit of response register 3).

/// Response kind a command expects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseKind {
    /// No response.
    None,
    /// 48-bit response with CRC (R1).
    R1,
    /// 48-bit response with CRC and busy (R1b).
    R1b,
    /// 136-bit response with CRC (R2).
    R2,
    /// 48-bit response without CRC (R3).
    R3,
    /// 48-bit response with CRC, the relative card address (R6).
    R6,
    /// 48-bit response with CRC, the CMD8 echo (R7).
    R7,
}

impl ResponseKind {
    /// The two-bit response-type field of the SDHCI command register.
    pub const fn register_bits(self) -> u16 {
        match self {
            Self::None => 0b00,
            Self::R2 => 0b01,
            Self::R1 | Self::R3 | Self::R6 | Self::R7 => 0b10,
            Self::R1b => 0b11,
        }
    }

    /// Whether the host enables the response CRC check for this kind.
    pub const fn crc_check(self) -> bool {
        !matches!(self, Self::None | Self::R3)
    }

    /// Whether the response is 136 bits wide.
    pub const fn is_long(self) -> bool {
        matches!(self, Self::R2)
    }
}

/// One 128-bit response in SDHCI register order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RawLong {
    words: [u32; 4],
}

impl RawLong {
    /// Wrap the four response registers, register 0 first.
    pub const fn from_registers(words: [u32; 4]) -> Self {
        Self { words }
    }

    /// The four response registers, register 0 first.
    pub const fn registers(self) -> [u32; 4] {
        self.words
    }

    /// One response register by index (`0..=3`).
    pub const fn word(self, index: usize) -> Option<u32> {
        if index < 4 {
            Some(self.words[index])
        } else {
            None
        }
    }

    /// The field spanning specification bits `high ..= low`, with bit 0 in register 0.
    ///
    /// Returns zero for an inverted or out-of-range field, or for a field wider than 64 bits.
    pub fn bits(&self, high: usize, low: usize) -> u64 {
        if high < low || high >= 128 || high - low + 1 > 64 {
            return 0;
        }
        let mut value = 0u64;
        let mut bit = high + 1;
        while bit > low {
            bit -= 1;
            value = (value << 1) | u64::from(self.bit(bit));
        }
        value
    }

    fn bit(&self, index: usize) -> u32 {
        let word = self.words[index / 32];
        (word >> (index % 32)) & 1
    }
}

/// A decoded response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// No response.
    None,
    /// R1 or R1b card status.
    R1(CardStatus),
    /// R2, the 128-bit CID or CSD.
    R2(RawLong),
    /// R3, the OCR.
    R3(Ocr),
    /// R6, the relative card address and its status field.
    R6 {
        /// Relative card address.
        rca: u16,
        /// The low 16 bits of card status.
        status: CardStatus,
    },
    /// R7, the CMD8 voltage and pattern echo.
    R7(IfCond),
}

/// Decode the response registers for `kind`.
pub const fn decode(kind: ResponseKind, words: [u32; 4]) -> Response {
    match kind {
        ResponseKind::None => Response::None,
        ResponseKind::R1 | ResponseKind::R1b => Response::R1(CardStatus::from_raw(words[0])),
        ResponseKind::R2 => Response::R2(RawLong::from_registers(words)),
        ResponseKind::R3 => Response::R3(Ocr::from_raw(words[0])),
        ResponseKind::R6 => Response::R6 {
            rca: (words[0] >> 16) as u16,
            status: CardStatus::from_raw(words[0] & 0xffff),
        },
        ResponseKind::R7 => Response::R7(IfCond::from_raw(words[0])),
    }
}

/// Card status bits that report a failed command or an error the caller must not ignore.
const CARD_ERROR_MASK: u32 = 0xFDF9_A008;

/// Current-state field of card status (bits 12:9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CurrentState {
    /// Idle.
    Idle = 0,
    /// Ready.
    Ready = 1,
    /// Identification.
    Identification = 2,
    /// Standby.
    Standby = 3,
    /// Transfer.
    Transfer = 4,
    /// Sending data.
    SendingData = 5,
    /// Receiving data.
    ReceivingData = 6,
    /// Programming.
    Programming = 7,
    /// Disconnected.
    Disconnected = 8,
}

impl CurrentState {
    /// Decode the four-bit field; values 9..=15 are reserved.
    pub const fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            0 => Some(Self::Idle),
            1 => Some(Self::Ready),
            2 => Some(Self::Identification),
            3 => Some(Self::Standby),
            4 => Some(Self::Transfer),
            5 => Some(Self::SendingData),
            6 => Some(Self::ReceivingData),
            7 => Some(Self::Programming),
            8 => Some(Self::Disconnected),
            _ => None,
        }
    }
}

/// The 32-bit R1/R1b card status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardStatus(u32);

impl CardStatus {
    /// Wrap a raw status word.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw status word.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Any error bit is set; the command did not succeed.
    pub const fn command_error(self) -> bool {
        self.0 & CARD_ERROR_MASK != 0
    }

    /// The card expects an application command (bit 5).
    pub const fn app_cmd(self) -> bool {
        self.0 & (1 << 5) != 0
    }

    /// The card is ready for data (bit 8).
    pub const fn ready_for_data(self) -> bool {
        self.0 & (1 << 8) != 0
    }

    /// The card is locked (bit 25).
    pub const fn is_locked(self) -> bool {
        self.0 & (1 << 25) != 0
    }

    /// The four-bit current-state field.
    pub const fn state(self) -> Option<CurrentState> {
        CurrentState::from_bits((self.0 >> 9) & 0xf)
    }
}

/// The OCR returned by ACMD41 (R3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ocr(u32);

impl Ocr {
    /// Wrap a raw OCR word.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw OCR word.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The power-up sequence finished (bit 31).
    pub const fn ready(self) -> bool {
        self.0 & (1 << 31) != 0
    }

    /// The card is SDHC/SDXC (CCS, bit 30) when ready.
    pub const fn capacity_status(self) -> bool {
        self.0 & (1 << 30) != 0
    }

    /// The voltage-window bits (23:15).
    pub const fn voltage_window(self) -> u32 {
        (self.0 >> 15) & 0x1ff
    }
}

/// The CMD8 (SEND_IF_COND) echo, R7.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IfCond(u32);

impl IfCond {
    /// Wrap a raw R7 word.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The raw R7 word.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// The accepted voltage field (bits 11:8).
    pub const fn voltage_accepted(self) -> u8 {
        ((self.0 >> 8) & 0xf) as u8
    }

    /// The echoed check pattern (bits 7:0).
    pub const fn check_pattern(self) -> u8 {
        self.0 as u8
    }
}

/// Card identification register (CID), a decoded 128-bit R2 response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cid(RawLong);

impl Cid {
    /// Decode a 128-bit CID response.
    pub const fn from_raw(raw: RawLong) -> Self {
        Self(raw)
    }

    /// The underlying response.
    pub const fn raw(self) -> RawLong {
        self.0
    }

    /// Manufacturer ID (bits 127:120).
    pub fn manufacturer(&self) -> u8 {
        self.0.bits(127, 120) as u8
    }

    /// OEM/application ID (bits 119:104).
    pub fn oem(&self) -> u16 {
        self.0.bits(119, 104) as u16
    }

    /// Product name, five bytes (bits 103:64).
    pub fn product_name(&self) -> [u8; 5] {
        [
            self.0.bits(103, 96) as u8,
            self.0.bits(95, 88) as u8,
            self.0.bits(87, 80) as u8,
            self.0.bits(79, 72) as u8,
            self.0.bits(71, 64) as u8,
        ]
    }

    /// Product revision (bits 63:56).
    pub fn revision(&self) -> u8 {
        self.0.bits(63, 56) as u8
    }

    /// Product serial number (bits 55:24).
    pub fn serial(&self) -> u32 {
        self.0.bits(55, 24) as u32
    }
}

/// Card-specific data register (CSD), a decoded 128-bit R2 response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Csd(RawLong);

impl Csd {
    /// Decode a 128-bit CSD response.
    pub const fn from_raw(raw: RawLong) -> Self {
        Self(raw)
    }

    /// The underlying response.
    pub const fn raw(self) -> RawLong {
        self.0
    }

    /// CSD structure version (bits 127:126): 0 for v1, 1 for v2.
    pub fn structure_version(&self) -> u8 {
        self.0.bits(127, 126) as u8
    }

    /// Read block length in bytes (bits 83:80, `1 << value`).
    pub fn read_block_len(&self) -> Option<u32> {
        let exponent = self.0.bits(83, 80) as u32;
        1u32.checked_shl(exponent)
    }

    /// Card capacity in bytes, or `None` for a reserved structure or an overflowing field.
    pub fn capacity_bytes(&self) -> Option<u64> {
        match self.structure_version() {
            1 => {
                let c_size = self.0.bits(69, 48);
                c_size.checked_add(1)?.checked_mul(512 * 1024)
            }
            0 => {
                let c_size = self.0.bits(73, 62);
                let c_size_mult = self.0.bits(49, 47);
                let read_bl_len = self.read_block_len()?;
                let multiplier = 1u64.checked_shl(2 + c_size_mult as u32)?;
                c_size
                    .checked_add(1)?
                    .checked_mul(multiplier)?
                    .checked_mul(u64::from(read_bl_len))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "response_ut.rs"]
mod tests;
