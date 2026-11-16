// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver launch arguments and platform/PCI handle-slot selection.
//!
//! `devd` transfers PCI handles as BAR slot 3, configuration slot 4, DMA slot 5 and IRQ slot 6.
//! The P8 platform launcher transfers BAR slot 3, DMA slot 4 and IRQ slot 5 because there is no
//! configuration page. The argv tokens have the same shape on both paths, so the platform path
//! must select its slot layout explicitly (`platform`/`platform-slots`, `slots=platform`) or pass
//! `bar_handle=`, `dma_handle=` and `irq_handle=`. The default is the PCI layout for the QEMU/hw
//! path.

use core::fmt;

/// Which launcher transferred the handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GrantLayout {
    /// PCI: BAR 3, configuration 4, DMA 5, IRQ 6.
    Pci,
    /// Platform: BAR 3, DMA 4, IRQ 5.
    Platform,
    /// Explicit handle overrides.
    Explicit,
}

/// Why a launch argument set is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsError {
    /// A numeric token could not be parsed.
    BadNumber,
    /// `bar_size` was absent or zero.
    MissingBarSize,
    /// `dma` was absent or zero.
    MissingDma,
    /// `irq` was absent or zero.
    MissingIrq,
    /// A handle index is below the first grant slot (3).
    BadHandle,
    /// Two of the BAR/DMA/IRQ handles are the same slot.
    DuplicateHandle,
    /// An unknown launch token was supplied.
    UnknownArgument,
    /// An unknown `slots=` value was supplied.
    BadSlots,
}

impl fmt::Display for OptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BadNumber => "invalid numeric launch argument",
            Self::MissingBarSize => "missing bar_size token",
            Self::MissingDma => "missing dma token",
            Self::MissingIrq => "missing irq token",
            Self::BadHandle => "handle slot is below the first grant slot",
            Self::DuplicateHandle => "BAR, DMA and IRQ handles must be distinct",
            Self::UnknownArgument => "unknown launch argument",
            Self::BadSlots => "unknown slots= value",
        })
    }
}

/// Parsed launch options.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// MMIO BAR index from the device tree or PCI function.
    pub bar: usize,
    /// MMIO window length.
    pub bar_size: usize,
    /// DMA window length.
    pub dma_size: usize,
    /// IRQ line or GIC INTID.
    pub irq: u32,
    /// BAR handle slot.
    pub bar_handle: u32,
    /// DMA handle slot.
    pub dma_handle: u32,
    /// IRQ handle slot.
    pub irq_handle: u32,
    /// Selected launcher layout.
    pub layout: GrantLayout,
    /// Test-only `nic:quit` mode.
    pub test: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            bar: 0,
            bar_size: 0,
            dma_size: 0,
            irq: 0,
            bar_handle: 3,
            dma_handle: 5,
            irq_handle: 6,
            layout: GrantLayout::Pci,
            test: false,
        }
    }
}

impl Options {
    /// Parse the arguments after the executable name.
    pub fn parse<'a, I: IntoIterator<Item = &'a str>>(args: I) -> Result<Self, OptionsError> {
        let mut options = Self::default();
        let mut irq_seen = false;
        for argument in args {
            match argument {
                "serve" => {}
                "test" => options.test = true,
                "platform" | "platform-slots" => options.select_platform(),
                "pci" => options.select_pci(),
                "slots=platform" => options.select_platform(),
                "slots=pci" => options.select_pci(),
                _ if argument.starts_with("slots=") => return Err(OptionsError::BadSlots),
                _ if argument.starts_with("bar=") => {
                    options.bar = parse_usize(argument, "bar=")?;
                }
                _ if argument.starts_with("bar_size=") => {
                    options.bar_size = parse_usize(argument, "bar_size=")?;
                }
                _ if argument.starts_with("dma=") => {
                    options.dma_size = parse_usize(argument, "dma=")?;
                }
                _ if argument.starts_with("irq=") => {
                    options.irq = parse_u32(argument, "irq=")?;
                    irq_seen = true;
                }
                _ if argument.starts_with("bar_handle=") => {
                    options.bar_handle = parse_u32(argument, "bar_handle=")?;
                    options.layout = GrantLayout::Explicit;
                }
                _ if argument.starts_with("dma_handle=") => {
                    options.dma_handle = parse_u32(argument, "dma_handle=")?;
                    options.layout = GrantLayout::Explicit;
                }
                _ if argument.starts_with("irq_handle=") => {
                    options.irq_handle = parse_u32(argument, "irq_handle=")?;
                    options.layout = GrantLayout::Explicit;
                }
                _ => return Err(OptionsError::UnknownArgument),
            }
        }
        if options.bar_size == 0 {
            return Err(OptionsError::MissingBarSize);
        }
        if options.dma_size == 0 {
            return Err(OptionsError::MissingDma);
        }
        if !irq_seen || options.irq == 0 {
            return Err(OptionsError::MissingIrq);
        }
        if options.bar_handle < 3
            || options.dma_handle < 3
            || options.irq_handle < 3
            || options.bar_handle > 0x00ff_ffff
            || options.dma_handle > 0x00ff_ffff
            || options.irq_handle > 0x00ff_ffff
        {
            return Err(OptionsError::BadHandle);
        }
        if options.bar_handle == options.dma_handle
            || options.bar_handle == options.irq_handle
            || options.dma_handle == options.irq_handle
        {
            return Err(OptionsError::DuplicateHandle);
        }
        Ok(options)
    }

    fn select_platform(&mut self) {
        self.bar_handle = 3;
        self.dma_handle = 4;
        self.irq_handle = 5;
        self.layout = GrantLayout::Platform;
    }

    fn select_pci(&mut self) {
        self.bar_handle = 3;
        self.dma_handle = 5;
        self.irq_handle = 6;
        self.layout = GrantLayout::Pci;
    }
}

fn parse_usize(argument: &str, prefix: &str) -> Result<usize, OptionsError> {
    argument
        .strip_prefix(prefix)
        .ok_or(OptionsError::BadNumber)?
        .parse()
        .map_err(|_| OptionsError::BadNumber)
}

fn parse_u32(argument: &str, prefix: &str) -> Result<u32, OptionsError> {
    argument
        .strip_prefix(prefix)
        .ok_or(OptionsError::BadNumber)?
        .parse()
        .map_err(|_| OptionsError::BadNumber)
}

#[cfg(test)]
#[path = "options_ut.rs"]
mod tests;
