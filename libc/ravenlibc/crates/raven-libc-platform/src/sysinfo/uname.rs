// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uname`.

use raven_abi::errno::Errno;
use raven_abi::uname::Uname;
use raven_libc_abi::structs::Utsname;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

fn copy_field(source: &[u8; 65], target: &mut [core::ffi::c_char; 256]) {
    let mut index = 0usize;
    while index < source.len() {
        if let (Some(value), Some(slot)) = (source.get(index), target.get_mut(index)) {
            *slot = *value as core::ffi::c_char;
        }
        index = index.saturating_add(1);
    }
}

/// A NUL-terminated `Uname` field as bytes.
pub fn field_bytes(field: &[u8; 65]) -> &[u8] {
    match field.iter().position(|&byte| byte == 0) {
        Some(end) => field.get(..end).unwrap_or(&[]),
        None => field,
    }
}

/// `uname`: one syscall, five FreeBSD `SYS_NMLN` (256-byte) fields; the tail of each
/// destination field stays zero past the kernel's 65-byte text.
pub fn uname<B: SysBackend, L: Lock>(platform: &Platform<B, L>) -> Result<Utsname, Errno> {
    let raw = platform.backend().uname()?;
    let mut out = Utsname::ZERO;
    copy_field(&raw.sysname, &mut out.sysname);
    copy_field(&raw.nodename, &mut out.nodename);
    copy_field(&raw.release, &mut out.release);
    copy_field(&raw.version, &mut out.version);
    copy_field(&raw.machine, &mut out.machine);
    Ok(out)
}

/// The `Uname` record as the ABI model.
pub fn raw_uname<B: SysBackend, L: Lock>(platform: &Platform<B, L>) -> Result<Uname, Errno> {
    platform.backend().uname()
}

#[cfg(test)]
#[path = "uname_ut.rs"]
mod tests;
