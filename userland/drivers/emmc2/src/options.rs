// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! EMMC2 launch options.
//!
//! Two launchers exist. `svcd` passes the manifest arguments directly, while the Pi 4 platform
//! launcher (`devd`) transfers the selected MMIO window, DMA buffer and GIC interrupt as child
//! handles 3, 4 and 5 and appends `bar=N`, `bar_size=`, `dma=` and `irq=` tokens. Both spellings
//! are accepted here so the driver has one parser and the host tests cover the board launch.

use alloc::format;
use alloc::string::{String, ToString};

use raven_abi::types::Handle;
use raven_pi4::fdt::DmaWindow;

/// Register window size from the Pi 4 device tree.
pub const MMIO_SIZE: usize = 0x100;
/// DMA window used by the real path.
pub const DMA_BYTES: usize = 64 * 1024;
/// Default DMA buffer requested from the platform launcher.
pub const PLATFORM_DMA_BYTES: usize = 1024 * 1024;
/// First child handle the platform launcher grants after self/initfs/namespace.
pub const FIRST_GRANTED_HANDLE: u32 = 3;

/// Program options parsed from the service arguments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Use the host-backed memory disk (`test`).
    pub test: bool,
    /// MMIO handle.
    pub mmio: Handle,
    /// DMA handle.
    pub dma: Handle,
    /// IRQ handle.
    pub irq: Handle,
    /// MMIO window length.
    pub mmio_size: usize,
    /// DMA buffer length.
    pub dma_size: usize,
    /// EMMC2 base clock in Hz, when the launcher knows it.
    pub clock_hz: Option<u32>,
    /// `emmc2bus` DMA window.
    pub window: DmaWindow,
    /// Optional image path for the host-backed mode.
    pub disk: Option<String>,
    /// GIC INTID from the platform launcher, for diagnostics only.
    pub irq_line: Option<u32>,
    /// Keep serving after a flush in the host-backed mode.
    ///
    /// The one-shot `pi4-emmc2` probe relies on the default stop-on-flush; the root profiles
    /// need the fake card to survive the UFS sync/unmount/remount cycle.
    pub persist: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            test: false,
            mmio: Handle::new(FIRST_GRANTED_HANDLE, 0),
            dma: Handle::new(FIRST_GRANTED_HANDLE + 1, 0),
            irq: Handle::new(FIRST_GRANTED_HANDLE + 2, 0),
            mmio_size: MMIO_SIZE,
            dma_size: DMA_BYTES,
            clock_hz: None,
            // The Pi 4 `emmc2bus` view; the platform launcher overrides this from the DTB.
            window: DmaWindow {
                bus_base: 0xc000_0000,
                cpu_base: 0,
                size: 0x4000_0000,
            },
            disk: None,
            irq_line: None,
            persist: false,
        }
    }
}

impl Options {
    /// Parse the service arguments.
    ///
    /// # Errors
    /// Returns a message for an unknown token, a malformed number or an out-of-range handle.
    pub fn parse(args: &[&str]) -> Result<Self, String> {
        let mut options = Self::default();
        for (index, argument) in args.iter().enumerate() {
            // `svcd` passes manifest args directly; a devd-like launcher prepends the program
            // path or the driver name. Skip that first token when it is one of those.
            if index == 0 && (argument.contains('/') || *argument == "emmc2") {
                continue;
            }
            if *argument == "test" {
                options.test = true;
            } else if *argument == "persist" {
                options.persist = true;
            } else if *argument == "platform" {
                // The launcher's handle layout is already the 3/4/5 default.
            } else if *argument == "serve" {
                // Accepted for symmetry with the other platform drivers.
            } else if let Some(value) = argument.strip_prefix("mmio_handle=") {
                options.mmio = parse_handle(value)?;
            } else if let Some(value) = argument.strip_prefix("dma_handle=") {
                options.dma = parse_handle(value)?;
            } else if let Some(value) = argument.strip_prefix("irq_handle=") {
                options.irq = parse_handle(value)?;
            } else if let Some(value) = argument.strip_prefix("bar=") {
                // The launcher already selected the window; the index is informational.
                let bar = parse_u32(value)?;
                if bar != 0 {
                    return Err(format!("emmc2 has one register window, not bar {bar}"));
                }
            } else if let Some(value) = argument.strip_prefix("bar_size=") {
                options.mmio_size = parse_usize(value)?;
            } else if let Some(value) = argument.strip_prefix("dma=") {
                // The platform launcher passes the granted buffer length. A `handle:length`
                // spelling is accepted for a future direct-capability launcher.
                let length = match value.rsplit_once(':') {
                    Some((_, length)) => length,
                    None => value,
                };
                options.dma_size = parse_usize(length)?;
            } else if let Some(value) = argument.strip_prefix("irq=") {
                options.irq_line = Some(parse_u32(value)?);
            } else if let Some(value) = argument.strip_prefix("mmio_size=") {
                options.mmio_size = parse_usize(value)?;
            } else if let Some(value) = argument.strip_prefix("dma_size=") {
                options.dma_size = parse_usize(value)?;
            } else if let Some(value) = argument.strip_prefix("clock=") {
                options.clock_hz = Some(parse_u32(value)?);
            } else if let Some(value) = argument.strip_prefix("window_bus=") {
                options.window.bus_base = parse_u64(value)?;
            } else if let Some(value) = argument.strip_prefix("window_cpu=") {
                options.window.cpu_base = parse_u64(value)?;
            } else if let Some(value) = argument.strip_prefix("window_size=") {
                options.window.size = parse_u64(value)?;
            } else if let Some(value) = argument.strip_prefix("disk=") {
                options.disk = Some(value.to_string());
            } else {
                return Err(format!("unknown argument `{argument}`"));
            }
        }
        if options.mmio_size == 0 {
            return Err(String::from("mmio window must not be zero"));
        }
        if options.dma_size == 0 {
            return Err(String::from("dma window must not be zero"));
        }
        Ok(options)
    }
}

fn parse_handle(value: &str) -> Result<Handle, String> {
    let raw = value
        .parse::<u32>()
        .map_err(|_| format!("bad handle `{value}`"))?;
    if raw < FIRST_GRANTED_HANDLE {
        return Err(format!("handle {raw} is below the first granted slot"));
    }
    Ok(Handle::new(raw, 0))
}

fn parse_usize(value: &str) -> Result<usize, String> {
    value.parse().map_err(|_| format!("bad number `{value}`"))
}

fn parse_u32(value: &str) -> Result<u32, String> {
    value.parse().map_err(|_| format!("bad number `{value}`"))
}

fn parse_u64(value: &str) -> Result<u64, String> {
    if let Some(hex) = value.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|_| format!("bad number `{value}`"))
    } else {
        value.parse().map_err(|_| format!("bad number `{value}`"))
    }
}

#[cfg(test)]
#[path = "options_ut.rs"]
mod tests;
