// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `elf_aux_info` (`B08`, `02` B08).
//!
//! The FreeBSD-style wrapper over the auxv reader: `0` on success, `EINVAL` for an unknown key
//! or a buffer shorter than the key's width, `ENOENT` for a key the process does not carry.
//! The C prototype is `int elf_aux_info(int aux, void *buf, int buflen)`; the shell builds the
//! slice. The installed `sys/auxv.h` of this branch declares only `getauxval`, so the shell
//! export is a documented C38 gap (see the C66 report).

use crate::seams::Errno;

/// `AT_PHDR`.
pub const AT_PHDR: i32 = 3;
/// `AT_PHENT`.
pub const AT_PHENT: i32 = 4;
/// `AT_PHNUM`.
pub const AT_PHNUM: i32 = 5;
/// `AT_PAGESZ`.
pub const AT_PAGESZ: i32 = 6;
/// `AT_BASE`.
pub const AT_BASE: i32 = 7;
/// `AT_ENTRY`.
pub const AT_ENTRY: i32 = 9;
/// `AT_HWCAP`.
pub const AT_HWCAP: i32 = 16;
/// `AT_CLKTCK`.
pub const AT_CLKTCK: i32 = 17;
/// `AT_SECURE`.
pub const AT_SECURE: i32 = 23;
/// `AT_RANDOM`.
pub const AT_RANDOM: i32 = 25;
/// `AT_HWCAP2`.
pub const AT_HWCAP2: i32 = 26;
/// `AT_EXECFN`.
pub const AT_EXECFN: i32 = 31;

/// The byte width of one supported key, or `None` for an unknown key.
#[must_use]
pub const fn width(aux: i32) -> Option<usize> {
    match aux {
        AT_PHDR | AT_PAGESZ | AT_BASE | AT_ENTRY | AT_HWCAP | AT_RANDOM | AT_HWCAP2 | AT_EXECFN => {
            Some(8)
        }
        AT_PHENT | AT_PHNUM | AT_CLKTCK | AT_SECURE => Some(4),
        _ => None,
    }
}

/// `elf_aux_info(aux, buf, len)`.
pub fn elf_aux_info(
    aux: i32,
    buf: &mut [u8],
    lookup: &dyn Fn(u64) -> Option<u64>,
) -> Result<(), Errno> {
    let Some(size) = width(aux) else {
        return Err(Errno::EINVAL);
    };
    if buf.len() < size {
        return Err(Errno::EINVAL);
    }
    let value = lookup(u64::try_from(aux).unwrap_or(0)).ok_or(Errno::ENOENT)?;
    let bytes = value.to_ne_bytes();
    let Some(target) = buf.get_mut(..size) else {
        return Err(Errno::EINVAL);
    };
    let Some(source) = bytes.get(..size) else {
        return Err(Errno::EINVAL);
    };
    target.copy_from_slice(source);
    Ok(())
}

#[cfg(test)]
#[path = "aux_ut.rs"]
mod tests;
