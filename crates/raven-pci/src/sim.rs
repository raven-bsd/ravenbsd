// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Software models of a config space and the BCMstb indirect register pair.
//!
//! These are deterministic host doubles, not hardware access. They implement the observable
//! behaviour the real registers must show: a BAR returns the all-ones mask while it is being
//! sized, an unimplemented BAR reads zero, a missing function reads all ones, and the BCMstb
//! index register selects the bus/device/function for the next data-register access.

use crate::bar::{IO_BAR, MEMORY_64, MEMORY_PREFETCHABLE, TYPE_MASK};
use crate::bdf::Bdf;
use crate::config::{ConfigSpace, NO_DEVICE, REG_BAR0, REG_HEADER, REG_REVISION_CLASS};
use crate::indirect::{EXT_CFG_DATA, EXT_CFG_INDEX, RegisterWindow};

/// Config words a [`SimDevice`] carries for offsets `0x00..=0xff`.
const SIM_WORDS: usize = 64;
/// Devices one [`SimConfig`] holds.
pub const SIM_DEVICES: usize = 4;

/// One simulated BAR.
/// The value [`.value()`][Self::value] reports includes the type bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimBar {
    size: u64,
    kind: crate::bar::BarKind,
    prefetchable: bool,
    value: u64,
    /// Bit 0: the low half is being sized; bit 1: the high half is being sized.
    probing: u8,
}

impl SimBar {
    /// A register that reads zero and ignores writes.
    #[must_use]
    pub const fn unimplemented() -> Self {
        Self {
            size: 0,
            kind: crate::bar::BarKind::Unimplemented,
            prefetchable: false,
            value: 0,
            probing: 0,
        }
    }

    /// A 32-bit memory BAR of `size` bytes (a power of two, at least 16, below 4 GiB).
    #[must_use]
    pub const fn memory32(size: u64) -> Option<Self> {
        if !power_of_two(size) || size < 0x10 || size >= 0x1_0000_0000 {
            return None;
        }
        Some(Self {
            size,
            kind: crate::bar::BarKind::Memory32,
            prefetchable: false,
            value: 0,
            probing: 0,
        })
    }

    /// A 64-bit memory BAR of `size` bytes (a power of two, at least 16).
    #[must_use]
    pub const fn memory64(size: u64) -> Option<Self> {
        if !power_of_two(size) || size < 0x10 {
            return None;
        }
        Some(Self {
            size,
            kind: crate::bar::BarKind::Memory64,
            prefetchable: false,
            value: 0,
            probing: 0,
        })
    }

    /// An IO BAR of `size` bytes (a power of two, at least 4, below 4 GiB).
    #[must_use]
    pub const fn io(size: u64) -> Option<Self> {
        if !power_of_two(size) || size < 0x4 || size >= 0x1_0000_0000 {
            return None;
        }
        Some(Self {
            size,
            kind: crate::bar::BarKind::Io,
            prefetchable: false,
            value: 0,
            probing: 0,
        })
    }

    /// Mark a memory BAR prefetchable.
    #[must_use]
    pub const fn with_prefetchable(self) -> Self {
        Self {
            prefetchable: true,
            ..self
        }
    }

    /// Aperture size in bytes.
    #[must_use]
    pub const fn size(self) -> u64 {
        self.size
    }

    /// Programmed raw register value, including the BAR type bits.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }

    const fn mask(self) -> u64 {
        if self.size == 0 {
            return 0;
        }
        let mask = !(self.size - 1);
        match self.kind {
            crate::bar::BarKind::Unimplemented => 0,
            crate::bar::BarKind::Io => mask & 0xFFFF_FFFC,
            crate::bar::BarKind::Memory32 => mask & 0xFFFF_FFFF,
            crate::bar::BarKind::Memory64 => mask,
        }
    }

    const fn slot_mask(self, high: bool) -> u32 {
        let mask = self.mask();
        if high {
            return (mask >> 32) as u32;
        }
        match self.kind {
            crate::bar::BarKind::Unimplemented => 0,
            crate::bar::BarKind::Io => (mask as u32 & !0x3) | IO_BAR,
            crate::bar::BarKind::Memory32 => mask as u32 & !TYPE_MASK,
            crate::bar::BarKind::Memory64 => {
                let mut bits = (mask as u32 & !TYPE_MASK) | MEMORY_64;
                if self.prefetchable {
                    bits |= MEMORY_PREFETCHABLE;
                }
                bits
            }
        }
    }

    const fn probe_bit(high: bool) -> u8 {
        if high { 0b10 } else { 0b01 }
    }

    const fn read(self, high: bool) -> u32 {
        if matches!(self.kind, crate::bar::BarKind::Unimplemented) {
            return 0;
        }
        if self.probing & Self::probe_bit(high) != 0 {
            return self.slot_mask(high);
        }
        if high {
            (self.value >> 32) as u32
        } else {
            (self.value as u32) | (self.slot_mask(false) & TYPE_MASK)
        }
    }

    const fn write(self, high: bool, value: u32) -> Self {
        if matches!(self.kind, crate::bar::BarKind::Unimplemented) {
            return self;
        }
        let bit = Self::probe_bit(high);
        if value == u32::MAX {
            return Self {
                probing: self.probing | bit,
                ..self
            };
        }
        let mask = self.slot_mask(high);
        let shift = if high { 32 } else { 0 };
        let low_mask = (mask as u64) << shift;
        let merged = (self.value & !low_mask) | ((value as u64 & mask as u64) << shift);
        Self {
            value: merged,
            probing: self.probing & !bit,
            ..self
        }
    }
}

const fn power_of_two(value: u64) -> bool {
    value != 0 && value & (value - 1) == 0
}

/// One simulated function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimDevice {
    bdf: Bdf,
    words: [u32; SIM_WORDS],
    bars: [SimBar; 6],
}

impl SimDevice {
    /// A single-function endpoint with the given vendor and device IDs.
    #[must_use]
    pub fn new(bdf: Bdf, vendor: u16, device: u16) -> Self {
        let mut words = [0u32; SIM_WORDS];
        if let Some(word) = words.get_mut(0) {
            *word = u32::from(vendor) | (u32::from(device) << 16);
        }
        Self {
            bdf,
            words,
            bars: [SimBar::unimplemented(); 6],
        }
    }

    /// Set the class/revision word.
    #[must_use]
    pub fn with_class(mut self, class: u32) -> Self {
        if let Some(word) = self.words.get_mut((REG_REVISION_CLASS as usize) / 4) {
            *word = class;
        }
        self
    }

    /// Set the header-type byte.
    #[must_use]
    pub fn with_header(mut self, header: u8) -> Self {
        if let Some(word) = self.words.get_mut((REG_HEADER as usize) / 4) {
            *word = (*word & 0x0000_FFFF) | (u32::from(header) << 16);
        }
        self
    }

    /// Install a BAR model at `index`.
    ///
    /// Fails when `index` is outside `0..=5`, or when a 64-bit BAR would occupy the last slot.
    pub fn with_bar(&mut self, index: u8, bar: SimBar) -> bool {
        let Some(slot) = self.bars.get_mut(index as usize) else {
            return false;
        };
        if matches!(bar.kind, crate::bar::BarKind::Memory64) && index >= 5 {
            return false;
        }
        *slot = bar;
        true
    }

    /// The function address.
    #[must_use]
    pub const fn bdf(self) -> Bdf {
        self.bdf
    }

    /// The installed BAR model at `index`.
    #[must_use]
    pub fn bar(&self, index: u8) -> Option<SimBar> {
        self.bars.get(index as usize).copied()
    }

    fn bar_for_slot(&self, slot: usize) -> Option<(SimBar, bool)> {
        if let Some(bar) = self.bars.get(slot)
            && !matches!(bar.kind, crate::bar::BarKind::Unimplemented)
        {
            return Some((*bar, false));
        }
        let previous = slot.checked_sub(1)?;
        let bar = *self.bars.get(previous)?;
        if matches!(bar.kind, crate::bar::BarKind::Memory64) {
            Some((bar, true))
        } else {
            None
        }
    }

    fn read(&self, offset: u16) -> u32 {
        if (REG_BAR0..REG_BAR0 + 24).contains(&offset) {
            let slot = ((offset - REG_BAR0) / 4) as usize;
            return match self.bar_for_slot(slot) {
                Some((bar, high)) => bar.read(high),
                None => 0,
            };
        }
        let index = (offset as usize) / 4;
        self.words.get(index).copied().unwrap_or(0)
    }

    fn write(&mut self, offset: u16, value: u32) {
        if (REG_BAR0..REG_BAR0 + 24).contains(&offset) {
            let slot = ((offset - REG_BAR0) / 4) as usize;
            if let Some((bar, high)) = self.bar_for_slot(slot) {
                let updated = bar.write(high, value);
                if high {
                    if let Some(previous) = slot.checked_sub(1)
                        && let Some(target) = self.bars.get_mut(previous)
                    {
                        *target = updated;
                    }
                } else if let Some(target) = self.bars.get_mut(slot) {
                    *target = updated;
                }
            }
            return;
        }
        let index = (offset as usize) / 4;
        if let Some(word) = self.words.get_mut(index) {
            *word = value;
        }
    }
}

/// A config space holding up to [`SIM_DEVICES`] functions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SimConfig {
    devices: [Option<SimDevice>; SIM_DEVICES],
}

impl SimConfig {
    /// An empty config space.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            devices: [None; SIM_DEVICES],
        }
    }

    /// Add a function; returns `false` when the model is full.
    pub fn add(&mut self, device: SimDevice) -> bool {
        for slot in &mut self.devices {
            if slot.is_none() {
                *slot = Some(device);
                return true;
            }
        }
        false
    }

    /// Find a function by address.
    #[must_use]
    pub fn device(&self, bdf: Bdf) -> Option<&SimDevice> {
        self.devices
            .iter()
            .flatten()
            .find(|device| device.bdf == bdf)
    }

    /// Find a function by address, mutably.
    pub fn device_mut(&mut self, bdf: Bdf) -> Option<&mut SimDevice> {
        self.devices
            .iter_mut()
            .flatten()
            .find(|device| device.bdf == bdf)
    }
}

impl ConfigSpace for SimConfig {
    fn read32(&mut self, bdf: Bdf, offset: u16) -> u32 {
        match self.device_mut(bdf) {
            Some(device) => device.read(offset),
            None => NO_DEVICE,
        }
    }

    fn write32(&mut self, bdf: Bdf, offset: u16, value: u32) {
        if let Some(device) = self.device_mut(bdf) {
            device.write(offset, value);
        }
    }
}

/// A BCMstb-style indirect config window over a [`SimConfig`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BcmWindow {
    index: u32,
    memory: SimConfig,
}

impl BcmWindow {
    /// Wrap a config space.
    #[must_use]
    pub const fn new(memory: SimConfig) -> Self {
        Self { index: 0, memory }
    }

    /// The last value written to the index register.
    #[must_use]
    pub const fn index(self) -> u32 {
        self.index
    }

    /// The wrapped config space.
    #[must_use]
    pub const fn config(&self) -> &SimConfig {
        &self.memory
    }

    /// The wrapped config space, mutably.
    pub fn config_mut(&mut self) -> &mut SimConfig {
        &mut self.memory
    }
}

/// Decode a BCMstb index word into an address.
#[must_use]
pub fn decode_index(index: u32) -> Option<Bdf> {
    Bdf::new(
        ((index >> 20) & 0xFF) as u8,
        ((index >> 15) & 0x1F) as u8,
        ((index >> 12) & 0x7) as u8,
    )
    .ok()
}

impl RegisterWindow for BcmWindow {
    fn read32(&mut self, offset: u32) -> u32 {
        if offset == EXT_CFG_INDEX {
            return self.index;
        }
        let Some(page) = offset
            .checked_sub(EXT_CFG_DATA)
            .filter(|page| *page < 0x1000)
        else {
            return 0;
        };
        match decode_index(self.index) {
            Some(bdf) => self.memory.read32(bdf, page as u16),
            None => NO_DEVICE,
        }
    }

    fn write32(&mut self, offset: u32, value: u32) {
        if offset == EXT_CFG_INDEX {
            self.index = value;
            return;
        }
        let Some(page) = offset
            .checked_sub(EXT_CFG_DATA)
            .filter(|page| *page < 0x1000)
        else {
            return;
        };
        if let Some(bdf) = decode_index(self.index) {
            self.memory.write32(bdf, page as u16, value);
        }
    }
}

#[cfg(test)]
#[path = "sim_ut.rs"]
mod tests;
