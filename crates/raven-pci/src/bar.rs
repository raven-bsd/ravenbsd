// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Base-address register detection, sizing and assignment.
//!
//! The sizing algorithm is the PCI one: save the register, write all ones, read the mask back,
//! restore the saved value, and derive `size = !mask + 1`. A 64-bit BAR consumes two consecutive
//! registers and only the low register carries the type bits. Every write is verified by
//! reading the register back, because a silent mismatch would leave the device decoding an
//! address the host never assigned.

use core::fmt;

use crate::bdf::Bdf;
use crate::config::{ConfigSpace, REG_BAR0};
use crate::region::Region;

/// Number of 32-bit BAR registers in a type-0 header.
pub const BAR_COUNT: u8 = 6;
/// Mask selecting the memory/IO and type bits of a BAR.
pub const TYPE_MASK: u32 = 0x0000_000F;
/// Bit 0 of a BAR: IO space (memory otherwise).
pub const IO_BAR: u32 = 0x0000_0001;
/// Bits 2:1 of a memory BAR: 64-bit address.
pub const MEMORY_64: u32 = 0x0000_0004;
/// Bit 3 of a memory BAR: prefetchable.
pub const MEMORY_PREFETCHABLE: u32 = 0x0000_0008;
/// The all-ones word written while sizing.
pub const SIZE_PROBE: u32 = 0xFFFF_FFFF;

/// How a BAR decodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarKind {
    /// The function does not implement this register.
    Unimplemented,
    /// 32-bit memory address.
    Memory32,
    /// 64-bit memory address occupying this and the next register.
    Memory64,
    /// 32-bit IO address.
    Io,
}

impl fmt::Display for BarKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unimplemented => f.write_str("unimplemented"),
            Self::Memory32 => f.write_str("memory32"),
            Self::Memory64 => f.write_str("memory64"),
            Self::Io => f.write_str("io"),
        }
    }
}

/// One probed base-address register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bar {
    /// Register index, `0..=5`.
    pub index: u8,
    /// Decode kind.
    pub kind: BarKind,
    /// The prefetchable bit of a memory BAR.
    pub prefetchable: bool,
    /// Currently programmed address (zero before assignment).
    pub address: u64,
    /// Aperture size in bytes; zero for an unimplemented register.
    pub size: u64,
}

impl Bar {
    /// An empty placeholder; [`BarKind::Unimplemented`] with zero address and size.
    pub const NONE: Self = Self {
        index: 0,
        kind: BarKind::Unimplemented,
        prefetchable: false,
        address: 0,
        size: 0,
    };

    /// True for a 32-bit or 64-bit memory BAR.
    #[must_use]
    pub const fn is_memory(self) -> bool {
        matches!(self.kind, BarKind::Memory32 | BarKind::Memory64)
    }

    /// The decoded aperture, or `None` for an unimplemented or zero-sized BAR.
    #[must_use]
    pub const fn region(self) -> Option<Region> {
        Region::new(self.address, self.size)
    }
}

/// Why a BAR is unusable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BarError {
    /// The register index is not `0..=5`.
    Index(u8),
    /// A 64-bit BAR claims the last register, so its upper half is missing.
    MissingUpperHalf(u8),
    /// The sizing mask is not the complement of a power-of-two size.
    MalformedMask {
        /// Register index.
        index: u8,
        /// The offending mask.
        mask: u64,
    },
    /// The requested base is not a multiple of the aperture size.
    MisalignedBase {
        /// Requested base.
        base: u64,
        /// Aperture size.
        size: u64,
    },
    /// The requested base does not fit the BAR width.
    BaseOutOfRange {
        /// Requested base.
        base: u64,
    },
    /// The register does not decode an address.
    Unimplemented(u8),
    /// The read-back address differs from the programmed one.
    VerifyMismatch {
        /// Register index.
        index: u8,
        /// Address the host wrote.
        expected: u64,
        /// Address the device reports.
        observed: u64,
    },
    /// A window has no room for the request.
    NoSpace {
        /// Aperture size that was requested.
        size: u64,
    },
}

impl fmt::Display for BarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Index(index) => write!(f, "BAR register {index} is outside 0..=5"),
            Self::MissingUpperHalf(index) => {
                write!(f, "BAR {index} is 64-bit and has no upper register")
            }
            Self::MalformedMask { index, mask } => {
                write!(f, "BAR {index} sizing mask {mask:#x} is not a BAR mask")
            }
            Self::MisalignedBase { base, size } => {
                write!(f, "BAR base {base:#x} is not aligned to its size {size:#x}")
            }
            Self::BaseOutOfRange { base } => {
                write!(f, "BAR base {base:#x} does not fit the BAR width")
            }
            Self::Unimplemented(index) => write!(f, "BAR {index} is not implemented"),
            Self::VerifyMismatch {
                index,
                expected,
                observed,
            } => write!(
                f,
                "BAR {index} read back {observed:#x} instead of {expected:#x}"
            ),
            Self::NoSpace { size } => {
                write!(f, "the window has no room for a {size:#x}-byte aperture")
            }
        }
    }
}

impl core::error::Error for BarError {}

/// The config-space offset of BAR register `index`.
#[must_use]
pub const fn register(index: u8) -> Option<u16> {
    if index >= BAR_COUNT {
        return None;
    }
    Some(REG_BAR0 + (index as u16) * 4)
}

/// Probe one BAR.
///
/// The kind is derived from the sizing mask, not from the saved value: a BAR that firmware left
/// unassigned reads zero, and only the all-ones probe reveals whether it is memory or IO.
///
/// # Errors
///
/// Returns [`BarError`] for an out-of-range index, a missing 64-bit upper half or an implausible
/// sizing mask.
pub fn probe<C: ConfigSpace + ?Sized>(
    config: &mut C,
    bdf: Bdf,
    index: u8,
) -> Result<Bar, BarError> {
    let offset = register(index).ok_or(BarError::Index(index))?;
    let low_saved = config.read32(bdf, offset);
    config.write32(bdf, offset, SIZE_PROBE);
    let low_mask = config.read32(bdf, offset);
    config.write32(bdf, offset, low_saved);

    if low_mask == 0 {
        return Ok(Bar { index, ..Bar::NONE });
    }
    let kind = if low_mask & IO_BAR != 0 {
        BarKind::Io
    } else if low_mask & TYPE_MASK & MEMORY_64 != 0 {
        BarKind::Memory64
    } else {
        BarKind::Memory32
    };
    let prefetchable = low_mask & MEMORY_PREFETCHABLE != 0;

    let (mask, address) = match kind {
        BarKind::Io => (
            u64::from(low_mask & !0x3) | 0xFFFF_FFFF_0000_0000,
            u64::from(low_saved & !0x3),
        ),
        BarKind::Memory32 => (
            u64::from(low_mask & !TYPE_MASK) | 0xFFFF_FFFF_0000_0000,
            u64::from(low_saved & !TYPE_MASK),
        ),
        BarKind::Memory64 => {
            let high_index = index
                .checked_add(1)
                .filter(|next| *next < BAR_COUNT)
                .ok_or(BarError::MissingUpperHalf(index))?;
            let high_offset = register(high_index).ok_or(BarError::Index(high_index))?;
            let high_saved = config.read32(bdf, high_offset);
            config.write32(bdf, high_offset, SIZE_PROBE);
            let high_mask = config.read32(bdf, high_offset);
            config.write32(bdf, high_offset, high_saved);
            let mask = (u64::from(high_mask) << 32) | u64::from(low_mask & !TYPE_MASK);
            let address = (u64::from(high_saved) << 32) | u64::from(low_saved & !TYPE_MASK);
            (mask, address)
        }
        BarKind::Unimplemented => return Ok(Bar { index, ..Bar::NONE }),
    };

    if mask == 0 {
        return Err(BarError::MalformedMask { index, mask: 0 });
    }
    let size = (!mask).wrapping_add(1);
    let minimum = if kind == BarKind::Io { 0x4 } else { 0x10 };
    if !size.is_power_of_two() || size < minimum {
        return Err(BarError::MalformedMask { index, mask });
    }
    Ok(Bar {
        index,
        kind,
        prefetchable,
        address,
        size,
    })
}

/// Program one BAR and verify the read-back.
///
/// A 64-bit BAR writes its high half first, so the device never decodes a new low half against a
/// stale high half.
///
/// # Errors
///
/// Returns [`BarError`] for a malformed size, a misaligned base, an address the BAR width cannot
/// hold, an unimplemented register or a read-back mismatch.
pub fn program<C: ConfigSpace + ?Sized>(
    config: &mut C,
    bdf: Bdf,
    bar: &Bar,
    base: u64,
) -> Result<(), BarError> {
    if bar.size == 0 || !bar.size.is_power_of_two() {
        return Err(BarError::MalformedMask {
            index: bar.index,
            mask: !(bar.size.wrapping_sub(1)),
        });
    }
    if !base.is_multiple_of(bar.size) {
        return Err(BarError::MisalignedBase {
            base,
            size: bar.size,
        });
    }
    let offset = register(bar.index).ok_or(BarError::Index(bar.index))?;
    match bar.kind {
        BarKind::Unimplemented => Err(BarError::Unimplemented(bar.index)),
        BarKind::Io => {
            if base > u64::from(u32::MAX) {
                return Err(BarError::BaseOutOfRange { base });
            }
            config.write32(bdf, offset, (base as u32) | IO_BAR);
            verify(config, bdf, bar.index, offset, base, !0x3)
        }
        BarKind::Memory32 => {
            if base > u64::from(u32::MAX) {
                return Err(BarError::BaseOutOfRange { base });
            }
            let mut value = base as u32;
            if bar.prefetchable {
                value |= MEMORY_PREFETCHABLE;
            }
            config.write32(bdf, offset, value);
            verify(config, bdf, bar.index, offset, base, !TYPE_MASK)
        }
        BarKind::Memory64 => {
            let high_index = bar
                .index
                .checked_add(1)
                .filter(|next| *next < BAR_COUNT)
                .ok_or(BarError::MissingUpperHalf(bar.index))?;
            let high_offset = register(high_index).ok_or(BarError::Index(high_index))?;
            config.write32(bdf, high_offset, (base >> 32) as u32);
            let mut value = base as u32 & !TYPE_MASK;
            value |= MEMORY_64;
            if bar.prefetchable {
                value |= MEMORY_PREFETCHABLE;
            }
            config.write32(bdf, offset, value);
            let observed = (u64::from(config.read32(bdf, high_offset)) << 32)
                | u64::from(config.read32(bdf, offset) & !TYPE_MASK);
            if observed == base {
                Ok(())
            } else {
                Err(BarError::VerifyMismatch {
                    index: bar.index,
                    expected: base,
                    observed,
                })
            }
        }
    }
}

fn verify<C: ConfigSpace + ?Sized>(
    config: &mut C,
    bdf: Bdf,
    index: u8,
    offset: u16,
    base: u64,
    mask: u32,
) -> Result<(), BarError> {
    let observed = u64::from(config.read32(bdf, offset) & mask);
    if observed == base {
        Ok(())
    } else {
        Err(BarError::VerifyMismatch {
            index,
            expected: base,
            observed,
        })
    }
}

/// All implemented BARs of one function, in register order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarSet {
    entries: [Bar; BAR_COUNT as usize],
    count: usize,
}

impl BarSet {
    /// Probe every BAR register of a function.
    ///
    /// The upper half of a 64-bit BAR is consumed by its low half and not listed separately. A
    /// malformed BAR aborts the set, because a partial map would misdescribe the aperture.
    ///
    /// # Errors
    ///
    /// Returns the first [`BarError`] from [`probe`].
    pub fn read<C: ConfigSpace + ?Sized>(config: &mut C, bdf: Bdf) -> Result<Self, BarError> {
        let mut entries = [Bar::NONE; BAR_COUNT as usize];
        let mut count = 0usize;
        let mut index = 0u8;
        while index < BAR_COUNT {
            let bar = probe(config, bdf, index)?;
            if bar.kind != BarKind::Unimplemented
                && let Some(slot) = entries.get_mut(count)
            {
                *slot = bar;
                count = count.saturating_add(1);
            }
            index = if bar.kind == BarKind::Memory64 {
                index.saturating_add(2)
            } else {
                index.saturating_add(1)
            };
        }
        Ok(Self { entries, count })
    }

    /// Number of implemented BARs.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.count
    }

    /// Iterate the implemented BARs.
    pub fn iter(&self) -> impl Iterator<Item = &Bar> {
        self.entries.iter().take(self.count)
    }

    /// Find the memory BAR that fully covers `region`.
    ///
    /// An unimplemented or zero-sized BAR never matches; a 64-bit BAR matches on its full
    /// aperture.
    #[must_use]
    pub fn find_covering(&self, region: Region) -> Option<&Bar> {
        self.iter().find(|bar| {
            bar.is_memory()
                && bar
                    .region()
                    .is_some_and(|window| window.contains_region(region))
        })
    }
}

/// A first-fit allocator over one PCI memory window.
///
/// Addresses come out in ascending order with the requested alignment; nothing is released,
/// because the enumeration path assigns each aperture exactly once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowAllocator {
    window: Region,
    cursor: u64,
}

impl WindowAllocator {
    /// Start at the bottom of `window`.
    #[must_use]
    pub const fn new(window: Region) -> Self {
        Self {
            cursor: window.base(),
            window,
        }
    }

    /// The window being allocated.
    #[must_use]
    pub const fn window(self) -> Region {
        self.window
    }

    /// Next unallocated address.
    #[must_use]
    pub const fn cursor(self) -> u64 {
        self.cursor
    }

    /// Reserve an aligned aperture, or `None` when the window is full.
    pub fn allocate(&mut self, size: u64, alignment: u64) -> Option<Region> {
        if size == 0 || alignment == 0 || !alignment.is_power_of_two() {
            return None;
        }
        let aligned = align_up(self.cursor, alignment)?;
        let end = aligned.checked_add(size)?;
        if end > self.window.end() {
            return None;
        }
        self.cursor = end;
        Region::new(aligned, size)
    }

    /// Assign and program the next aperture for `bar`.
    ///
    /// # Errors
    ///
    /// Returns [`BarError::NoSpace`] when the window cannot hold the aperture, otherwise the
    /// [`program`] error.
    pub fn assign<C: ConfigSpace + ?Sized>(
        &mut self,
        config: &mut C,
        bdf: Bdf,
        bar: &mut Bar,
    ) -> Result<Region, BarError> {
        let region = self
            .allocate(bar.size, bar.size)
            .ok_or(BarError::NoSpace { size: bar.size })?;
        program(config, bdf, bar, region.base())?;
        bar.address = region.base();
        Ok(region)
    }
}

/// Round `value` up to the next multiple of `alignment` (a power of two).
#[must_use]
pub fn align_up(value: u64, alignment: u64) -> Option<u64> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return None;
    }
    let mask = alignment - 1;
    value.checked_add(mask).map(|value| value & !mask)
}

#[cfg(test)]
#[path = "bar_ut.rs"]
mod tests;
