// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The BSD `sysctl` API over `sys:` (`B02`, `11` 3.11, `15` T4).
//!
//! There is no `sysctl` syscall and no dynamic MIB tree: [`TABLE`] is the complete, static list
//! of names and numbers from the pinned FreeBSD contract. Anything else is `ENOENT`.
//! Strings are returned with their NUL and their length; `CTLTYPE_INT` is always 4 bytes and
//! `CTLTYPE_ULONG` always 8; a short buffer is `ENOMEM` with the number of bytes actually copied
//! in `*oldlen`, the FreeBSD partial-copy rule. Only `kern.hostname` is writable. A dynamic
//! value without an authoritative provider is `ENOENT`, never a fabricated constant.

use alloc::vec;
use alloc::vec::Vec;

use crate::seams::{Errno, SysInfoSrc};

/// `CTL_KERN`.
pub const CTL_KERN: i32 = 1;
/// `CTL_HW`.
pub const CTL_HW: i32 = 6;
/// `CTL_MAXNAME` (FreeBSD `sys/sys/sysctl.h`).
pub const CTL_MAXNAME: usize = 24;
/// `KERN_OSTYPE`.
pub const KERN_OSTYPE: i32 = 1;
/// `KERN_OSRELEASE`.
pub const KERN_OSRELEASE: i32 = 2;
/// `KERN_VERSION`.
pub const KERN_VERSION: i32 = 4;
/// `KERN_ARGMAX`.
pub const KERN_ARGMAX: i32 = 8;
/// `KERN_HOSTNAME`.
pub const KERN_HOSTNAME: i32 = 10;
/// `KERN_HOSTID`.
pub const KERN_HOSTID: i32 = 11;
/// `KERN_NISDOMAINNAME`.
pub const KERN_NISDOMAINNAME: i32 = 22;
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

/// `MAXHOSTNAMELEN` used by the FreeBSD string handler.
const MAXHOSTNAMELEN: usize = 256;

/// The `kern___sysctlbyname` name bound; Raven's `MAXPATHLEN` is `PATH_MAX` (doc 14 3.A.6).
const MAX_SYSCTL_NAME: usize = raven_libc_abi::limits::PATH_MAX as usize;

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
    /// No authoritative CPU-model provider; reading is `ENOENT`.
    Model,
    /// `SYSINFO.cpus_online`; `ENOENT` without `SYSINFO`.
    Ncpu,
    /// `SYSINFO.mem_total_pages * page size`, `ENOENT` without `SYSINFO`.
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
        mib: &[CTL_KERN, KERN_NISDOMAINNAME],
        kind: ValueKind::Text,
        writable: false,
        source: Source::DomainName,
    },
    Row {
        name: "kern.argmax",
        mib: &[CTL_KERN, KERN_ARGMAX],
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
        mib: &[CTL_HW, HW_MODEL],
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

fn value_of<S: SysInfoSrc>(source: &mut S, row: &Row) -> Result<Value, Errno> {
    match row.source {
        Source::Sysname => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.sysname)))),
        Source::Release => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.release)))),
        Source::Version => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.version)))),
        Source::Machine => Ok(Value::Text(with_nul(field_bytes(&source.uname()?.machine)))),
        Source::Hostname => Ok(Value::Text(with_nul(source.hostname()?))),
        Source::HostId => Ok(Value::ULong(0)),
        Source::DomainName => Ok(Value::Text(vec![0])),
        Source::ArgMax => Ok(Value::Int(i64::from(raven_libc_abi::limits::ARG_MAX))),
        // No authoritative CPU-model provider exists; a synthesised string would be fabricated.
        Source::Model => Err(Errno::ENOENT),
        // `SYSINFO` is absent before `PG-21`; the name is declared but has no value yet.
        Source::Ncpu => match source.sysinfo() {
            Ok(info) => Ok(Value::Int(i64::from(info.cpus_online))),
            Err(Errno::ENOSYS) => Err(Errno::ENOENT),
            Err(error) => Err(error),
        },
        Source::Physmem => match source.sysinfo() {
            Ok(info) => {
                let page_size = raven_libc_sys::pages::DEFAULT_PAGE_SIZE as u64;
                Ok(Value::ULong(info.mem_total_pages.saturating_mul(page_size)))
            }
            Err(Errno::ENOSYS) => Err(Errno::ENOENT),
            Err(error) => Err(error),
        },
        Source::Pagesize => Ok(Value::Int(raven_libc_sys::pages::DEFAULT_PAGE_SIZE as i64)),
    }
}

fn serialized_len(value: &Value) -> usize {
    match value {
        Value::Text(text) => text.len(),
        Value::Int(_) => 4,
        Value::ULong(_) => 8,
    }
}

fn store(value: &Value, target: &mut [u8], oldlen: &mut usize) -> Result<(), Errno> {
    let needed = serialized_len(value);
    let capacity = (*oldlen).min(target.len());
    let copied = capacity.min(needed);
    match value {
        Value::Text(text) => {
            if let (Some(slot), Some(source)) = (target.get_mut(..copied), text.get(..copied)) {
                slot.copy_from_slice(source);
            }
        }
        Value::Int(number) => {
            let narrowed = i32::try_from(*number).map_err(|_| Errno::ERANGE)?;
            let bytes = narrowed.to_ne_bytes();
            if let (Some(slot), Some(source)) = (target.get_mut(..copied), bytes.get(..copied)) {
                slot.copy_from_slice(source);
            }
        }
        Value::ULong(number) => {
            let bytes = number.to_ne_bytes();
            if let (Some(slot), Some(source)) = (target.get_mut(..copied), bytes.get(..copied)) {
                slot.copy_from_slice(source);
            }
        }
    }
    *oldlen = copied;
    if copied < needed {
        return Err(Errno::ENOMEM);
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
    if data.len() >= MAXHOSTNAMELEN {
        return Err(Errno::EINVAL);
    }
    source.set_hostname(data)
}

/// The FreeBSD handler order: copy the old value out first, then apply the new value. A short
/// old buffer is `ENOMEM` and leaves the new value unapplied.
fn apply<S: SysInfoSrc>(
    source: &mut S,
    row: &'static Row,
    old: Option<&mut [u8]>,
    oldlen: &mut usize,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    if new.is_some() && !row.writable {
        return Err(Errno::EPERM);
    }
    let value = value_of(source, row)?;
    match old {
        Some(target) => store(&value, target, oldlen)?,
        None => *oldlen = serialized_len(&value),
    }
    write_value(source, row, new)
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
    if mib.len() < 2 || mib.len() > CTL_MAXNAME {
        return Err(Errno::EINVAL);
    }
    let row = resolve(mib, None)?;
    apply(source, row, old, oldlen, new)
}

/// `sysctlbyname(name, old, oldlen, new, newlen)`.
pub fn sysctlbyname<S: SysInfoSrc>(
    source: &mut S,
    name: &[u8],
    old: Option<&mut [u8]>,
    oldlen: &mut usize,
    new: Option<&[u8]>,
) -> Result<(), Errno> {
    if name.is_empty() || name.len() > MAX_SYSCTL_NAME {
        return Err(Errno::EINVAL);
    }
    let row = resolve(&[], Some(name))?;
    apply(source, row, old, oldlen, new)
}

/// `sysctlnametomib(name, mib, miblen)`.
pub fn sysctlnametomib(name: &[u8], mib: &mut [i32], miblen: &mut usize) -> Result<(), Errno> {
    if name.is_empty() || name.len() > MAX_SYSCTL_NAME {
        return Err(Errno::EINVAL);
    }
    let row = resolve(&[], Some(name))?;
    let needed = row.mib.len();
    if mib.len() < needed {
        // FreeBSD's libc wrapper passes ENOMEM through from the NAME2OID sysctl and keeps the
        // caller's element count; it does not report the needed count.
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
