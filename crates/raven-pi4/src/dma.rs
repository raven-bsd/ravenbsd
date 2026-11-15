// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SDMA and ADMA2 descriptor model, DMA-window rules and bounce decisions.
//!
//! EMMC2 sees RAM through the `emmc2bus` DMA window: bus `0xC000_0000` maps CPU `0` for the first
//! 1 GiB. A buffer outside that window cannot be described directly and needs a bounce buffer; a
//! descriptor table must obey the same rule. The descriptor encoders and validators here are
//! data-only so the host tests can cover alignment, ownership bits, the END bit and the 4 GiB
//! limit before any board run.

use crate::fdt::DmaWindow;

/// Largest transfer one ADMA2 descriptor can describe.
///
/// The 16-bit length field encodes 65536 bytes as zero, which is the convention the SDHCI
/// controllers and the NetBSD donor use.
pub const ADMA2_MAX_LENGTH: usize = 65_536;

/// Encoded size of a 32-bit ADMA2 descriptor.
pub const ADMA2_DESCRIPTOR32_LEN: usize = 8;

/// Encoded size of a 64-bit ADMA2 descriptor.
pub const ADMA2_DESCRIPTOR64_LEN: usize = 12;

/// Maximum data descriptors in one [`Adma2Table`].
pub const ADMA2_MAX_DESCRIPTORS: usize = 16;

/// One-past the largest 32-bit bus address.
const BUS32_LIMIT: u64 = 1u64 << 32;

/// Descriptor address width selected by the capabilities register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorWidth {
    /// The 8-byte descriptor with a 32-bit address.
    Bits32,
    /// The 12-byte descriptor with a 64-bit address.
    Bits64,
}

impl DescriptorWidth {
    /// Encoded descriptor size in bytes.
    pub const fn encoded_len(self) -> usize {
        match self {
            Self::Bits32 => ADMA2_DESCRIPTOR32_LEN,
            Self::Bits64 => ADMA2_DESCRIPTOR64_LEN,
        }
    }

    /// Required descriptor-table alignment in bytes.
    pub const fn alignment(self) -> usize {
        match self {
            Self::Bits32 => 4,
            Self::Bits64 => 8,
        }
    }

    /// Largest bus address a descriptor of this width can start at.
    pub const fn address_limit(self) -> u64 {
        match self {
            Self::Bits32 => u32::MAX as u64,
            Self::Bits64 => u64::MAX,
        }
    }
}

/// The action field of an ADMA2 descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorAction {
    /// No operation.
    Nop,
    /// Transfer data at the descriptor address.
    Transfer,
    /// Follow the descriptor address to another table.
    Link,
}

impl DescriptorAction {
    /// The two-bit action value shifted into the attribute word.
    pub const fn bits(self) -> u16 {
        match self {
            Self::Nop => 0,
            Self::Transfer => 2 << 4,
            Self::Link => 3 << 4,
        }
    }

    /// Decode the two-bit action value.
    pub const fn from_bits(bits: u16) -> Option<Self> {
        match bits {
            0 => Some(Self::Nop),
            2 => Some(Self::Transfer),
            3 => Some(Self::Link),
            _ => None,
        }
    }
}

/// One ADMA2 descriptor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adma2Descriptor {
    /// VALID attribute bit.
    pub valid: bool,
    /// END attribute bit.
    pub end: bool,
    /// INT attribute bit.
    pub interrupt: bool,
    /// Action field.
    pub action: DescriptorAction,
    /// Encoded 16-bit length; zero means [`ADMA2_MAX_LENGTH`] for a transfer.
    pub length: u16,
    /// Bus address of the data buffer or the linked table.
    pub address: u64,
}

impl Adma2Descriptor {
    /// The all-zero terminator descriptor.
    pub const EMPTY: Self = Self {
        valid: false,
        end: false,
        interrupt: false,
        action: DescriptorAction::Nop,
        length: 0,
        address: 0,
    };

    /// Build a transfer descriptor.
    pub fn transfer(
        address: u64,
        length: usize,
        end: bool,
        interrupt: bool,
    ) -> Result<Self, DmaError> {
        if !(1..=ADMA2_MAX_LENGTH).contains(&length) {
            return Err(DmaError::DescriptorLength { length });
        }
        if !length.is_multiple_of(4) {
            return Err(DmaError::MisalignedLength {
                length,
                alignment: 4,
            });
        }
        if !address.is_multiple_of(4) {
            return Err(DmaError::MisalignedAddress {
                address,
                alignment: 4,
            });
        }
        Ok(Self {
            valid: true,
            end,
            interrupt,
            action: DescriptorAction::Transfer,
            length: if length == ADMA2_MAX_LENGTH {
                0
            } else {
                length as u16
            },
            address,
        })
    }

    /// Build a link descriptor.
    pub fn link(address: u64, end: bool) -> Result<Self, DmaError> {
        if !address.is_multiple_of(4) {
            return Err(DmaError::MisalignedAddress {
                address,
                alignment: 4,
            });
        }
        Ok(Self {
            valid: true,
            end,
            interrupt: false,
            action: DescriptorAction::Link,
            length: 0,
            address,
        })
    }

    /// The encoded attribute word.
    pub const fn attribute(self) -> u16 {
        let mut value = self.action.bits();
        if self.valid {
            value |= 1;
        }
        if self.end {
            value |= 1 << 1;
        }
        if self.interrupt {
            value |= 1 << 2;
        }
        value
    }

    /// The transfer length after decoding the zero-means-65536 convention.
    pub const fn actual_length(self) -> usize {
        match self.action {
            DescriptorAction::Transfer => {
                if self.length == 0 {
                    ADMA2_MAX_LENGTH
                } else {
                    self.length as usize
                }
            }
            _ => 0,
        }
    }

    /// Encode the descriptor in the 8-byte 32-bit form.
    pub fn encode32(self) -> Result<[u8; ADMA2_DESCRIPTOR32_LEN], DmaError> {
        if self.address > u64::from(u32::MAX) {
            return Err(DmaError::BusAddressTooHigh {
                address: self.address,
            });
        }
        let mut bytes = [0u8; ADMA2_DESCRIPTOR32_LEN];
        bytes[0..2].copy_from_slice(&self.attribute().to_le_bytes());
        bytes[2..4].copy_from_slice(&self.length.to_le_bytes());
        bytes[4..8].copy_from_slice(&(self.address as u32).to_le_bytes());
        Ok(bytes)
    }

    /// Encode the descriptor in the 12-byte 64-bit form.
    pub fn encode64(self) -> [u8; ADMA2_DESCRIPTOR64_LEN] {
        let mut bytes = [0u8; ADMA2_DESCRIPTOR64_LEN];
        bytes[0..2].copy_from_slice(&self.attribute().to_le_bytes());
        bytes[2..4].copy_from_slice(&self.length.to_le_bytes());
        bytes[4..12].copy_from_slice(&self.address.to_le_bytes());
        bytes
    }

    /// Decode one 8-byte 32-bit descriptor.
    pub fn decode32(bytes: &[u8]) -> Result<Self, DmaError> {
        if bytes.len() < ADMA2_DESCRIPTOR32_LEN {
            return Err(DmaError::BufferTooSmall {
                needed: ADMA2_DESCRIPTOR32_LEN,
                available: bytes.len(),
            });
        }
        let attribute = u16::from_le_bytes([bytes[0], bytes[1]]);
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);
        let address = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as u64;
        Self::from_attribute(attribute, length, address)
    }

    /// Decode one 12-byte 64-bit descriptor.
    pub fn decode64(bytes: &[u8]) -> Result<Self, DmaError> {
        if bytes.len() < ADMA2_DESCRIPTOR64_LEN {
            return Err(DmaError::BufferTooSmall {
                needed: ADMA2_DESCRIPTOR64_LEN,
                available: bytes.len(),
            });
        }
        let attribute = u16::from_le_bytes([bytes[0], bytes[1]]);
        let length = u16::from_le_bytes([bytes[2], bytes[3]]);
        let mut address = [0u8; 8];
        address.copy_from_slice(&bytes[4..12]);
        Self::from_attribute(attribute, length, u64::from_le_bytes(address))
    }

    fn from_attribute(attribute: u16, length: u16, address: u64) -> Result<Self, DmaError> {
        if attribute & !0x003f != 0 {
            return Err(DmaError::ReservedAttribute { attribute });
        }
        let action =
            DescriptorAction::from_bits((attribute >> 4) & 0x3).ok_or(DmaError::InvalidAction)?;
        Ok(Self {
            valid: attribute & 1 != 0,
            end: attribute & (1 << 1) != 0,
            interrupt: attribute & (1 << 2) != 0,
            action,
            length,
            address,
        })
    }

    /// Validate a data descriptor against the bus window.
    pub fn validate(self, window: DmaWindow) -> Result<(), DmaError> {
        if !self.valid {
            return Err(DmaError::InvalidDescriptor);
        }
        match self.action {
            DescriptorAction::Transfer => {
                let length = self.actual_length();
                if !length.is_multiple_of(4) {
                    return Err(DmaError::MisalignedLength {
                        length,
                        alignment: 4,
                    });
                }
                if !self.address.is_multiple_of(4) {
                    return Err(DmaError::MisalignedAddress {
                        address: self.address,
                        alignment: 4,
                    });
                }
                if !within_bus_window(window, self.address, length) {
                    return Err(DmaError::OutOfWindow {
                        address: self.address,
                        length,
                    });
                }
                Ok(())
            }
            DescriptorAction::Link => {
                if self.length != 0 {
                    return Err(DmaError::InvalidDescriptor);
                }
                if !self.address.is_multiple_of(4) {
                    return Err(DmaError::MisalignedAddress {
                        address: self.address,
                        alignment: 4,
                    });
                }
                if !within_bus_window(window, self.address, 0) {
                    return Err(DmaError::OutOfWindow {
                        address: self.address,
                        length: 0,
                    });
                }
                Ok(())
            }
            DescriptorAction::Nop => Err(DmaError::InvalidAction),
        }
    }
}

/// Why a CPU buffer cannot be used directly by the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BounceReason {
    /// The buffer starts below the CPU window.
    BelowWindow,
    /// The buffer ends above the CPU window.
    AboveWindow,
    /// The device-visible address does not fit the descriptor or SDMA width.
    BusAddressTooHigh,
}

/// The result of mapping a CPU buffer into the device bus window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaMapping {
    /// The buffer is directly usable.
    Direct {
        /// Device-visible start address.
        bus_address: u64,
    },
    /// The caller must copy through a buffer inside the window.
    Bounce {
        /// Why the direct mapping is not possible.
        reason: BounceReason,
    },
}

/// A validated SDMA transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SdmaPlan {
    /// 32-bit bus address written to `DMA_ADDR`.
    pub bus_address: u32,
    /// Transfer length in bytes.
    pub length: usize,
}

/// A validated ADMA2 descriptor table and its own bus address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adma2Plan {
    /// Data descriptors.
    pub table: Adma2Table,
    /// Bus address written to `ADMA_SYSTEM_ADDR`.
    pub table_bus_address: u64,
    /// Data bytes described by the table.
    pub data_bytes: usize,
}

/// An owned, bounded ADMA2 descriptor table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Adma2Table {
    descriptors: [Adma2Descriptor; ADMA2_MAX_DESCRIPTORS],
    count: usize,
    width: DescriptorWidth,
}

impl Adma2Table {
    /// An empty table for `width`.
    pub const fn new(width: DescriptorWidth) -> Self {
        Self {
            descriptors: [Adma2Descriptor::EMPTY; ADMA2_MAX_DESCRIPTORS],
            count: 0,
            width,
        }
    }

    /// Build a table for a contiguous CPU buffer that lies inside `window`.
    pub fn from_buffer(
        window: DmaWindow,
        cpu_address: u64,
        length: usize,
        width: DescriptorWidth,
    ) -> Result<Self, DmaError> {
        if length == 0 {
            return Err(DmaError::EmptyBuffer);
        }
        let mut table = Self::new(width);
        let mut cpu = cpu_address;
        let mut remaining = length;
        while remaining > 0 {
            let chunk = remaining.min(ADMA2_MAX_LENGTH);
            let bus = match map_dma(window, cpu, chunk)? {
                DmaMapping::Direct { bus_address } => bus_address,
                DmaMapping::Bounce { reason } => return Err(DmaError::BounceRequired { reason }),
            };
            if width == DescriptorWidth::Bits32
                && bus
                    .checked_add(chunk as u64)
                    .is_none_or(|end| end > BUS32_LIMIT)
            {
                return Err(DmaError::BounceRequired {
                    reason: BounceReason::BusAddressTooHigh,
                });
            }
            let end = chunk == remaining;
            table.push(Adma2Descriptor::transfer(bus, chunk, end, false)?)?;
            cpu = cpu
                .checked_add(chunk as u64)
                .ok_or(DmaError::LengthOverflow)?;
            remaining -= chunk;
        }
        table.validate(window)?;
        Ok(table)
    }

    /// Append one transfer descriptor.
    pub fn push(&mut self, descriptor: Adma2Descriptor) -> Result<(), DmaError> {
        if !descriptor.valid || descriptor.action != DescriptorAction::Transfer {
            return Err(DmaError::InvalidDescriptor);
        }
        if self.count >= ADMA2_MAX_DESCRIPTORS {
            return Err(DmaError::TableFull);
        }
        self.descriptors[self.count] = descriptor;
        self.count += 1;
        Ok(())
    }

    /// Descriptor width.
    pub const fn width(self) -> DescriptorWidth {
        self.width
    }

    /// Number of data descriptors.
    pub const fn len(self) -> usize {
        self.count
    }

    /// True when the table has no data descriptors.
    pub const fn is_empty(self) -> bool {
        self.count == 0
    }

    /// The data descriptors.
    pub fn descriptors(&self) -> &[Adma2Descriptor] {
        &self.descriptors[..self.count]
    }

    /// Total data bytes described by the table.
    pub fn data_bytes(&self) -> usize {
        self.descriptors()
            .iter()
            .map(|descriptor| descriptor.actual_length())
            .sum()
    }

    /// Encoded table size including the all-zero terminator descriptor.
    pub const fn encoded_len(self) -> usize {
        (self.count + 1) * self.width.encoded_len()
    }

    /// Validate the table invariants and every descriptor against `window`.
    pub fn validate(&self, window: DmaWindow) -> Result<(), DmaError> {
        if self.count == 0 {
            return Err(DmaError::EmptyTable);
        }
        let mut ends = 0;
        for (index, descriptor) in self.descriptors().iter().enumerate() {
            descriptor.validate(window)?;
            if self.width == DescriptorWidth::Bits32
                && descriptor
                    .address
                    .checked_add(descriptor.actual_length() as u64)
                    .is_none_or(|end| end > BUS32_LIMIT)
            {
                return Err(DmaError::BusAddressTooHigh {
                    address: descriptor.address,
                });
            }
            if descriptor.end {
                ends += 1;
                if index + 1 != self.count {
                    return Err(DmaError::EarlyEnd);
                }
            }
        }
        if ends != 1 {
            return Err(if ends == 0 {
                DmaError::MissingEnd
            } else {
                DmaError::EarlyEnd
            });
        }
        Ok(())
    }

    /// Encode the table plus the terminator into `out`; returns the encoded length.
    pub fn encode(&self, out: &mut [u8]) -> Result<usize, DmaError> {
        let needed = self.encoded_len();
        if out.len() < needed {
            return Err(DmaError::BufferTooSmall {
                needed,
                available: out.len(),
            });
        }
        let mut offset = 0;
        for descriptor in self.descriptors() {
            match self.width {
                DescriptorWidth::Bits32 => {
                    let bytes = descriptor.encode32()?;
                    out[offset..offset + bytes.len()].copy_from_slice(&bytes);
                    offset += bytes.len();
                }
                DescriptorWidth::Bits64 => {
                    let bytes = descriptor.encode64();
                    out[offset..offset + bytes.len()].copy_from_slice(&bytes);
                    offset += bytes.len();
                }
            }
        }
        let terminator = self.width.encoded_len();
        out[offset..offset + terminator].fill(0);
        Ok(needed)
    }
}

/// Why a DMA mapping or descriptor table was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaError {
    /// The buffer length is zero.
    EmptyBuffer,
    /// A length or address calculation overflowed.
    LengthOverflow,
    /// The address is not aligned for the access.
    MisalignedAddress {
        /// The rejected address.
        address: u64,
        /// Required alignment in bytes.
        alignment: usize,
    },
    /// The length is not a multiple of the required alignment.
    MisalignedLength {
        /// The rejected length.
        length: usize,
        /// Required alignment in bytes.
        alignment: usize,
    },
    /// The direct mapping is outside the window or too wide; use a bounce buffer.
    BounceRequired {
        /// Why the direct mapping cannot be used.
        reason: BounceReason,
    },
    /// A bus address exceeds the descriptor or SDMA width.
    BusAddressTooHigh {
        /// The rejected address.
        address: u64,
    },
    /// A descriptor length is outside 1..=65536.
    DescriptorLength {
        /// The rejected length.
        length: usize,
    },
    /// The table already holds [`ADMA2_MAX_DESCRIPTORS`] entries.
    TableFull,
    /// A table with no descriptors cannot be programmed.
    EmptyTable,
    /// The descriptor is not valid, or a reserved field is set.
    InvalidDescriptor,
    /// The action field is reserved.
    InvalidAction,
    /// Attribute bits outside VALID, END, INT and ACT are set.
    ReservedAttribute {
        /// The rejected attribute word.
        attribute: u16,
    },
    /// No descriptor carries the END bit.
    MissingEnd,
    /// An END bit appears before the last descriptor.
    EarlyEnd,
    /// The destination buffer is too small.
    BufferTooSmall {
        /// Bytes required.
        needed: usize,
        /// Bytes supplied.
        available: usize,
    },
    /// A descriptor address or range lies outside the bus window.
    OutOfWindow {
        /// The rejected address.
        address: u64,
        /// The rejected length.
        length: usize,
    },
}

/// Translate a CPU-physical buffer into the device bus window, or report that a bounce is needed.
pub fn map_dma(
    window: DmaWindow,
    cpu_physical: u64,
    length: usize,
) -> Result<DmaMapping, DmaError> {
    if length == 0 {
        return Err(DmaError::EmptyBuffer);
    }
    let length = u64::try_from(length).map_err(|_| DmaError::LengthOverflow)?;
    let end = cpu_physical
        .checked_add(length)
        .ok_or(DmaError::LengthOverflow)?;
    let window_end = window
        .cpu_base
        .checked_add(window.size)
        .ok_or(DmaError::LengthOverflow)?;
    if window.size == 0 || cpu_physical < window.cpu_base {
        return Ok(DmaMapping::Bounce {
            reason: BounceReason::BelowWindow,
        });
    }
    if end > window_end {
        return Ok(DmaMapping::Bounce {
            reason: BounceReason::AboveWindow,
        });
    }
    let offset = cpu_physical - window.cpu_base;
    let bus_address = window
        .bus_base
        .checked_add(offset)
        .ok_or(DmaError::LengthOverflow)?;
    bus_address
        .checked_add(length)
        .ok_or(DmaError::LengthOverflow)?;
    Ok(DmaMapping::Direct { bus_address })
}

/// Plan an SDMA transfer, rejecting a buffer that would need a bounce buffer.
pub fn plan_sdma(
    window: DmaWindow,
    cpu_physical: u64,
    length: usize,
) -> Result<SdmaPlan, DmaError> {
    let bus_address = match map_dma(window, cpu_physical, length)? {
        DmaMapping::Direct { bus_address } => bus_address,
        DmaMapping::Bounce { reason } => return Err(DmaError::BounceRequired { reason }),
    };
    let length64 = u64::try_from(length).map_err(|_| DmaError::LengthOverflow)?;
    if bus_address > u64::from(u32::MAX)
        || bus_address
            .checked_add(length64)
            .is_none_or(|end| end > BUS32_LIMIT)
    {
        return Err(DmaError::BounceRequired {
            reason: BounceReason::BusAddressTooHigh,
        });
    }
    if !bus_address.is_multiple_of(4) {
        return Err(DmaError::MisalignedAddress {
            address: bus_address,
            alignment: 4,
        });
    }
    Ok(SdmaPlan {
        bus_address: bus_address as u32,
        length,
    })
}

/// Plan an ADMA2 transfer and validate the descriptor table's own DMA placement.
pub fn plan_adma2(
    window: DmaWindow,
    data_cpu: u64,
    data_length: usize,
    table_cpu: u64,
    width: DescriptorWidth,
) -> Result<Adma2Plan, DmaError> {
    let table = Adma2Table::from_buffer(window, data_cpu, data_length, width)?;
    let table_length = table.encoded_len();
    let table_bus_address = match map_dma(window, table_cpu, table_length)? {
        DmaMapping::Direct { bus_address } => bus_address,
        DmaMapping::Bounce { reason } => return Err(DmaError::BounceRequired { reason }),
    };
    if !table_bus_address.is_multiple_of(width.alignment() as u64) {
        return Err(DmaError::MisalignedAddress {
            address: table_bus_address,
            alignment: width.alignment(),
        });
    }
    if width == DescriptorWidth::Bits32
        && (table_bus_address > u64::from(u32::MAX)
            || table_bus_address
                .checked_add(table_length as u64)
                .is_none_or(|end| end > BUS32_LIMIT))
    {
        return Err(DmaError::BounceRequired {
            reason: BounceReason::BusAddressTooHigh,
        });
    }
    Ok(Adma2Plan {
        table,
        table_bus_address,
        data_bytes: data_length,
    })
}

fn within_bus_window(window: DmaWindow, address: u64, length: usize) -> bool {
    let Some(end) = address.checked_add(length as u64) else {
        return false;
    };
    let Some(window_end) = window.bus_base.checked_add(window.size) else {
        return false;
    };
    address >= window.bus_base && end <= window_end
}

#[cfg(test)]
#[path = "dma_ut.rs"]
mod tests;
