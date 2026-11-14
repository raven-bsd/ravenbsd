// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The BSD `sysctl` API over `sys:` (`B02`, `11` 3.11, `15` T4).
//!
//! There is no `sysctl` syscall and no dynamic MIB tree: [`TABLE`] is the complete, static list
//! of names and numbers common to NetBSD, FreeBSD and OpenBSD. Anything else is `ENOENT`.
//! Strings are returned with their NUL and their length; integers accept a 4- or 8-byte
//! buffer; a short buffer is `ENOMEM` with the needed length in `*oldlen`. Only
//! `kern.hostname` is writable. `hw.ncpu` is 1 until `PG-21` (O-15-2) and `hw.physmem` is
//! `ENOENT` before it.

use alloc::vec;
use alloc::vec::Vec;

use crate::seams::{Errno, SysInfoSrc};

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
/// `KERN_DOMAINNAME`.
pub const KERN_DOMAINNAME: i32 = 22;
/// `HW_MACHINE`.
pub const HW_MACHINE: i32 = 1;
/// `HW_MODEL`.
pub const HW_MODEL: i32 = 2;
/// `HW_NCPU`.
pub const HW_NCPU: i32 = 3;
/// `HW_PHYSMEM`.
pub const HW_PHYSMEM: i32 = 5;
/// `HW_PAGESIZE`.
pub const HW_PAGESIZE: i32 = 7;

/// The private namespace for names that have no common 4.4BSD number (PROPOSED).
pub const CTL_RAVEN: i32 = 0x7a00;
/// `kern.argmax` in the private namespace.
pub const RAVEN_ARGMAX: i32 = 1;
/// `hw.model` in the private namespace.
pub const RAVEN_MODEL: i32 = 2;

/// The value kind of one row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueKind {
    /// Text including its NUL.
    Text,
    /// A 4- or 8-byte signed integer.
    Int,
    /// A 4- or 8-byte unsigned long.
    ULong,
}

/// Where a row's value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `uname().sysname`.
    Sysname,
    /// `uname().release`.
    Release,
    /// `uname().version`.
    Version,
    /// `uname().machine`.
    Machine,
    /// `hostname()` / `set_hostname()`.
    Hostname,
    /// Constant zero.
    HostId,
    /// Empty string.
    DomainName,
    /// `ARG_MAX` of `limits.h`.
    ArgMax,
    /// `"RavenBSD " + machine`.
    Model,
    /// `SYSINFO.cpus_online`, 1 before `PG-21`.
    Ncpu,
    /// `SYSINFO.mem_total_pages * page size`, `ENOENT` before `PG-21`.
    Physmem,
    /// `sysconf(_SC_PAGESIZE)`.
    Pagesize,
}

/// One row of the static table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    /// BSD name.
    pub name: &'static str,
    /// Numeric MIB.
    pub mib: &'static [i32],
    /// Value kind.
    pub kind: ValueKind,
    /// Whether `new` is accepted.
    pub writable: bool,
    /// Value source.
    pub source: Source,
}

/// The complete table of `11` 3.11 / `15` T4.
pub const TABLE: &[Row] = &[
    Row {
        name: "kern.ostype",
        mib: &[CTL_KERN, KERN_OSTYPE],
        kind: ValueKind::Text,
        writable: false,
        source: Source::Sysname,
    },
    Row {
        name: "kern.osrelease",
        mib: &[CTL_KERN, KERN_OSRELEASE],
        kind: ValueKind::Text,
        writable: false,
        source: Source::Release,
    },
    Row {
        name: "kern.version",
        mib: &[CTL_KERN, KERN_VERSION],
        kind: ValueKind::Text,
        writable: false,
        source: Source::Version,
    },
    Row {
        name: "kern.hostname",
        mib: &[CTL_KERN, KERN_HOSTNAME],
        kind: ValueKind::Text,
        writable: true,
        source: Source::Hostname,
    },
    Row {
        name: "kern.hostid",
        mib: &[CTL_KERN, KERN_HOSTID],
        kind: ValueKind::ULong,
        writable: false,
        source: Source::HostId,
    },
    Row {
        name: "kern.domainname",
        mib: &[CTL_KERN, KERN_DOMAINNAME],
        kind: ValueKind::Text,
        writable: false,
        source: Source::DomainName,
    },
    Row {
        name: "kern.argmax",
        mib: &[CTL_RAVEN, RAVEN_ARGMAX],
        kind: ValueKind::Int,
        writable: false,
        source: Source::ArgMax,
    },
    Row {
        name: "hw.machine",
        mib: &[CTL_HW, HW_MACHINE],
        kind: ValueKind::Text,
        writable: false,
        source: Source::Machine,
    },
    Row {
        name: "hw.model",
        mib: &[CTL_RAVEN, RAVEN_MODEL],
        kind: ValueKind::Text,
        writable: false,
        source: Source::Model,
    },
    Row {
        name: "hw.ncpu",
        mib: &[CTL_HW, HW_NCPU],
        kind: ValueKind::Int,
        writable: false,
        source: Source::Ncpu,
    },
    Row {
        name: "hw.physmem",
        mib: &[CTL_HW, HW_PHYSMEM],
        kind: ValueKind::ULong,
        writable: false,
        source: Source::Physmem,
    },
    Row {
        name: "hw.pagesize",
        mib: &[CTL_HW, HW_PAGESIZE],
        kind: ValueKind::Int,
        writable: false,
        source: Source::Pagesize,
    },
];

/// Find a row by name.
#[must_use]
pub fn by_name(name: &[u8]) -> Option<&'static Row> {
    TABLE.iter().find(|row| row.name.as_bytes() == name)
}

/// Find a row by MIB.
#[must_use]
pub fn by_mib(mib: &[i32]) -> Option<&'static Row> {
    TABLE.iter().find(|row| row.mib == mib)
}

fn field_bytes(field: &[core::ffi::c_char]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for byte in field {
        if *byte == 0 {
            break;
        }
        bytes.push(*byte as u8);
    }
    bytes
}

fn with_nul(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.push(0);
    bytes
}

/// One materialised value.
enum Value {
    Text(Vec<u8>),
    Int(i64),
    ULong(u64),
}

const PAGE_SIZE: u64 = 4096;
const ARG_MAX: i64 = 131_072;

fn value_of<S: SysInfoSrc>(source: &mut S, row: &Row) -> Result<Value, Errno> {
    match row.source {
        Source::Sysname => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.sysname)))),
        Source::Release => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.release)))),
        Source::Version => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.version)))),
        Source::Machine => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.machine)))),
        Source::Hostname => Ok(Value::Text(with_nul(source.hostname()?))),
        Source::HostId => Ok(Value::ULong(0)),
        Source::DomainName => Ok(Value::Text(vec![0])),
        Source::ArgMax => Ok(Value::Int(ARG_MAX)),
        Source::Model => {
            let mut value = b"RavenBSD ".to_vec();
            value.extend_from_slice(&field_bytes(&source.uname()?.machine));
            Ok(Value::Text(with_nul(value)))
        }
        Source::Ncpu => match source.sysinfo() {
            Ok(info) => Ok(Value::Int(i64::from(info.cpus_online.max(1)))),
            Err(_) => Ok(Value::Int(1)),
        },
        Source::Physmem => match source.sysinfo() {
            Ok(info) => Ok(Value::ULong(info.mem_total_pages.saturating_mul(PAGE_SIZE))),
            Err(_) => Err(Errno::ENOENT),
        },
        Source::Pagesize => Ok(Value::Int(4096)),
    }
}

fn serialized_len(value: &Value) -> usize {
    match value {
        Value::Text(text) => text.len(),
        Value::Int(_) => 4,
        Value::ULong(_) => 8,
    }
}

fn store(value: &Value, old: Option<&mut [u8]>, oldlen: &mut usize) -> Result<(), Errno> {
    let needed = serialized_len(value);
    let Some(target) = old else {
        *oldlen = needed;
        return Ok(());
    };
    let capacity = (*oldlen).min(target.len());
    if capacity < needed {
        // Integers may be copied in the other width when the caller asked for it.
        let alternative = match value {
            Value::Int(_) => Some(4usize),
            Value::ULong(_) => Some(4usize),
            Value::Text(_) => None,
        };
        if let Some(width) = alternative
            && capacity >= width
        {
            let bytes = match value {
                Value::Int(number) => i32::try_from(*number).unwrap_or(0).to_ne_bytes().to_vec(),
                Value::ULong(number) => u32::try_from(*number).unwrap_or(0).to_ne_bytes().to_vec(),
                Value::Text(_) => Vec::new(),
            };
            if let Some(slot) = target.get_mut(..width) {
                slot.copy_from_slice(&bytes);
            }
            *oldlen = width;
            return Ok(());
        }
        *oldlen = needed;
        return Err(Errno::ENOMEM);
    }
    match value {
        Value::Text(text) => {
            if let Some(slot) = target.get_mut(..text.len()) {
                slot.copy_from_slice(text);
            }
            *oldlen = text.len();
        }
        Value::Int(number) => {
            let width = if capacity >= 8 { 8 } else { 4 };
            let bytes = number.to_ne_bytes();
            if let Some(source) = bytes.get(..width)
                && let Some(slot) = target.get_mut(..width)
            {
                slot.copy_from_slice(source);
            }
            *oldlen = width;
        }
        Value::ULong(number) => {
            let width = if capacity >= 8 { 8 } else { 4 };
            let bytes = number.to_ne_bytes();
            if let Some(source) = bytes.get(..width)
                && let Some(slot) = target.get_mut(..width)
            {
                slot.copy_from_slice(source);
            }
            *oldlen = width;
        }
    }
    Ok(())
}

fn write_value<S: SysInfoSrc>(source: &mut S, row: &Row, new: Option<&[u8]>) -> Result<(), Errno> {
    let Some(data) = new else {
        return Ok(());
    };
    if !row.writable {
        return Err(Errno::EPERM);
    }
    if row.source != Source::Hostname {
        return Err(Errno::EPERM);
    }
    if data.len() > 64 {
        return Err(Errno::EINVAL);
    }
    source.set_hostname(data)
}

fn resolve(mib: &[i32], name: Option<&[u8]>) -> Result<&'static Row, Errno> {
    let row = match name {
        Some(value) => by_name(value),
        None => by_mib(mib),
    };
    row.ok_or(Errno::ENOENT)
}

/// `sysctl(mib, namelen, old, oldlen, new, newlen)`.
pub fn sysctl<S: SysInfoSrc>(
    source: &mut S,
    mib: &[i32],
    old: Option<&mut [u8]>,
    oldlen: &mut usize,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    let row = resolve(mib, None)?;
    write_value(source, row, new)?;
    if old.is_none() && new.is_some() {
        return Ok(());
    }
    let value = value_of(source, row)?;
    store(&value, old, oldlen)
}

/// `sysctlbyname(name, old, oldlen, new, newlen)`.
pub fn sysctlbyname<S: SysInfoSrc>(
    source: &mut S,
    name: &[u8],
    old: Option<&mut [u8]>,
    oldlen: &mut usize,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    let row = resolve(&[], Some(name))?;
    write_value(source, row, new)?;
    if old.is_none() && new.is_some() {
        return Ok(());
    }
    let value = value_of(source, row)?;
    store(&value, old, oldlen)
}

/// `sysctlnametomib(name, mib, miblen)`.
pub fn sysctlnametomib(name: &[u8], mib: &mut [i32], miblen: &mut usize) -> Result<(), Errno> {
    let row = resolve(&[], Some(name))?;
    let needed = row.mib.len();
    if mib.len() < needed {
        *miblen = needed;
        return Err(Errno::ENOMEM);
    }
    if let (Some(target), Some(source)) = (mib.get_mut(..needed), row.mib.get(..needed)) {
        target.copy_from_slice(source);
    }
    *miblen = needed;
    Ok(())
}

/// `sysctlgetmibinfo(name, mib, miblen, ...)`.
///
/// The installed `sys/sysctl.h` of this branch does not declare the full NetBSD signature (C38
/// gap); the shell exports this name with the `sysctlnametomib` shape and reports the gap.
pub fn sysctlgetmibinfo(name: &[u8], mib: &mut [i32], miblen: &mut usize) -> Result<(), Errno> {
    sysctlnametomib(name, mib, miblen)
}

#[cfg(test)]
#[path = "sysctl_ut.rs"]
mod tests;
