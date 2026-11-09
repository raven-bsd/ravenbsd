// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! System identification record returned by `SYS_UNAME` (shape of POSIX `utsname`, 5 fields).

/// Length of every `Uname` field in bytes, including the NUL terminator.
pub const UNAME_FIELD_LEN: usize = 65;

/// Operating system name.
pub const SYSNAME: &str = "RavenBSD";
/// Release number (`uname -r`).
pub const RELEASE: &str = "0.1";

/// The `uname` record. Every field is a NUL-terminated byte string, zero padded.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Uname {
    /// `uname -s`.
    pub sysname: [u8; UNAME_FIELD_LEN],
    /// `uname -n`.
    pub nodename: [u8; UNAME_FIELD_LEN],
    /// `uname -r`.
    pub release: [u8; UNAME_FIELD_LEN],
    /// `uname -v`.
    pub version: [u8; UNAME_FIELD_LEN],
    /// `uname -m`.
    pub machine: [u8; UNAME_FIELD_LEN],
}

const _: () = assert!(core::mem::size_of::<Uname>() == 5 * UNAME_FIELD_LEN);
const _: () = assert!(core::mem::align_of::<Uname>() == 1);

impl Uname {
    /// An all-zero record.
    pub const fn zeroed() -> Uname {
        Uname {
            sysname: [0; UNAME_FIELD_LEN],
            nodename: [0; UNAME_FIELD_LEN],
            release: [0; UNAME_FIELD_LEN],
            version: [0; UNAME_FIELD_LEN],
            machine: [0; UNAME_FIELD_LEN],
        }
    }

    /// Copy `s` into `field`, truncating to 64 bytes and always leaving a NUL terminator.
    pub fn set_field(field: &mut [u8; UNAME_FIELD_LEN], s: &str) {
        field.fill(0);
        let n = s.len().min(UNAME_FIELD_LEN - 1);
        for (dst, src) in field.iter_mut().zip(s.as_bytes().iter().take(n)) {
            *dst = *src;
        }
    }

    /// Read a field back as `&str` up to the first NUL (lossy-free: returns `None` if not UTF-8).
    pub fn field_str(field: &[u8; UNAME_FIELD_LEN]) -> Option<&str> {
        let end = field
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(UNAME_FIELD_LEN);
        core::str::from_utf8(field.get(..end)?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        let mut u = Uname::zeroed();
        Uname::set_field(&mut u.sysname, SYSNAME);
        assert_eq!(Uname::field_str(&u.sysname), Some("RavenBSD"));
    }

    #[test]
    fn truncates_and_terminates() {
        let mut f = [0xAAu8; UNAME_FIELD_LEN];
        let long: String = "x".repeat(200);
        Uname::set_field(&mut f, &long);
        assert_eq!(f[UNAME_FIELD_LEN - 1], 0);
        assert_eq!(
            Uname::field_str(&f).map(str::len),
            Some(UNAME_FIELD_LEN - 1)
        );
    }

    #[test]
    fn empty_field_is_empty_string() {
        let u = Uname::zeroed();
        assert_eq!(Uname::field_str(&u.machine), Some(""));
    }
}
