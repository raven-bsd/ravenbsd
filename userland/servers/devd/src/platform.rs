// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Platform resource selection for `devd`.
//!
//! A platform driver manifest declares the same selectors as a PCI driver: `bar=<n>` selects a
//! `reg` window and `resources=dma+irq` asks for a DMA buffer and the device's first GIC
//! interrupt. The selection is pure; the caller creates exactly the handles in the returned plan
//! and transfers them to the child. Nothing outside the selected window, buffer or interrupt is
//! created.

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::dtb::{DeviceSpec, GicInterrupt, RegWindow};

/// Default DMA buffer size for a platform function.
pub const PLATFORM_DMA_LEN: usize = 1024 * 1024;

/// Platform page size for MMIO rounding and DMA alignment.
pub const PLATFORM_PAGE_SIZE: usize = 4096;

/// The selectors parsed from a driver manifest entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchArgs {
    /// Selected `reg` index.
    pub bar: usize,
    /// Request a DMA buffer.
    pub dma: bool,
    /// Request the first GIC interrupt.
    pub irq: bool,
    /// DMA buffer size in bytes.
    pub dma_len: usize,
    /// Tokens passed through to the driver.
    pub rest: Vec<String>,
}

/// The resources one platform launch will create.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformPlan {
    /// Selected `reg` index, passed as `bar=<n>`.
    pub bar: usize,
    /// Selected MMIO window from the device tree.
    pub reg: RegWindow,
    /// Requested DMA buffer length.
    pub dma: Option<usize>,
    /// Requested GIC interrupt.
    pub irq: Option<GicInterrupt>,
    /// Tokens passed through to the driver.
    pub rest: Vec<String>,
}

/// One capability transferred to a platform driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grant {
    /// The selected `reg` window, mapped read/write.
    Bar {
        /// `reg` index, passed as `bar=<n>`.
        index: usize,
        /// CPU physical base from the device tree.
        base: u64,
        /// Window length in bytes.
        size: u64,
    },
    /// A device-visible DMA buffer.
    Dma {
        /// Requested length in bytes.
        len: usize,
    },
    /// The device's first GIC interrupt.
    Irq {
        /// GIC INTID (SPI or PPI base included).
        intid: u32,
    },
}

impl PlatformPlan {
    /// The child argv tokens after the executable name.
    #[must_use]
    pub fn tokens(&self) -> Vec<String> {
        let mut tokens = Vec::with_capacity(self.rest.len() + 4);
        tokens.extend(self.rest.iter().cloned());
        tokens.push(format!("bar={}", self.bar));
        tokens.push(format!("bar_size={}", self.reg.size));
        if let Some(len) = self.dma {
            tokens.push(format!("dma={len}"));
        }
        if let Some(irq) = self.irq {
            tokens.push(format!("irq={}", irq.intid()));
        }
        tokens
    }

    /// The capabilities to create and transfer, in child handle order.
    ///
    /// The BAR is always first, then the DMA buffer when requested, then the interrupt. A fresh
    /// child has handles 0 (self), 1 (initfs) and 2 (namespace), so these land at slots 3, 4 and
    /// 5. The PCI path also transfers a configuration page, so its slot numbers differ even
    /// though the argv tokens have the same shape.
    #[must_use]
    pub fn grants(&self) -> Vec<Grant> {
        let mut grants = Vec::with_capacity(3);
        grants.push(Grant::Bar {
            index: self.bar,
            base: self.reg.base,
            size: self.reg.size,
        });
        if let Some(len) = self.dma {
            grants.push(Grant::Dma { len });
        }
        if let Some(irq) = self.irq {
            grants.push(Grant::Irq { intid: irq.intid() });
        }
        grants
    }
}

/// Why a platform resource request cannot be satisfied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformError {
    /// The driver manifest has an unknown selector or bad number.
    BadArgument,
    /// The selected `reg` index is outside the device's windows.
    BarOutOfRange(usize),
    /// `resources=irq` was requested but the device has no GIC interrupt.
    MissingInterrupt,
    /// The requested DMA length is zero or cannot be represented.
    BadDmaLength,
}

impl core::fmt::Display for PlatformError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadArgument => formatter.write_str("invalid platform driver argument"),
            Self::BarOutOfRange(index) => write!(formatter, "platform bar{index} does not exist"),
            Self::MissingInterrupt => {
                formatter.write_str("platform driver asked for an IRQ but the node has none")
            }
            Self::BadDmaLength => formatter.write_str("platform DMA length is zero or too large"),
        }
    }
}

/// Parse the platform driver selectors.
///
/// `bar=N` defaults to 0. `resources=dma+irq` is the PCI shape; `dma`/`irq` bare tokens are also
/// accepted, and `dma_size=N` sets the buffer length. Every other token is forwarded to the child.
pub fn parse_args(args: &[String]) -> Result<LaunchArgs, PlatformError> {
    let mut parsed = LaunchArgs {
        bar: 0,
        dma: false,
        irq: false,
        dma_len: PLATFORM_DMA_LEN,
        rest: Vec::with_capacity(args.len()),
    };
    for argument in args {
        if let Some(value) = argument.strip_prefix("bar=") {
            parsed.bar = value.parse().map_err(|_| PlatformError::BadArgument)?;
            continue;
        }
        if let Some(value) = argument.strip_prefix("dma_size=") {
            parsed.dma_len = value.parse().map_err(|_| PlatformError::BadArgument)?;
            if parsed.dma_len == 0 {
                return Err(PlatformError::BadDmaLength);
            }
            continue;
        }
        if let Some(value) = argument.strip_prefix("resources=") {
            if value == "none" {
                parsed.dma = false;
                parsed.irq = false;
            } else {
                for part in value.split('+') {
                    match part {
                        "dma" => parsed.dma = true,
                        "irq" => parsed.irq = true,
                        _ => return Err(PlatformError::BadArgument),
                    }
                }
            }
            continue;
        }
        match argument.as_str() {
            "dma" => parsed.dma = true,
            "irq" => parsed.irq = true,
            _ => parsed.rest.push(argument.clone()),
        }
    }
    Ok(parsed)
}

/// Select the resource plan for `device`.
///
/// # Errors
///
/// Returns a [`PlatformError`] when a selector is malformed or the device cannot satisfy it. The
/// caller must not create any handle after this fails.
pub fn plan(device: &DeviceSpec, args: &[String]) -> Result<PlatformPlan, PlatformError> {
    let parsed = parse_args(args)?;
    let reg = device
        .regs
        .get(parsed.bar)
        .copied()
        .ok_or(PlatformError::BarOutOfRange(parsed.bar))?;
    let irq = if parsed.irq {
        Some(
            *device
                .interrupts
                .first()
                .ok_or(PlatformError::MissingInterrupt)?,
        )
    } else {
        None
    };
    let dma = if parsed.dma {
        // `dma_alloc` accepts whole pages only; round up here so the grant, the generated token
        // and the kernel's `dma_info.len` all describe the same buffer.
        let length = parsed
            .dma_len
            .checked_add(PLATFORM_PAGE_SIZE - 1)
            .ok_or(PlatformError::BadDmaLength)?
            & !(PLATFORM_PAGE_SIZE - 1);
        if length == 0 {
            return Err(PlatformError::BadDmaLength);
        }
        Some(length)
    } else {
        None
    };
    Ok(PlatformPlan {
        bar: parsed.bar,
        reg,
        dma,
        irq,
        rest: parsed.rest,
    })
}

#[cfg(test)]
#[path = "platform_ut.rs"]
mod tests;
