// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Platform capability argument plan for `platd`'s hardware backend.
//!
//! The supervisor passes the mailbox, GPIO, RNG and DMA handles as
//! `name=handle:length` arguments. The P8 platform launcher also adds its
//! `bar=`, `bar_size=`, `dma=<length>` and `irq=` tokens; those describe the
//! launcher grant and are not `platd` handle names. The parser ignores those
//! tokens, rejects unknown arguments, and validates every declared window
//! before the program maps a handle.

use alloc::format;
use alloc::string::String;

/// Minimum mailbox window: the property channel registers.
pub const MAILBOX_LEN: usize = 0x40;
/// Minimum GPIO window: the BCM2711 GPFSEL/GPLEV registers.
pub const GPIO_LEN: usize = 0xb4;
/// Minimum RNG200 window: control, status and FIFO.
pub const RNG_LEN: usize = 0x28;
/// Minimum DMA buffer: one mailbox property message.
pub const DMA_LEN: usize = 0x100;
/// The first handle a fresh P8 child receives after self/initfs/namespace.
const FIRST_GRANTED_HANDLE: u32 = 3;

/// One mapped capability window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    /// Child handle number from the supervisor.
    pub handle: u32,
    /// Length passed to `MAP`/`DMA_INFO`.
    pub len: usize,
}

/// The four capabilities the hardware backend consumes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatResources {
    /// VideoCore mailbox registers.
    pub mailbox: Window,
    /// BCM2711 GPIO registers.
    pub gpio: Window,
    /// RNG200 registers.
    pub rng: Window,
    /// Mailbox property DMA buffer.
    pub dma: Window,
}

/// Why a capability plan is unusable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceError {
    /// A required `name=handle:length` argument is absent.
    Missing(&'static str),
    /// The argument value is not `handle:length` with decimal numbers.
    BadArgument(String),
    /// The same handle is declared for two different windows.
    DuplicateHandle(u32),
    /// The handle is one of the child's reserved 0/1/2 slots.
    ReservedHandle(u32),
    /// The declared window is shorter than the hardware register block.
    ShortWindow {
        /// Resource name.
        name: &'static str,
        /// Declared length.
        len: usize,
        /// Required minimum.
        minimum: usize,
    },
    /// The argument is not a declared capability or a launcher token.
    Unknown(String),
}

impl core::fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Missing(name) => write!(formatter, "missing {name}=HANDLE:LENGTH"),
            Self::BadArgument(value) => write!(formatter, "invalid capability `{value}`"),
            Self::DuplicateHandle(handle) => write!(formatter, "duplicate handle {handle}"),
            Self::ReservedHandle(handle) => write!(formatter, "reserved handle {handle}"),
            Self::ShortWindow { name, len, minimum } => {
                write!(formatter, "{name} window is {len} bytes, minimum {minimum}")
            }
            Self::Unknown(argument) => write!(formatter, "unknown argument `{argument}`"),
        }
    }
}

/// Parse the declared capabilities from the driver arguments.
///
/// # Errors
///
/// Returns a [`ResourceError`] when a required window is absent, a value is
/// malformed, a handle is reserved or duplicated, a window is too short, or an
/// argument is neither a capability nor a known launcher token. The caller must
/// not map a handle after this fails.
pub fn parse(args: &[&str]) -> Result<PlatResources, ResourceError> {
    let mut mailbox = None;
    let mut gpio = None;
    let mut rng = None;
    let mut dma = None;

    for argument in args {
        if let Some(value) = argument.strip_prefix("mailbox=") {
            mailbox = Some(parse_window("mailbox", value)?);
        } else if let Some(value) = argument.strip_prefix("gpio=") {
            gpio = Some(parse_window("gpio", value)?);
        } else if let Some(value) = argument.strip_prefix("rng=") {
            rng = Some(parse_window("rng", value)?);
        } else if let Some(value) = argument.strip_prefix("dma=") {
            // The P8 launcher's `dma=<length>` token has no colon; the
            // capability declaration is `dma=<handle>:<length>`.
            if value.contains(':') {
                dma = Some(parse_window("dma", value)?);
            }
        } else if argument.starts_with("bar=")
            || argument.starts_with("bar_size=")
            || argument.starts_with("irq=")
            || *argument == "fake"
            || argument.starts_with("fake=")
        {
            // Launcher/fake tokens; not a capability declaration.
        } else {
            return Err(ResourceError::Unknown(String::from(*argument)));
        }
    }

    let mailbox = mailbox.ok_or(ResourceError::Missing("mailbox"))?;
    let gpio = gpio.ok_or(ResourceError::Missing("gpio"))?;
    let rng = rng.ok_or(ResourceError::Missing("rng"))?;
    let dma = dma.ok_or(ResourceError::Missing("dma"))?;

    check_minimum("mailbox", mailbox, MAILBOX_LEN)?;
    check_minimum("gpio", gpio, GPIO_LEN)?;
    check_minimum("rng", rng, RNG_LEN)?;
    check_minimum("dma", dma, DMA_LEN)?;

    let handles = [mailbox.handle, gpio.handle, rng.handle, dma.handle];
    for (index, handle) in handles.iter().enumerate() {
        if *handle < FIRST_GRANTED_HANDLE {
            return Err(ResourceError::ReservedHandle(*handle));
        }
        if handles.iter().skip(index + 1).any(|other| other == handle) {
            return Err(ResourceError::DuplicateHandle(*handle));
        }
    }

    Ok(PlatResources {
        mailbox,
        gpio,
        rng,
        dma,
    })
}

fn parse_window(name: &'static str, value: &str) -> Result<Window, ResourceError> {
    let (handle, length) = value
        .split_once(':')
        .ok_or_else(|| ResourceError::BadArgument(format!("{name}={value}")))?;
    let handle = handle
        .parse()
        .map_err(|_| ResourceError::BadArgument(format!("{name}={value}")))?;
    let len = length
        .parse()
        .map_err(|_| ResourceError::BadArgument(format!("{name}={value}")))?;
    if len == 0 {
        return Err(ResourceError::BadArgument(format!("{name}={value}")));
    }
    Ok(Window { handle, len })
}

fn check_minimum(name: &'static str, window: Window, minimum: usize) -> Result<(), ResourceError> {
    if window.len < minimum {
        return Err(ResourceError::ShortWindow {
            name,
            len: window.len,
            minimum,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "resources_ut.rs"]
mod tests;
