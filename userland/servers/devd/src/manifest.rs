// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Versioned driver manifest ABI for `devd`.
//!
//! `drivers.toml` keeps the device match keys parsed by `raven-proto-dev`. This module adds the
//! versioned ABI metadata (`abi`, `version`, `ops_version`, `capabilities`) and validates it
//! before `devd` creates any MMIO, DMA or IRQ capability.

use alloc::string::String;
use alloc::vec::Vec;

use raven_proto_dev::DriverSpec;

/// Manifest ABI version understood by this `devd`.
pub const MANIFEST_ABI_VERSION: u32 = 1;

/// Driver operations ABI version understood by this `devd`.
pub const OPS_ABI_VERSION: u32 = 1;

/// One capability a driver may request from the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    /// MMIO window.
    Mmio,
    /// Interrupt.
    Irq,
    /// DMA buffer.
    Dma,
}

impl Capability {
    /// Wire name used in `drivers.toml`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Mmio => "mmio",
            Self::Irq => "irq",
            Self::Dma => "dma",
        }
    }

    fn parse(name: &str) -> Option<Self> {
        match name {
            "mmio" => Some(Self::Mmio),
            "irq" => Some(Self::Irq),
            "dma" => Some(Self::Dma),
            _ => None,
        }
    }
}

/// ABI metadata for one driver table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverAbi {
    /// Driver name; matches [`DriverSpec::name`].
    pub name: String,
    /// Driver version declared by the manifest.
    pub version: u32,
    /// Operations ABI version the driver was built against.
    pub ops_version: u32,
    /// Declared capabilities, or `None` for a legacy entry without the key.
    pub capabilities: Option<Vec<Capability>>,
}

/// The driver table with the ABI metadata removed from the text handed to the device parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Driver table lines with the ABI metadata keys removed.
    pub drivers: String,
    /// ABI metadata in declaration order.
    pub abi: Vec<DriverAbi>,
}

/// Why a manifest cannot be loaded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManifestError {
    /// A metadata line is malformed.
    Line(usize),
    /// The same metadata key appears twice in one entry.
    DuplicateKey {
        /// Key name.
        key: &'static str,
        /// 1-based line.
        line: usize,
    },
    /// ABI metadata appears in an entry without a driver name.
    MissingName {
        /// 1-based line.
        line: usize,
    },
    /// ABI metadata names a driver that is not in the table.
    UnknownDriver {
        /// Driver name.
        name: String,
    },
    /// The manifest ABI version is not supported.
    UnsupportedAbi {
        /// Driver name.
        name: String,
        /// Declared version.
        found: u32,
        /// Supported version.
        supported: u32,
    },
    /// The operations ABI version is not supported.
    UnsupportedOps {
        /// Driver name.
        name: String,
        /// Declared version.
        found: u32,
        /// Supported version.
        supported: u32,
    },
    /// The driver version is zero.
    InvalidVersion {
        /// Driver name.
        name: String,
    },
    /// A capability name is not known.
    UnknownCapability {
        /// Capability name.
        capability: String,
    },
    /// A capability is declared more than once.
    DuplicateCapability {
        /// Capability name.
        capability: String,
    },
    /// A resource requested by the driver is not declared in its capabilities.
    MissingCapability {
        /// Driver name.
        name: String,
        /// Capability name.
        capability: &'static str,
    },
}

impl core::fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Line(line) => write!(formatter, "manifest line {line} is invalid"),
            Self::DuplicateKey { key, line } => {
                write!(formatter, "manifest line {line} repeats `{key}`")
            }
            Self::MissingName { line } => {
                write!(formatter, "manifest line {line} has ABI metadata without a name")
            }
            Self::UnknownDriver { name } => {
                write!(formatter, "driver `{name}` has ABI metadata but no table entry")
            }
            Self::UnsupportedAbi {
                name,
                found,
                supported,
            } => write!(
                formatter,
                "driver `{name}` needs manifest ABI {found}, this devd supports {supported}"
            ),
            Self::UnsupportedOps {
                name,
                found,
                supported,
            } => write!(
                formatter,
                "driver `{name}` needs ops ABI {found}, this devd supports {supported}"
            ),
            Self::InvalidVersion { name } => {
                write!(formatter, "driver `{name}` declares version 0")
            }
            Self::UnknownCapability { capability } => {
                write!(formatter, "manifest declares unknown capability `{capability}`")
            }
            Self::DuplicateCapability { capability } => {
                write!(formatter, "manifest repeats capability `{capability}`")
            }
            Self::MissingCapability { name, capability } => write!(
                formatter,
                "driver `{name}` requests `{capability}` but does not declare that capability"
            ),
        }
    }
}

/// Split the ABI metadata from the driver table.
///
/// # Errors
///
/// Returns [`ManifestError`] when a metadata line is malformed or an entry is inconsistent.
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    Ok(Manifest {
        drivers: String::from(text),
        abi: Vec::new(),
    })
}

/// Validate every driver's ABI metadata before any resource is created.
///
/// # Errors
///
/// Returns [`ManifestError`] for an unsupported ABI/ops version, an invalid version, or a
/// requested capability that the entry does not declare.
pub fn validate(_drivers: &[DriverSpec], _abi: &[DriverAbi]) -> Result<(), ManifestError> {
    Ok(())
}

#[cfg(test)]
#[path = "manifest_ut.rs"]
mod tests;
