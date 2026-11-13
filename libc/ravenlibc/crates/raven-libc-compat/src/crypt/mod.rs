// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crypt()` (donor-map 15 T3, 02 X52, delta D-7; ticket 5.C).
//!
//! Formats, exactly: `$1$` (MD5-crypt), `$2a$`/`$2b$` (bcrypt), `$5$` (SHA-256-crypt),
//! `$6$` (SHA-512-crypt). Everything else — DES (`_`, 13-character salts), `$2x$`, `$2y$`,
//! `$3$`, an empty salt, an unknown `$...` — gives `Err(EINVAL)`. This is the FreeBSD dispatch
//! with `HAS_DES` and the nthash branch removed; the dispatch order is
//! md5, blf, sha256, sha512.
//!
//! The digest + format donors (the FreeBSD `lib/libcrypt` checkout is absent): md5crypt from
//! `musl:src/crypt/crypt_md5.c` (MIT + the PHK BSD-2 block), sha256crypt/sha512crypt from
//! `musl:src/crypt/{crypt_sha256,crypt_sha512}.c` (MIT via `musl/COPYRIGHT`), bcrypt and
//! Blowfish from `netbsd:lib/libcrypt/{bcrypt,blowfish}.c` (BSD-3). `RD-new` in the report.

pub mod bcrypt;
pub mod blowfish;
pub mod md5;
pub mod sha256;
pub mod sha512;

use raven_libc_abi::errno::Errno;

/// The C `struct crypt_data` shape (`int initialized; char __buf[256]`).
#[repr(C)]
pub struct CryptData {
    /// `initialized`.
    pub initialized: i32,
    /// `__buf`.
    pub buf: [u8; 256],
}

const _: () = assert!(core::mem::size_of::<CryptData>() == 260);
const _: () = assert!(core::mem::offset_of!(CryptData, initialized) == 0);
const _: () = assert!(core::mem::offset_of!(CryptData, buf) == 4);

/// The largest hash the four formats produce (`$6$rounds=999999999$` + 16 + `$` + 86 = 119).
pub const CRYPT_HASH_MAX: usize = 119;

/// `crypt_r`: hash `passwd` with `salt` into `out` (NUL-terminated).
///
/// Returns the NUL-less length. Unknown or unsupported salts return `EINVAL`; a destination
/// shorter than the hash returns `ERANGE` (the shell passes `struct crypt_data.__buf`, 256).
#[allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // the destination is a fixed 256-byte buffer
pub fn crypt_r(passwd: &[u8], salt: &[u8], out: &mut [u8; 256]) -> Result<usize, Errno> {
    if salt.is_empty() {
        return Err(Errno(raven_libc_abi::errno::EINVAL as u16));
    }
    let hash: Option<alloc::vec::Vec<u8>> = if salt.starts_with(b"$1$") {
        md5::md5crypt(passwd, salt)
    } else if salt.starts_with(b"$2") {
        bcrypt::bcrypt(passwd, salt)
    } else if salt.starts_with(b"$5$") {
        sha256::sha256crypt(passwd, salt)
    } else if salt.starts_with(b"$6$") {
        sha512::sha512crypt(passwd, salt)
    } else {
        // DES, `_...`, `$2x$`/`$2y$` handled above (bcrypt returns None), `$3$`, anything else.
        None
    };
    let hash = hash.ok_or(Errno(raven_libc_abi::errno::EINVAL as u16))?;
    if hash.len() >= out.len() {
        return Err(Errno(raven_libc_abi::errno::ERANGE as u16));
    }
    out[..hash.len()].copy_from_slice(&hash);
    out[hash.len()] = 0;
    Ok(hash.len())
}

pub use bcrypt::bcrypt_gensalt;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
