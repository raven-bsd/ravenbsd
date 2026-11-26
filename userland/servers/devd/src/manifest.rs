// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Versioned driver manifest ABI for `devd`.
//!
//! `drivers.toml` keeps the device match keys parsed by `raven-proto-dev`. This module adds the
//! versioned ABI metadata (`abi`, `version`, `ops_version`, `capabilities`) and validates it
//! before `devd` creates any MMIO, DMA or IRQ capability.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
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
    /// Manifest ABI version the entry was written for.
    pub abi: u32,
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
    /// Two metadata entries use the same driver name.
    DuplicateDriver {
        /// Driver name.
        name: String,
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
                write!(
                    formatter,
                    "manifest line {line} has ABI metadata without a name"
                )
            }
            Self::DuplicateDriver { name } => {
                write!(formatter, "driver `{name}` has more than one ABI entry")
            }
            Self::UnknownDriver { name } => {
                write!(
                    formatter,
                    "driver `{name}` has ABI metadata but no table entry"
                )
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
                write!(
                    formatter,
                    "manifest declares unknown capability `{capability}`"
                )
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

#[derive(Default)]
struct PendingAbi {
    name: Option<String>,
    abi: Option<u32>,
    version: Option<u32>,
    ops_version: Option<u32>,
    capabilities: Option<Vec<Capability>>,
}

impl PendingAbi {
    fn has_metadata(&self) -> bool {
        self.abi.is_some()
            || self.version.is_some()
            || self.ops_version.is_some()
            || self.capabilities.is_some()
    }
}

/// Split the ABI metadata from the driver table.
///
/// # Errors
///
/// Returns [`ManifestError`] when a metadata line is malformed or an entry is inconsistent.
pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
    let mut drivers = String::with_capacity(text.len());
    let mut abi = Vec::new();
    let mut pending: Option<PendingAbi> = None;
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let code = line.split('#').next().unwrap_or("").trim();
        if code == "[[driver]]" {
            flush(pending.take(), &mut abi, number)?;
            pending = Some(PendingAbi::default());
            drivers.push_str(line);
            drivers.push('\n');
            continue;
        }
        let Some((key, value)) = code.split_once('=') else {
            drivers.push_str(line);
            drivers.push('\n');
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        if !matches!(key, "abi" | "version" | "ops_version" | "capabilities") {
            if key == "name"
                && let Some(current) = pending.as_mut()
            {
                current.name = Some(unquote(value).ok_or(ManifestError::Line(number))?);
            }
            drivers.push_str(line);
            drivers.push('\n');
            continue;
        }
        let current = pending.as_mut().ok_or(ManifestError::Line(number))?;
        match key {
            "abi" => {
                set_u32(&mut current.abi, value, "abi", number)?;
            }
            "version" => {
                set_u32(&mut current.version, value, "version", number)?;
            }
            "ops_version" => {
                set_u32(&mut current.ops_version, value, "ops_version", number)?;
            }
            "capabilities" => {
                if current.capabilities.is_some() {
                    return Err(ManifestError::DuplicateKey {
                        key: "capabilities",
                        line: number,
                    });
                }
                current.capabilities = Some(parse_capabilities(value, number)?);
            }
            _ => return Err(ManifestError::Line(number)),
        }
    }
    flush(pending.take(), &mut abi, text.lines().count())?;
    Ok(Manifest { drivers, abi })
}

fn flush(
    pending: Option<PendingAbi>,
    abi: &mut Vec<DriverAbi>,
    line: usize,
) -> Result<(), ManifestError> {
    let Some(pending) = pending else {
        return Ok(());
    };
    if !pending.has_metadata() {
        return Ok(());
    }
    let name = pending.name.ok_or(ManifestError::MissingName { line })?;
    if abi.iter().any(|entry| entry.name == name) {
        return Err(ManifestError::DuplicateDriver { name });
    }
    abi.push(DriverAbi {
        name,
        abi: pending.abi.unwrap_or(MANIFEST_ABI_VERSION),
        version: pending.version.unwrap_or(1),
        ops_version: pending.ops_version.unwrap_or(OPS_ABI_VERSION),
        capabilities: pending.capabilities,
    });
    Ok(())
}

fn set_u32(
    slot: &mut Option<u32>,
    value: &str,
    key: &'static str,
    line: usize,
) -> Result<(), ManifestError> {
    if slot.is_some() {
        return Err(ManifestError::DuplicateKey { key, line });
    }
    *slot = Some(parse_u32(value, line)?);
    Ok(())
}

fn unquote(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    if value.contains('"') || value.len() > 128 {
        return None;
    }
    Some(value.to_string())
}

fn parse_u32(value: &str, line: usize) -> Result<u32, ManifestError> {
    let (digits, radix) = match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(digits) => (digits, 16),
        None => (value, 10),
    };
    if digits.is_empty() {
        return Err(ManifestError::Line(line));
    }
    u32::from_str_radix(digits, radix).map_err(|_| ManifestError::Line(line))
}

fn parse_capabilities(value: &str, line: usize) -> Result<Vec<Capability>, ManifestError> {
    let value = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .ok_or(ManifestError::Line(line))?;
    let mut capabilities = Vec::new();
    if value.trim().is_empty() {
        return Ok(capabilities);
    }
    for item in value.split(',') {
        let name = unquote(item.trim()).ok_or(ManifestError::Line(line))?;
        let capability = Capability::parse(&name)
            .ok_or(ManifestError::UnknownCapability { capability: name })?;
        if capabilities.contains(&capability) {
            return Err(ManifestError::DuplicateCapability {
                capability: capability.name().to_string(),
            });
        }
        capabilities.push(capability);
    }
    Ok(capabilities)
}

/// Validate every driver's ABI metadata before any resource is created.
///
/// # Errors
///
/// Returns [`ManifestError`] for an unsupported ABI/ops version, an invalid version, or a
/// requested capability that the entry does not declare.
pub fn validate(drivers: &[DriverSpec], abi: &[DriverAbi]) -> Result<(), ManifestError> {
    let mut entries: BTreeMap<&str, &DriverAbi> = BTreeMap::new();
    for entry in abi {
        entries.insert(entry.name.as_str(), entry);
    }
    for driver in drivers {
        let Some(entry) = entries.remove(driver.name.as_str()) else {
            continue;
        };
        if entry.abi != MANIFEST_ABI_VERSION {
            return Err(ManifestError::UnsupportedAbi {
                name: driver.name.clone(),
                found: entry.abi,
                supported: MANIFEST_ABI_VERSION,
            });
        }
        if entry.ops_version != OPS_ABI_VERSION {
            return Err(ManifestError::UnsupportedOps {
                name: driver.name.clone(),
                found: entry.ops_version,
                supported: OPS_ABI_VERSION,
            });
        }
        if entry.version == 0 {
            return Err(ManifestError::InvalidVersion {
                name: driver.name.clone(),
            });
        }
        if let Some(declared) = &entry.capabilities {
            for capability in requested_capabilities(&driver.args) {
                if !declared.contains(&capability) {
                    return Err(ManifestError::MissingCapability {
                        name: driver.name.clone(),
                        capability: capability.name(),
                    });
                }
            }
        }
    }
    if let Some((name, _)) = entries.into_iter().next() {
        return Err(ManifestError::UnknownDriver {
            name: name.to_string(),
        });
    }
    Ok(())
}

/// The capabilities the current launch selectors ask `devd` to create.
///
/// Every launch selects at least the first BAR, so MMIO is always requested. DMA and IRQ are
/// requested by the same `resources=`/bare tokens the platform and PCI paths consume.
fn requested_capabilities(args: &[String]) -> Vec<Capability> {
    let mut dma = false;
    let mut irq = false;
    for argument in args {
        if let Some(value) = argument.strip_prefix("resources=") {
            if value == "none" {
                dma = false;
                irq = false;
            } else {
                for part in value.split('+') {
                    match part {
                        "dma" => dma = true,
                        "irq" => irq = true,
                        _ => {}
                    }
                }
            }
            continue;
        }
        match argument.as_str() {
            "dma" => dma = true,
            "irq" => irq = true,
            _ => {}
        }
    }
    let mut capabilities = Vec::with_capacity(3);
    capabilities.push(Capability::Mmio);
    if dma {
        capabilities.push(Capability::Dma);
    }
    if irq {
        capabilities.push(Capability::Irq);
    }
    capabilities
}

#[cfg(test)]
#[path = "manifest_ut.rs"]
mod tests;
