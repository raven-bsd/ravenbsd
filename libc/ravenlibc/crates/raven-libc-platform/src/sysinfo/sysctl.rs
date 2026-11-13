// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The BSD `sysctl` API over the `sys:` scheme.
//!
//! There is no `sysctl` syscall. `sysctlbyname("kern.hostname")` opens `sys:kern/hostname`; the
//! numeric API accepts only the MIB numbers common to NetBSD, FreeBSD and OpenBSD. Values are
//! decimal text (with a trailing newline) for numbers and plain text for strings. A short buffer
//! is `ENOMEM` with the needed length; an unknown name or MIB is `ENOENT`.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

/// `CTL_KERN`.
pub const CTL_KERN: i32 = 1;
/// `CTL_HW`.
pub const CTL_HW: i32 = 6;
/// `KERN_OSTYPE`.
pub const KERN_OSTYPE: i32 = 1;
/// `KERN_OSRELEASE`.
pub const KERN_OSRELEASE: i32 = 2;
/// `KERN_VERSION`.
pub const KERN_VERSION: i32 = 4;
/// `KERN_HOSTNAME`.
pub const KERN_HOSTNAME: i32 = 10;
/// `KERN_HOSTID`.
pub const KERN_HOSTID: i32 = 11;
/// `KERN_DOMAINNAME` (NetBSD and OpenBSD).
pub const KERN_DOMAINNAME: i32 = 22;
/// `HW_MACHINE`.
pub const HW_MACHINE: i32 = 1;
/// `HW_NCPU`.
pub const HW_NCPU: i32 = 3;
/// `HW_PHYSMEM`.
pub const HW_PHYSMEM: i32 = 5;
/// `HW_PAGESIZE`.
pub const HW_PAGESIZE: i32 = 7;

/// How a `sys:` value is interpreted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// Printable text.
    Text,
    /// Decimal integer of the caller's width.
    Integer,
}

/// One supported MIB row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MibEntry {
    /// Numeric MIB path.
    pub mib: &'static [i32],
    /// BSD name.
    pub name: &'static str,
    /// `sys:` path without the scheme prefix.
    pub path: &'static [u8],
    /// Value interpretation.
    pub kind: ValueKind,
}

/// The static MIB table (doc 11 section 3.11); nothing else is supported by number.
pub const MIB_TABLE: &[MibEntry] = &[
    MibEntry {
        mib: &[CTL_KERN, KERN_OSTYPE],
        name: "kern.ostype",
        path: b"kern/ostype",
        kind: ValueKind::Text,
    },
    MibEntry {
        mib: &[CTL_KERN, KERN_OSRELEASE],
        name: "kern.osrelease",
        path: b"kern/osrelease",
        kind: ValueKind::Text,
    },
    MibEntry {
        mib: &[CTL_KERN, KERN_VERSION],
        name: "kern.version",
        path: b"kern/version",
        kind: ValueKind::Text,
    },
    MibEntry {
        mib: &[CTL_KERN, KERN_HOSTNAME],
        name: "kern.hostname",
        path: b"kern/hostname",
        kind: ValueKind::Text,
    },
    MibEntry {
        mib: &[CTL_KERN, KERN_HOSTID],
        name: "kern.hostid",
        path: b"kern/hostid",
        kind: ValueKind::Integer,
    },
    MibEntry {
        mib: &[CTL_KERN, KERN_DOMAINNAME],
        name: "kern.domainname",
        path: b"kern/domainname",
        kind: ValueKind::Text,
    },
    MibEntry {
        mib: &[CTL_HW, HW_MACHINE],
        name: "hw.machine",
        path: b"hw/machine",
        kind: ValueKind::Text,
    },
    MibEntry {
        mib: &[CTL_HW, HW_NCPU],
        name: "hw.ncpu",
        path: b"hw/ncpu",
        kind: ValueKind::Integer,
    },
    MibEntry {
        mib: &[CTL_HW, HW_PHYSMEM],
        name: "hw.physmem",
        path: b"hw/physmem",
        kind: ValueKind::Integer,
    },
    MibEntry {
        mib: &[CTL_HW, HW_PAGESIZE],
        name: "hw.pagesize",
        path: b"hw/pagesize",
        kind: ValueKind::Integer,
    },
];

/// Look a name up in the static table.
pub fn sysctl_name_to_mib(name: &[u8]) -> Result<&'static MibEntry, Errno> {
    MIB_TABLE
        .iter()
        .find(|entry| entry.name.as_bytes() == name)
        .ok_or(Errno::ENOENT)
}

/// Look a numeric MIB up in the static table.
pub fn sysctl_mib_to_name(mib: &[i32]) -> Result<&'static MibEntry, Errno> {
    MIB_TABLE
        .iter()
        .find(|entry| entry.mib == mib)
        .ok_or(Errno::ENOENT)
}

/// `sysctlnametomib`: write the MIB into `out` and return its length (2 or more on a future
/// extension; the static table is two levels deep today).
pub fn sysctlnametomib(name: &[u8], out: &mut [i32]) -> Result<usize, Errno> {
    let entry = sysctl_name_to_mib(name)?;
    if out.len() < entry.mib.len() {
        return Err(Errno::ENOMEM);
    }
    out.get_mut(..entry.mib.len())
        .ok_or(Errno::ENOMEM)?
        .copy_from_slice(entry.mib);
    Ok(entry.mib.len())
}

fn parse_value(entry: &MibEntry, raw: &[u8], out: &mut [u8]) -> Result<usize, Errno> {
    let trimmed = trim(raw);
    match entry.kind {
        ValueKind::Text => {
            let needed = trimmed.len();
            if out.len() < needed {
                return Err(Errno::ENOMEM);
            }
            out.get_mut(..needed)
                .ok_or(Errno::ENOMEM)?
                .copy_from_slice(trimmed);
            Ok(needed)
        }
        ValueKind::Integer => {
            let text = core::str::from_utf8(trimmed).map_err(|_| Errno::EINVAL)?;
            let value = text.parse::<i64>().map_err(|_| Errno::EINVAL)?;
            match out.len() {
                4 => {
                    let narrowed = i32::try_from(value).map_err(|_| Errno::ERANGE)?;
                    out.get_mut(..4)
                        .ok_or(Errno::ENOMEM)?
                        .copy_from_slice(&narrowed.to_ne_bytes());
                    Ok(4)
                }
                8 => {
                    out.get_mut(..8)
                        .ok_or(Errno::ENOMEM)?
                        .copy_from_slice(&value.to_ne_bytes());
                    Ok(8)
                }
                0 => Err(Errno::ENOMEM),
                _ => Err(Errno::EINVAL),
            }
        }
    }
}

fn encode_value(entry: &MibEntry, raw: &[u8]) -> Result<Vec<u8>, Errno> {
    match entry.kind {
        ValueKind::Text => {
            let mut out = trim(raw).to_vec();
            out.push(b'\n');
            Ok(out)
        }
        ValueKind::Integer => {
            if raw.len() != 4 && raw.len() != 8 {
                return Err(Errno::EINVAL);
            }
            let value = if raw.len() == 4 {
                let mut bytes = [0u8; 4];
                bytes.copy_from_slice(raw);
                i64::from(i32::from_ne_bytes(bytes))
            } else {
                let mut bytes = [0u8; 8];
                bytes.copy_from_slice(raw);
                i64::from_ne_bytes(bytes)
            };
            let mut out = alloc::format!("{value}\n").into_bytes();
            out.shrink_to_fit();
            Ok(out)
        }
    }
}

fn trim(value: &[u8]) -> &[u8] {
    let start = value
        .iter()
        .position(|&byte| !byte.is_ascii_whitespace())
        .unwrap_or(value.len());
    let end = value
        .iter()
        .rposition(|&byte| !byte.is_ascii_whitespace())
        .map_or(start, |index| index.saturating_add(1));
    value.get(start..end).unwrap_or(&[])
}

/// `sysctlbyname`; `new_value` writes through `sys:`.
pub fn sysctlbyname<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    name: &[u8],
    old: Option<&mut [u8]>,
    new_value: Option<&[u8]>,
) -> Result<usize, Errno> {
    let entry = sysctl_name_to_mib(name)?;
    sysctl_entry(platform, entry, old, new_value)
}

/// `sysctl` by number.
pub fn sysctl<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    mib: &[i32],
    old: Option<&mut [u8]>,
    new_value: Option<&[u8]>,
) -> Result<usize, Errno> {
    let entry = sysctl_mib_to_name(mib)?;
    sysctl_entry(platform, entry, old, new_value)
}

fn sysctl_entry<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    entry: &MibEntry,
    old: Option<&mut [u8]>,
    new_value: Option<&[u8]>,
) -> Result<usize, Errno> {
    if let Some(value) = new_value {
        let encoded = encode_value(entry, value)?;
        platform.write_sys(entry.path, &encoded)?;
    }
    match old {
        None => Ok(0),
        Some(out) => {
            if out.is_empty() {
                // Size query: return the value length plus one for a NUL where applicable.
                let raw = platform.read_sys(entry.path)?;
                let length = match entry.kind {
                    ValueKind::Text => trim(&raw).len().saturating_add(1),
                    ValueKind::Integer => 8,
                };
                return Ok(length);
            }
            let raw = platform.read_sys(entry.path)?;
            parse_value(entry, &raw, out)
        }
    }
}

#[cfg(test)]
#[path = "sysctl_ut.rs"]
mod tests;
