// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Safe process-credential wrappers over the P2a syscall seam.
//!
//! The raw calls live in `raven-rt` because the tool crates are `#![forbid(unsafe_code)]`. The
//! public surface is scalar ids and caller-owned slices only; the kernel validates the range and
//! never writes more than the caller supplied.

use raven_abi::errno::Errno;

/// Kernel supplementary-group limit.
pub const MAX_GROUPS: usize = 16;

/// Raw `proc_cred_info(out, count)`.
#[cfg(target_os = "none")]
const PROC_CRED_INFO: usize = 0x3C;
/// Raw `proc_setcred_ex(which, a, b, c, groups, count)`.
#[cfg(target_os = "none")]
const PROC_SETCRED_EX: usize = 0x3B;
/// Raw `proc_getgroups(out, size)`.
#[cfg(target_os = "none")]
const PROC_GETGROUPS: usize = 0x3D;

/// The six credential words returned by `PROC_CRED_INFO`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Credentials {
    /// Real user id.
    pub uid: u32,
    /// Effective user id.
    pub euid: u32,
    /// Saved user id.
    pub suid: u32,
    /// Real group id.
    pub gid: u32,
    /// Effective group id.
    pub egid: u32,
    /// Saved group id.
    pub sgid: u32,
}

impl Credentials {
    /// The effective uid.
    #[must_use]
    pub const fn euid(self) -> u32 {
        self.euid
    }

    /// The effective gid.
    #[must_use]
    pub const fn egid(self) -> u32 {
        self.egid
    }
}

#[cfg(any(target_os = "none", test))]
fn decode_credentials(words: [u64; 6]) -> Credentials {
    // The kernel stores these as u64 words but every id is a u32 value.
    let value = |index: usize| -> u32 {
        words
            .get(index)
            .copied()
            .and_then(|word| u32::try_from(word).ok())
            .unwrap_or(0)
    };
    Credentials {
        uid: value(0),
        euid: value(1),
        suid: value(2),
        gid: value(3),
        egid: value(4),
        sgid: value(5),
    }
}

/// Read the caller's real, effective and saved ids.
///
/// # Errors
/// [`Errno`] from `PROC_CRED_INFO`; `ENOSYS` on the host.
#[cfg(target_os = "none")]
pub fn credentials() -> Result<Credentials, Errno> {
    let mut words = [0u64; 6];
    // SAFETY: `words` is a live six-word array and the kernel writes exactly six u64 values.
    let result =
        unsafe { crate::sys::syscall2(PROC_CRED_INFO, words.as_mut_ptr() as usize, words.len()) };
    Errno::from_ret(result)?;
    Ok(decode_credentials(words))
}

/// Read the caller's real, effective and saved ids.
///
/// # Errors
/// [`Errno::ENOSYS`] on the host.
#[cfg(not(target_os = "none"))]
pub fn credentials() -> Result<Credentials, Errno> {
    Err(Errno::ENOSYS)
}

/// Read the caller's supplementary groups.
///
/// A zero-length `out` returns the stored count without writing. A non-empty `out` must be at
/// least as large as the stored count; the returned count is the number of entries written.
///
/// # Errors
/// [`Errno`] from `PROC_GETGROUPS`; `ENOSYS` on the host.
#[cfg(target_os = "none")]
pub fn groups(out: &mut [u32]) -> Result<usize, Errno> {
    // SAFETY: `out` is a live caller-owned slice; a zero length only queries the count.
    let result =
        unsafe { crate::sys::syscall2(PROC_GETGROUPS, out.as_mut_ptr() as usize, out.len()) };
    Errno::from_ret(result)
}

/// Read the caller's supplementary groups.
///
/// # Errors
/// [`Errno::ENOSYS`] on the host.
#[cfg(not(target_os = "none"))]
pub fn groups(_out: &mut [u32]) -> Result<usize, Errno> {
    Err(Errno::ENOSYS)
}

/// Set the caller's real, effective and saved user id.
///
/// # Errors
/// [`Errno`] from `PROC_SETCRED_EX`; `ENOSYS` on the host.
#[cfg(target_os = "none")]
pub fn setresuid(uid: u32) -> Result<(), Errno> {
    setcred(4, uid, uid, uid, &[])
}

/// Set the caller's real, effective and saved user id.
///
/// # Errors
/// [`Errno::ENOSYS`] on the host.
#[cfg(not(target_os = "none"))]
pub fn setresuid(_uid: u32) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// Set the caller's real, effective and saved group id.
///
/// # Errors
/// [`Errno`] from `PROC_SETCRED_EX`; `ENOSYS` on the host.
#[cfg(target_os = "none")]
pub fn setresgid(gid: u32) -> Result<(), Errno> {
    setcred(8, gid, gid, gid, &[])
}

/// Set the caller's real, effective and saved group id.
///
/// # Errors
/// [`Errno::ENOSYS`] on the host.
#[cfg(not(target_os = "none"))]
pub fn setresgid(_gid: u32) -> Result<(), Errno> {
    Err(Errno::ENOSYS)
}

/// Replace the caller's supplementary groups.
///
/// # Errors
/// [`Errno::EINVAL`] for more than [`MAX_GROUPS`] entries; `ENOSYS` on the host.
#[cfg(target_os = "none")]
pub fn setgroups(groups: &[u32]) -> Result<(), Errno> {
    if groups.len() > MAX_GROUPS {
        return Err(Errno::EINVAL);
    }
    setcred(9, 0, 0, 0, groups)
}

/// Replace the caller's supplementary groups.
///
/// # Errors
/// [`Errno::EINVAL`] for more than [`MAX_GROUPS`] entries; `ENOSYS` on the host.
#[cfg(not(target_os = "none"))]
pub fn setgroups(groups: &[u32]) -> Result<(), Errno> {
    if groups.len() > MAX_GROUPS {
        return Err(Errno::EINVAL);
    }
    Err(Errno::ENOSYS)
}

#[cfg(target_os = "none")]
fn setcred(kind: usize, a: u32, b: u32, c: u32, groups: &[u32]) -> Result<(), Errno> {
    // The kernel reads `a`, `b` and `c` as i32; -1 means "leave unchanged", which is not used
    // here because every caller sets all three ids to one value.
    // SAFETY: the kernel validates the credential kind, the scalar ids and the live group slice.
    let result = unsafe {
        crate::sys::syscall6(
            PROC_SETCRED_EX,
            kind,
            a as usize,
            b as usize,
            c as usize,
            groups.as_ptr() as usize,
            groups.len(),
        )
    };
    Errno::from_ret(result).map(|_| ())
}

#[cfg(test)]
#[path = "proc_ut.rs"]
mod tests;
