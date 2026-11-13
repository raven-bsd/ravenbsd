// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared case type and observations for the `str-*` oracle families.
//!
//! One input shape covers the string and memory families: two byte strings (`a`, `b`), a length
//! `n`, a search byte `c`, a fill byte and a destination size `dsize`. The observation compares
//! the return value as an offset, every destination byte (guard bytes included) and, for the
//! tokenizers, the token offsets. This module is compiled with the host harness; it is not part
//! of the shipped crate.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)]
#![allow(dead_code)] // the methods are feature-gated per family
#![allow(missing_docs)] // the harness case type is not a shipped API

use serde::Deserialize;

use crate::corpus::{parse_bytes, render_bytes};
use crate::diff::Case;
use crate::rng::Rng;

/// Whole observation of one case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Obs {
    /// Return value; pointers are offsets from the input buffer, `-1` is NULL.
    pub ret: i64,
    /// Every output byte, guard bytes included.
    pub buf: Vec<u8>,
    /// Token offsets for `strtok_r`/`strsep`, empty otherwise.
    pub tokens: Vec<(i64, i64)>,
}

impl Obs {
    fn value(ret: i64) -> Self {
        Self {
            ret,
            buf: Vec::new(),
            tokens: Vec::new(),
        }
    }
}

/// One string/memory case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrCase {
    /// First byte string (haystack, `s1`, destination string, or source).
    pub a: Vec<u8>,
    /// Second byte string (needle, `s2`, set, or source).
    pub b: Vec<u8>,
    /// Length/bound argument.
    pub n: usize,
    /// Search byte (low byte is used, as in the C `char` parameter).
    pub c: i32,
    /// Destination fill byte.
    pub fill: u8,
    /// Destination size for the `strlcpy`/`strlcat`/`strncpy` family.
    pub dsize: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    #[serde(default)]
    a: Option<String>,
    #[serde(default)]
    a_hex: Option<String>,
    #[serde(default)]
    b: Option<String>,
    #[serde(default)]
    b_hex: Option<String>,
    #[serde(default)]
    n: usize,
    #[serde(default)]
    c: i32,
    #[serde(default)]
    fill: u8,
    #[serde(default)]
    dsize: usize,
}

impl TryFrom<Raw> for StrCase {
    type Error = String;

    fn try_from(raw: Raw) -> Result<Self, Self::Error> {
        Ok(Self {
            a: optional_bytes(raw.a, raw.a_hex, "str case a")?,
            b: optional_bytes(raw.b, raw.b_hex, "str case b")?,
            n: raw.n,
            c: raw.c,
            fill: raw.fill,
            dsize: raw.dsize,
        })
    }
}

impl<'de> Deserialize<'de> for StrCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Raw::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl StrCase {
    /// A random case: two short strings over a high-byte alphabet, an embedded NUL now and then.
    #[must_use]
    pub fn random(rng: &mut Rng) -> Self {
        let mut make = |limit: usize| {
            let length = (rng.below(24) as usize).min(limit);
            let mut bytes = Vec::with_capacity(length);
            for _ in 0..length {
                bytes.push(if rng.below(8) == 0 {
                    0
                } else {
                    match rng.below(4) {
                        0 => b'a',
                        1 => b'b',
                        2 => 0x80,
                        _ => 0xff,
                    }
                });
            }
            bytes
        };
        let a = make(24);
        let b = make(16);
        Self {
            a,
            b,
            n: rng.below(28) as usize,
            c: if rng.below(8) == 0 {
                rng.pick(&[-1, 0x161])
            } else {
                rng.below(256) as i32
            },
            fill: rng.next_u64() as u8,
            dsize: rng.below(28) as usize,
        }
    }

    /// All pairs of strings over `{a,b,0}` of length <= 4.
    #[must_use]
    pub fn exhaustive() -> Box<dyn Iterator<Item = Self>> {
        let domain = strings(&[b'a', b'b', 0], 4);
        let mut cases = Vec::new();
        for a in &domain {
            for b in &domain {
                cases.push(Self {
                    a: a.clone(),
                    b: b.clone(),
                    n: (a.len() + b.len()) % 8,
                    c: if a.is_empty() { 0 } else { i32::from(a[0]) },
                    fill: 0xA5,
                    dsize: a.len() % 8,
                });
            }
        }
        Box::new(cases.into_iter())
    }

    /// C view of `a` as a NUL-terminated string (bytes before the first NUL).
    fn a_str(&self) -> Vec<u8> {
        let end = self
            .a
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.a.len());
        let mut value = self.a[..end].to_vec();
        value.push(0);
        value
    }

    /// C view of `b` as a NUL-terminated string.
    fn b_str(&self) -> Vec<u8> {
        let end = self
            .b
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.b.len());
        let mut value = self.b[..end].to_vec();
        value.push(0);
        value
    }

    fn destination(&self, size: usize) -> Vec<u8> {
        vec![self.fill; size + 4]
    }

    fn pointer_offset(pointer: *const u8, base: *const u8) -> i64 {
        if pointer.is_null() {
            -1
        } else {
            (pointer as usize).wrapping_sub(base as usize) as i64
        }
    }
}

fn optional_bytes(
    bytes: Option<String>,
    hex: Option<String>,
    what: &str,
) -> Result<Vec<u8>, String> {
    if bytes.is_none() && hex.is_none() {
        return Ok(Vec::new());
    }
    parse_bytes(bytes, hex, what)
}

fn strings(alphabet: &[u8], max_len: usize) -> Vec<Vec<u8>> {
    let mut all = vec![Vec::new()];
    let mut level = vec![Vec::new()];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for prefix in &level {
            for &byte in alphabet {
                let mut value = prefix.clone();
                value.push(byte);
                next.push(value);
            }
        }
        all.extend(next.iter().cloned());
        level = next;
    }
    all
}

impl Case for StrCase {
    type Obs = Obs;

    fn shrink(&self) -> Vec<Self> {
        let mut candidates = Vec::new();
        if !self.a.is_empty() {
            candidates.push(Self {
                a: self.a[..self.a.len() - 1].to_vec(),
                ..self.clone()
            });
        }
        if !self.b.is_empty() {
            candidates.push(Self {
                b: self.b[..self.b.len() - 1].to_vec(),
                ..self.clone()
            });
        }
        if self.n > 0 {
            candidates.push(Self {
                n: self.n - 1,
                ..self.clone()
            });
        }
        if self.dsize > 0 {
            candidates.push(Self {
                dsize: self.dsize - 1,
                ..self.clone()
            });
        }
        candidates
    }

    fn to_toml(&self) -> String {
        format!(
            "{}\n{}\nn = {}\nc = {}\nfill = {}\ndsize = {}",
            render_bytes(&self.a),
            render_bytes(&self.b),
            self.n,
            self.c,
            self.fill,
            self.dsize
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "ret = {}, buf_hex = \"{}\", tokens = {:?}",
            obs.ret,
            crate::corpus::hex_encode(&obs.buf),
            obs.tokens
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Subject and oracle calls. The layouts mirror each other so the comparison is whole.
// ---------------------------------------------------------------------------------------------

impl StrCase {
    // -- str-mem ------------------------------------------------------------------------------

    #[cfg(feature = "oracle-str-mem")]
    fn copy_mem<F: Fn(*mut u8, *const u8, usize) -> *mut u8>(&self, function: F) -> Obs {
        let mut dst = self.destination(self.n);
        let mut src = self.a_str();
        src.resize(self.n.max(src.len()), 0);
        let ret = function(dst.as_mut_ptr(), src.as_ptr(), self.n);
        Obs {
            ret: Self::pointer_offset(ret, dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memcpy_oracle(&self) -> Obs {
        self.copy_mem(|dst, src, n| {
            // SAFETY: `dst` is `n + 4` bytes and `src` is NUL terminated.
            unsafe { super::str_mem::oracle_memcpy(dst.cast(), src.cast(), n).cast() }
        })
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memcpy_subject(&self) -> Obs {
        self.copy_mem(|dst, src, n| {
            // SAFETY: the caller-provided buffers are valid for `n` bytes and do not overlap.
            unsafe { raven_libc_str::raw::memcpy(dst, src, n) }
        })
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memmove_oracle(&self) -> Obs {
        self.copy_mem(|dst, src, n| {
            // SAFETY: the buffers are valid for `n` bytes; overlap is allowed.
            unsafe { super::str_mem::oracle_memmove(dst.cast(), src.cast(), n).cast() }
        })
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memmove_subject(&self) -> Obs {
        self.copy_mem(|dst, src, n| {
            // SAFETY: the buffers are valid for `n` bytes; overlap is allowed.
            unsafe { raven_libc_str::raw::memmove(dst, src, n) }
        })
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memset_oracle(&self) -> Obs {
        let mut dst = self.destination(self.n);
        // SAFETY: the destination is `n + 4` bytes.
        let ret = unsafe { super::str_mem::oracle_memset(dst.as_mut_ptr().cast(), self.c, self.n) };
        Obs {
            ret: Self::pointer_offset(ret.cast(), dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memset_subject(&self) -> Obs {
        let mut dst = self.destination(self.n);
        // SAFETY: the destination is `n + 4` bytes.
        let ret = unsafe { raven_libc_str::raw::memset(dst.as_mut_ptr(), self.c, self.n) };
        Obs {
            ret: Self::pointer_offset(ret, dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memcmp_oracle(&self) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(self.n.max(a.len()), 0);
        b.resize(self.n.max(b.len()), 0);
        // SAFETY: both buffers are NUL terminated and padded to `n` readable bytes.
        let ret =
            unsafe { super::str_mem::oracle_memcmp(a.as_ptr().cast(), b.as_ptr().cast(), self.n) };
        Obs::value(i64::from(ret))
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memcmp_subject(&self) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(self.n.max(a.len()), 0);
        b.resize(self.n.max(b.len()), 0);
        // SAFETY: both buffers are padded to `n` readable bytes.
        let ret = unsafe { raven_libc_str::raw::memcmp(a.as_ptr(), b.as_ptr(), self.n) };
        Obs::value(i64::from(ret))
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memchr_oracle(&self) -> Obs {
        let mut a = self.a_str();
        a.resize(self.n.max(a.len()), 0);
        // SAFETY: `a` is padded to `n` readable bytes.
        let ret = unsafe { super::str_mem::oracle_memchr(a.as_ptr().cast(), self.c, self.n) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[cfg(feature = "oracle-str-mem")]
    #[must_use]
    pub fn memchr_subject(&self) -> Obs {
        let mut a = self.a_str();
        a.resize(self.n.max(a.len()), 0);
        // SAFETY: `a` is padded to `n` readable bytes.
        let ret = unsafe { raven_libc_str::raw::memchr(a.as_ptr(), self.c, self.n) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }
}

// ---------------------------------------------------------------------------------------------
// str-openbsd
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "oracle-str-openbsd")]
impl StrCase {
    fn compare_plain<F: Fn(*const u8, *const u8, usize) -> i32>(
        &self,
        function: F,
        n: usize,
    ) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(n.max(a.len()), 0);
        b.resize(n.max(b.len()), 0);
        let ret = function(a.as_ptr(), b.as_ptr(), n);
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn strcmp_oracle(&self) -> Obs {
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe {
            super::str_openbsd::oracle_strcmp(
                self.a_str().as_ptr().cast(),
                self.b_str().as_ptr().cast(),
            )
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn strcmp_subject(&self) -> Obs {
        // SAFETY: both strings are NUL terminated.
        let ret =
            unsafe { raven_libc_str::raw::strcmp(self.a_str().as_ptr(), self.b_str().as_ptr()) };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn strncmp_oracle(&self) -> Obs {
        self.compare_plain(
            |a, b, n| {
                // SAFETY: both buffers are padded to `n` readable bytes.
                unsafe { super::str_openbsd::oracle_strncmp(a.cast(), b.cast(), n) }
            },
            self.n,
        )
    }

    #[must_use]
    pub fn strncmp_subject(&self) -> Obs {
        self.compare_plain(
            |a, b, n| {
                // SAFETY: both buffers are padded to `n` readable bytes.
                unsafe { raven_libc_str::raw::strncmp(a, b, n) }
            },
            self.n,
        )
    }

    #[must_use]
    pub fn strcasecmp_oracle(&self) -> Obs {
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe {
            super::str_openbsd::oracle_strcasecmp(
                self.a_str().as_ptr().cast(),
                self.b_str().as_ptr().cast(),
            )
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn strcasecmp_subject(&self) -> Obs {
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe {
            raven_libc_str::raw::strcasecmp(self.a_str().as_ptr(), self.b_str().as_ptr())
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn strlcpy_oracle(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        let src = self.b_str();
        // SAFETY: `dst` is `dsize + 4` bytes and `src` is NUL terminated.
        let ret = unsafe {
            super::str_openbsd::oracle_strlcpy(
                dst.as_mut_ptr().cast(),
                src.as_ptr().cast(),
                self.dsize,
            )
        };
        Obs {
            ret: ret as i64,
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strlcpy_subject(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        let src = self.b_str();
        // SAFETY: `dst` is `dsize + 4` bytes and `src` is NUL terminated.
        let ret =
            unsafe { raven_libc_str::raw::strlcpy(dst.as_mut_ptr(), src.as_ptr(), self.dsize) };
        Obs {
            ret: ret as i64,
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strlcat_oracle(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        let initial = self.a_str();
        for (index, &byte) in initial.iter().enumerate() {
            if index < self.dsize {
                dst[index] = byte;
            }
        }
        let src = self.b_str();
        // SAFETY: `dst` is `dsize + 4` bytes and `src` is NUL terminated.
        let ret = unsafe {
            super::str_openbsd::oracle_strlcat(
                dst.as_mut_ptr().cast(),
                src.as_ptr().cast(),
                self.dsize,
            )
        };
        Obs {
            ret: ret as i64,
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strlcat_subject(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        let initial = self.a_str();
        for (index, &byte) in initial.iter().enumerate() {
            if index < self.dsize {
                dst[index] = byte;
            }
        }
        let src = self.b_str();
        // SAFETY: `dst` is `dsize + 4` bytes and `src` is NUL terminated.
        let ret =
            unsafe { raven_libc_str::raw::strlcat(dst.as_mut_ptr(), src.as_ptr(), self.dsize) };
        Obs {
            ret: ret as i64,
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strncat_oracle(&self) -> Obs {
        let initial = self.a_str();
        let mut dst = self.destination(initial.len().wrapping_add(self.n).wrapping_add(1));
        dst[..initial.len()].copy_from_slice(&initial);
        let src = self.b_str();
        // SAFETY: `dst` has room for the initial string, `n` appended bytes and the NUL; `src`
        // is NUL terminated.
        let ret = unsafe {
            super::str_openbsd::oracle_strncat(dst.as_mut_ptr().cast(), src.as_ptr().cast(), self.n)
        };
        Obs {
            ret: Self::pointer_offset(ret.cast(), dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strncat_subject(&self) -> Obs {
        let initial = self.a_str();
        let mut dst = self.destination(initial.len().wrapping_add(self.n).wrapping_add(1));
        dst[..initial.len()].copy_from_slice(&initial);
        let src = self.b_str();
        // SAFETY: `dst` has room for the initial string, `n` appended bytes and the NUL; `src`
        // is NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strncat(dst.as_mut_ptr(), src.as_ptr(), self.n) };
        Obs {
            ret: Self::pointer_offset(ret, dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn memrchr_oracle(&self) -> Obs {
        let mut a = self.a_str();
        a.resize(self.n.max(a.len()), 0);
        // SAFETY: `a` is padded to `n` readable bytes.
        let ret = unsafe { super::str_openbsd::oracle_memrchr(a.as_ptr().cast(), self.c, self.n) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[must_use]
    pub fn memrchr_subject(&self) -> Obs {
        let mut a = self.a_str();
        a.resize(self.n.max(a.len()), 0);
        // SAFETY: `a` is padded to `n` readable bytes.
        let ret = unsafe { raven_libc_str::raw::memrchr(a.as_ptr(), self.c, self.n) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }

    #[must_use]
    pub fn strstr_oracle(&self) -> Obs {
        let a = self.a_str();
        let b = self.b_str();
        // SAFETY: both strings are NUL terminated.
        let ret =
            unsafe { super::str_openbsd::oracle_strstr(a.as_ptr().cast(), b.as_ptr().cast()) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[must_use]
    pub fn strstr_subject(&self) -> Obs {
        let a = self.a_str();
        let b = self.b_str();
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strstr(a.as_ptr(), b.as_ptr()) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }

    #[must_use]
    pub fn strcasestr_oracle(&self) -> Obs {
        let a = self.a_str();
        let b = self.b_str();
        // SAFETY: both strings are NUL terminated.
        let ret =
            unsafe { super::str_openbsd::oracle_strcasestr(a.as_ptr().cast(), b.as_ptr().cast()) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[must_use]
    pub fn strcasestr_subject(&self) -> Obs {
        let a = self.a_str();
        let b = self.b_str();
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strcasestr(a.as_ptr(), b.as_ptr()) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }

    #[must_use]
    pub fn timingsafe_bcmp_oracle(&self) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(self.n.max(a.len()), 0);
        b.resize(self.n.max(b.len()), 0);
        // SAFETY: both buffers are padded to `n` readable bytes.
        let ret = unsafe {
            super::str_openbsd::oracle_timingsafe_bcmp(a.as_ptr().cast(), b.as_ptr().cast(), self.n)
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn timingsafe_bcmp_subject(&self) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(self.n.max(a.len()), 0);
        b.resize(self.n.max(b.len()), 0);
        let ret = raven_libc_str::secure::timingsafe_bcmp(&a[..self.n], &b[..self.n]);
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn timingsafe_memcmp_oracle(&self) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(self.n.max(a.len()), 0);
        b.resize(self.n.max(b.len()), 0);
        // SAFETY: both buffers are padded to `n` readable bytes.
        let ret = unsafe {
            super::str_openbsd::oracle_timingsafe_memcmp(
                a.as_ptr().cast(),
                b.as_ptr().cast(),
                self.n,
            )
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn timingsafe_memcmp_subject(&self) -> Obs {
        let mut a = self.a_str();
        let mut b = self.b_str();
        a.resize(self.n.max(a.len()), 0);
        b.resize(self.n.max(b.len()), 0);
        let ret = raven_libc_str::secure::timingsafe_memcmp(&a[..self.n], &b[..self.n]);
        Obs::value(i64::from(ret))
    }

    /// `strmode` with the mode taken from `n` (the unit tests cover all 4096 permission patterns).
    #[must_use]
    pub fn strmode_oracle(&self) -> Obs {
        let mut out = [0xAAu8; 12];
        // SAFETY: `out` is exactly the 12 bytes the C function writes.
        unsafe { super::str_openbsd::oracle_strmode(self.n as u32, out.as_mut_ptr().cast()) };
        Obs {
            ret: 0,
            buf: out.to_vec(),
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strmode_subject(&self) -> Obs {
        let mut out = [0xAAu8; 12];
        raven_libc_str::strmode::strmode(self.n as u32, &mut out);
        Obs {
            ret: 0,
            buf: out.to_vec(),
            tokens: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------------------------
// str-freebsd
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "oracle-str-freebsd")]
impl StrCase {
    #[must_use]
    pub fn strnstr_oracle(&self) -> Obs {
        let a = self.a_str();
        let b = self.b_str();
        let mut padded = a.clone();
        padded.resize(self.n.max(padded.len()), 0);
        // SAFETY: `padded` is readable for `n` bytes and `b` is NUL terminated.
        let ret = unsafe {
            super::str_freebsd::oracle_strnstr(padded.as_ptr().cast(), b.as_ptr().cast(), self.n)
        };
        Obs::value(Self::pointer_offset(ret.cast(), padded.as_ptr()))
    }

    #[must_use]
    pub fn strnstr_subject(&self) -> Obs {
        let a = self.a_str();
        let b = self.b_str();
        let mut padded = a.clone();
        padded.resize(self.n.max(padded.len()), 0);
        // SAFETY: `padded` is readable for `n` bytes and `b` is NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strnstr(padded.as_ptr(), b.as_ptr(), self.n) };
        Obs::value(Self::pointer_offset(ret, padded.as_ptr()))
    }

    #[must_use]
    pub fn strverscmp_oracle(&self) -> Obs {
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe {
            super::str_freebsd::oracle_strverscmp(
                self.a_str().as_ptr().cast(),
                self.b_str().as_ptr().cast(),
            )
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn strverscmp_subject(&self) -> Obs {
        // SAFETY: both strings are NUL terminated.
        let ret = unsafe {
            raven_libc_str::raw::strverscmp(self.a_str().as_ptr(), self.b_str().as_ptr())
        };
        Obs::value(i64::from(ret))
    }

    #[must_use]
    pub fn mempcpy_oracle(&self) -> Obs {
        let mut dst = self.destination(self.n);
        let mut src = self.a_str();
        src.resize(self.n.max(src.len()), 0);
        // SAFETY: `dst` is `n + 4` bytes and `src` is NUL terminated.
        let ret = unsafe {
            super::str_freebsd::oracle_mempcpy(dst.as_mut_ptr().cast(), src.as_ptr().cast(), self.n)
        };
        Obs {
            ret: Self::pointer_offset(ret.cast(), dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn mempcpy_subject(&self) -> Obs {
        let mut dst = self.destination(self.n);
        let mut src = self.a_str();
        src.resize(self.n.max(src.len()), 0);
        // SAFETY: `dst` is `n + 4` bytes and `src` is NUL terminated.
        let ret = unsafe { raven_libc_str::raw::mempcpy(dst.as_mut_ptr(), src.as_ptr(), self.n) };
        Obs {
            ret: Self::pointer_offset(ret, dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn memset_s_oracle(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        // SAFETY: `dst` is `dsize + 4` bytes and `n <= dsize + 4`.
        let ret = unsafe {
            super::str_freebsd::oracle_memset_s(dst.as_mut_ptr().cast(), self.dsize, self.c, self.n)
        };
        Obs {
            ret: i64::from(ret),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn memset_s_subject(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        // SAFETY: `dst` is `dsize + 4` bytes and `n <= dsize + 4`.
        let ret =
            unsafe { raven_libc_str::raw::memset_s(dst.as_mut_ptr(), self.dsize, self.c, self.n) };
        Obs {
            ret: i64::from(ret),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn fls_oracle(&self) -> Obs {
        // SAFETY: no pointer arguments.
        Obs::value(i64::from(unsafe { super::str_freebsd::oracle_fls(self.c) }))
    }

    #[must_use]
    pub fn fls_subject(&self) -> Obs {
        Obs::value(i64::from(raven_libc_str::bits::fls(self.c)))
    }

    #[must_use]
    pub fn flsl_oracle(&self) -> Obs {
        let value = i64::from(self.c);
        // SAFETY: no pointer arguments.
        Obs::value(i64::from(unsafe { super::str_freebsd::oracle_flsl(value) }))
    }

    #[must_use]
    pub fn flsl_subject(&self) -> Obs {
        Obs::value(i64::from(raven_libc_str::bits::flsl(i64::from(self.c))))
    }

    #[must_use]
    pub fn stdc_count_ones_uc_oracle(&self) -> Obs {
        // SAFETY: no pointer arguments.
        Obs::value(i64::from(unsafe {
            super::str_freebsd::oracle_stdc_count_ones_uc(self.c as u8)
        }))
    }

    #[must_use]
    pub fn stdc_count_ones_uc_subject(&self) -> Obs {
        Obs::value(i64::from(raven_libc_str::stdbit::stdc_count_ones_uc(
            self.c as u8,
        )))
    }

    #[must_use]
    pub fn stdc_bit_ceil_ui_oracle(&self) -> Obs {
        let value = self.c as u32;
        // SAFETY: no pointer arguments.
        Obs::value(i64::from(unsafe {
            super::str_freebsd::oracle_stdc_bit_ceil_ui(value)
        }))
    }

    #[must_use]
    pub fn stdc_bit_ceil_ui_subject(&self) -> Obs {
        Obs::value(i64::from(raven_libc_str::stdbit::stdc_bit_ceil_ui(
            self.c as u32,
        )))
    }
}

// ---------------------------------------------------------------------------------------------
// str-musl
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "oracle-str-musl")]
impl StrCase {
    #[must_use]
    pub fn strlen_oracle(&self) -> Obs {
        // SAFETY: `a` is NUL terminated.
        Obs::value(unsafe { super::str_musl::oracle_strlen(self.a_str().as_ptr().cast()) } as i64)
    }

    #[must_use]
    pub fn strlen_subject(&self) -> Obs {
        // SAFETY: `a` is NUL terminated.
        Obs::value(unsafe { raven_libc_str::raw::strlen(self.a_str().as_ptr()) } as i64)
    }

    #[must_use]
    pub fn strnlen_oracle(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated; `n` is passed to the bounded scan.
        Obs::value(unsafe { super::str_musl::oracle_strnlen(a.as_ptr().cast(), self.n) } as i64)
    }

    #[must_use]
    pub fn strnlen_subject(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated; `n` bounds the scan.
        Obs::value(unsafe { raven_libc_str::raw::strnlen(a.as_ptr(), self.n) } as i64)
    }

    #[must_use]
    pub fn strchr_oracle(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated.
        let ret = unsafe { super::str_musl::oracle_strchr(a.as_ptr().cast(), self.c) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[must_use]
    pub fn strchr_subject(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strchr(a.as_ptr(), self.c) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }

    #[must_use]
    pub fn strrchr_oracle(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated.
        let ret = unsafe { super::str_musl::oracle_strrchr(a.as_ptr().cast(), self.c) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[must_use]
    pub fn strrchr_subject(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strrchr(a.as_ptr(), self.c) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }

    #[must_use]
    pub fn strchrnul_oracle(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated.
        let ret = unsafe { super::str_musl::oracle_strchrnul(a.as_ptr().cast(), self.c) };
        Obs::value(Self::pointer_offset(ret.cast(), a.as_ptr()))
    }

    #[must_use]
    pub fn strchrnul_subject(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated.
        let ret = unsafe { raven_libc_str::raw::strchrnul(a.as_ptr(), self.c) };
        Obs::value(Self::pointer_offset(ret, a.as_ptr()))
    }

    #[must_use]
    pub fn stpcpy_oracle(&self) -> Obs {
        let mut dst = self.destination(self.a_str().len());
        let src = self.a_str();
        // SAFETY: `dst` is large enough for the source and its NUL.
        let ret = unsafe {
            super::str_musl::oracle___stpcpy(dst.as_mut_ptr().cast(), src.as_ptr().cast())
        };
        Obs {
            ret: Self::pointer_offset(ret.cast(), dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn stpcpy_subject(&self) -> Obs {
        let mut dst = self.destination(self.a_str().len());
        let src = self.a_str();
        // SAFETY: `dst` is large enough for the source and its NUL.
        let ret = unsafe { raven_libc_str::raw::stpcpy(dst.as_mut_ptr(), src.as_ptr()) };
        Obs {
            ret: Self::pointer_offset(ret, dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn stpncpy_oracle(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        let src = self.a_str();
        // SAFETY: `dst` is `dsize + 4` bytes.
        let ret = unsafe {
            super::str_musl::oracle___stpncpy(
                dst.as_mut_ptr().cast(),
                src.as_ptr().cast(),
                self.dsize,
            )
        };
        Obs {
            ret: Self::pointer_offset(ret.cast(), dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn stpncpy_subject(&self) -> Obs {
        let mut dst = self.destination(self.dsize);
        let src = self.a_str();
        // SAFETY: `dst` is `dsize + 4` bytes.
        let ret =
            unsafe { raven_libc_str::raw::stpncpy(dst.as_mut_ptr(), src.as_ptr(), self.dsize) };
        Obs {
            ret: Self::pointer_offset(ret, dst.as_ptr()),
            buf: dst,
            tokens: Vec::new(),
        }
    }

    #[must_use]
    pub fn strdup_oracle(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated; the donor result is released with the host free.
        unsafe {
            let ret = super::str_musl::oracle_strdup(a.as_ptr().cast());
            if ret.is_null() {
                return Obs::value(-1);
            }
            let mut buf = Vec::new();
            let mut index = 0usize;
            while *ret.add(index) != 0 {
                buf.push(*ret.add(index) as u8);
                index += 1;
            }
            super::str_musl::free(ret.cast());
            Obs {
                ret: buf.len() as i64,
                buf,
                tokens: Vec::new(),
            }
        }
    }

    #[must_use]
    pub fn strdup_subject(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: the allocator returns the arena's start, which is writable for its length.
        let ret = unsafe { raven_libc_str::raw::strdup_with(test_alloc, a.as_ptr()) };
        if ret.is_null() {
            return Obs::value(-1);
        }
        // SAFETY: the allocator returned `arena`; `ret` points into it.
        unsafe {
            let mut buf = Vec::new();
            let mut index = 0usize;
            while *ret.add(index) != 0 {
                buf.push(*ret.add(index));
                index += 1;
            }
            Obs {
                ret: buf.len() as i64,
                buf,
                tokens: Vec::new(),
            }
        }
    }

    #[must_use]
    pub fn strndup_oracle(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: `a` is NUL terminated and `n` bounds the copy.
        unsafe {
            let ret = super::str_musl::oracle_strndup(a.as_ptr().cast(), self.n);
            if ret.is_null() {
                return Obs::value(-1);
            }
            let mut buf = Vec::new();
            let mut index = 0usize;
            while *ret.add(index) != 0 {
                buf.push(*ret.add(index) as u8);
                index += 1;
            }
            super::str_musl::free(ret.cast());
            Obs {
                ret: buf.len() as i64,
                buf,
                tokens: Vec::new(),
            }
        }
    }

    #[must_use]
    pub fn strndup_subject(&self) -> Obs {
        let a = self.a_str();
        // SAFETY: the allocator returns the arena's start.
        let ret = unsafe { raven_libc_str::raw::strndup_with(test_alloc, a.as_ptr(), self.n) };
        if ret.is_null() {
            return Obs::value(-1);
        }
        // SAFETY: `ret` points into `arena`.
        unsafe {
            let mut buf = Vec::new();
            let mut index = 0usize;
            while *ret.add(index) != 0 {
                buf.push(*ret.add(index));
                index += 1;
            }
            Obs {
                ret: buf.len() as i64,
                buf,
                tokens: Vec::new(),
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// str-errmsg
// ---------------------------------------------------------------------------------------------

#[cfg(feature = "oracle-str-errmsg")]
const MUSL_ERRNOS: &[i32] = &[
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
    27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 42, 43, 60, 61, 62, 63, 67, 71, 72, 74,
    75, 77, 84, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 99, 100, 101, 102, 103, 104, 105, 106,
    107, 108, 110, 111, 112, 113, 114, 115, 116, 117, 119, 121, 122, 123, 124, 125, 126, 127, 128,
    129, 130, 131,
];

#[cfg(feature = "oracle-str-errmsg")]
fn musl_errno(e: i32) -> bool {
    MUSL_ERRNOS.contains(&e)
}

#[cfg(feature = "oracle-str-errmsg")]
impl StrCase {
    /// `strerror` for the 95 numbers musl's table covers. The 36 CLEANROOM texts and the RD-13
    /// slot-0/unknown wording are declared deviations and are checked by the crate's unit tests.
    #[must_use]
    pub fn strerror_oracle(&self) -> Obs {
        let e = self.c.rem_euclid(133).wrapping_add(1);
        if !musl_errno(e) {
            return Obs::value(-1);
        }
        // SAFETY: `strerror` returns a static NUL-terminated string for known numbers.
        unsafe {
            let ret = super::str_errmsg::oracle_strerror(e);
            Obs {
                ret: i64::from(e),
                buf: copy_cstr(ret),
                tokens: vec![(1, 0)],
            }
        }
    }

    #[must_use]
    pub fn strerror_subject(&self) -> Obs {
        let e = self.c.rem_euclid(133).wrapping_add(1);
        if !musl_errno(e) {
            return Obs::value(-1);
        }
        let mut buffer = [0u8; raven_libc_str::errmsg::BUF_LEN];
        let (text, known) = raven_libc_str::errmsg::strerror_into(e, &mut buffer);
        Obs {
            ret: i64::from(e),
            buf: text.to_vec(),
            tokens: vec![(i64::from(known), 0)],
        }
    }

    /// `strsignal` for valid numbers 1..=64 only (the numbered unknown text deviates by design).
    #[must_use]
    pub fn strsignal_oracle(&self) -> Obs {
        let s = self.c.rem_euclid(64).wrapping_add(1);
        // SAFETY: `strsignal` returns a static NUL-terminated string.
        unsafe {
            let ret = super::str_errmsg::oracle_strsignal(s);
            Obs {
                ret: i64::from(s),
                buf: copy_cstr(ret),
                tokens: vec![(1, 0)],
            }
        }
    }

    #[must_use]
    pub fn strsignal_subject(&self) -> Obs {
        let s = self.c.rem_euclid(64).wrapping_add(1);
        let mut buffer = [0u8; raven_libc_str::errmsg::BUF_LEN];
        let (text, known) = raven_libc_str::errmsg::strsignal_into(s, &mut buffer);
        Obs {
            ret: i64::from(s),
            buf: text.to_vec(),
            tokens: vec![(i64::from(known), 0)],
        }
    }
}

#[cfg(feature = "oracle-str-errmsg")]
fn copy_cstr(pointer: *const core::ffi::c_char) -> Vec<u8> {
    let mut bytes = Vec::new();
    // SAFETY: the caller passes a C string returned by the donor.
    unsafe {
        let mut index = 0usize;
        while *pointer.add(index) != 0 {
            bytes.push(*pointer.add(index) as u8);
            index += 1;
        }
    }
    bytes
}

/// Test allocator for `strdup_with`/`strndup_with`: a per-thread scratch `Vec` whose pointer
/// stays valid until the next call. The harness is single-threaded per test case.
#[cfg(feature = "oracle-str-musl")]
unsafe fn test_alloc(size: usize) -> *mut u8 {
    use std::cell::RefCell;
    thread_local! {
        static ARENA: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
    ARENA.with(|arena| {
        let mut arena = arena.borrow_mut();
        arena.clear();
        arena.resize(size.max(1), 0);
        arena.as_mut_ptr()
    })
}
