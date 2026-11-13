// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `gethostname`/`sethostname`.
//!
//! `gethostname` reads `sys:hostname` and falls back to `uname.nodename`; `sethostname` writes
//! `sys:hostname`. Both enforce the Raven `HOST_NAME_MAX` of 64 (H-19); the server itself still
//! accepts 63 until G-27, which is a reported mismatch and not hidden here.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;
use crate::sysinfo::uname::field_bytes;

/// Raven `HOST_NAME_MAX` (H-19).
pub const HOST_NAME_MAX: usize = 64;

/// `gethostname`: copy the nodename, truncating to `buf.len()`; a buffer smaller than the name
/// plus its NUL is `ENAMETOOLONG`.
pub fn gethostname<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    buf: &mut [u8],
) -> Result<(), Errno> {
    let name = platform.hostname_value();
    let needed = name.len().saturating_add(1);
    if buf.len() < needed {
        return Err(Errno::ENAMETOOLONG);
    }
    buf.get_mut(..name.len())
        .ok_or(Errno::ENAMETOOLONG)?
        .copy_from_slice(&name);
    if let Some(slot) = buf.get_mut(name.len()) {
        *slot = 0;
    }
    Ok(())
}

/// The hostname with the `sys:` value taking precedence over `Uname.nodename`.
pub fn hostname_from(sys_value: Option<&[u8]>, nodename: &[u8]) -> Vec<u8> {
    let value = sys_value.unwrap_or(&[]);
    let trimmed = trim(value);
    if !trimmed.is_empty() {
        return trimmed.to_vec();
    }
    trim(nodename).to_vec()
}

/// The fallback rule used by [`hostname_from`] over a raw `Uname` record.
pub fn hostname_from_uname(sys_value: Option<&[u8]>, uname: &raven_abi::uname::Uname) -> Vec<u8> {
    hostname_from(sys_value, field_bytes(&uname.nodename))
}

/// `sethostname`: over 64 bytes is `EINVAL`; the value goes to `sys:hostname`.
pub fn validate_hostname(name: &[u8]) -> Result<&[u8], Errno> {
    if name.len() > HOST_NAME_MAX {
        return Err(Errno::EINVAL);
    }
    if name.contains(&0) {
        return Err(Errno::EINVAL);
    }
    Ok(trim(name))
}

/// `sethostname`.
pub fn sethostname<B: SysBackend, L: Lock>(
    platform: &Platform<B, L>,
    name: &[u8],
) -> Result<(), Errno> {
    let name = validate_hostname(name)?;
    platform.write_sys(b"hostname", name)
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

#[cfg(test)]
#[path = "hostname_ut.rs"]
mod tests;
