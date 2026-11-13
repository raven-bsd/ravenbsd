// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Case types, their generators and the unsafe calls into the C oracle.
//!
//! This module is the only place beside [`crate::decls`] that contains unsafe code. Every unsafe
//! block carries a `// SAFETY:` comment. The case types own their buffers and check the
//! `buflen >= dsize` invariant when a corpus case is parsed, so the C never writes outside the
//! allocation.

use core::ffi::c_int;

#[cfg(feature = "oracle-string-basic")]
use core::ffi::{c_char, c_void};

use serde::Deserialize;

use crate::corpus::{hex_encode, parse_bytes, render_bytes};
use crate::diff::Case;
use crate::rng::Rng;

// `errno` slot of the calling thread (glibc and musl both export the accessor).
#[cfg(target_os = "linux")]
unsafe extern "C" {
    fn __errno_location() -> *mut c_int;
}

/// Read the calling thread's `errno` (0 on a non-Linux host).
pub fn errno_get() -> i32 {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: glibc and musl both return a valid pointer to the calling thread's errno slot.
        unsafe { *__errno_location() }
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

/// Write the calling thread's `errno` (a no-op on a non-Linux host).
pub fn errno_set(value: i32) {
    #[cfg(target_os = "linux")]
    {
        // SAFETY: glibc and musl both return a valid pointer to the calling thread's errno slot.
        unsafe {
            *__errno_location() = value;
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = value;
    }
}

// ---------------------------------------------------------------------------------------------
// strlcpy
// ---------------------------------------------------------------------------------------------

/// One `strlcpy` case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrlcpyCase {
    /// Destination size passed to the C.
    pub dsize: usize,
    /// Allocation size; must be at least `dsize`.
    pub buflen: usize,
    /// Byte the whole destination buffer is filled with before the call.
    pub fill: u8,
    /// Source bytes; the C view is truncated at the first NUL.
    pub bytes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrlcpyCaseRaw {
    dsize: usize,
    buflen: usize,
    fill: u8,
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    bytes_hex: Option<String>,
}

impl TryFrom<StrlcpyCaseRaw> for StrlcpyCase {
    type Error = String;

    fn try_from(raw: StrlcpyCaseRaw) -> Result<Self, Self::Error> {
        if raw.buflen < raw.dsize {
            return Err(format!(
                "strlcpy case: buflen {} is smaller than dsize {}",
                raw.buflen, raw.dsize
            ));
        }
        Ok(Self {
            dsize: raw.dsize,
            buflen: raw.buflen,
            fill: raw.fill,
            bytes: parse_bytes(raw.bytes, raw.bytes_hex, "strlcpy case")?,
        })
    }
}

impl<'de> Deserialize<'de> for StrlcpyCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = StrlcpyCaseRaw::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

/// Whole `strlcpy` observation: return value and every byte of the destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrlcpyObs {
    /// Return value.
    pub ret: usize,
    /// Destination buffer after the call, guard bytes included.
    pub buf: Vec<u8>,
}

impl StrlcpyCase {
    /// The C view of the source: bytes before the first NUL.
    ///
    /// The donor reads up to the first NUL, so the case type truncates `bytes` at the first zero
    /// before calling both sides; the corpus may carry the untruncated form to describe the input.
    #[must_use]
    pub fn normalised(&self) -> &[u8] {
        match self.bytes.iter().position(|&byte| byte == 0) {
            Some(index) => &self.bytes[..index],
            None => &self.bytes,
        }
    }

    /// A random case: length 0..23, `dsize` 0..27, 0..4 guard bytes, a 1-in-8 embedded NUL.
    #[must_use]
    pub fn random(rng: &mut Rng) -> Self {
        let len = rng.below(24) as usize;
        let dsize = rng.below(28) as usize;
        let fill = rng.next_u64() as u8;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            if rng.below(8) == 0 {
                bytes.push(0);
            } else {
                bytes.push((rng.below(255) + 1) as u8);
            }
        }
        let guard = rng.below(5) as usize;
        Self {
            dsize,
            buflen: dsize + guard,
            fill,
            bytes,
        }
    }

    /// All strings over `{a,b}` of length 0..=4, for every `dsize` 0..=6.
    #[must_use]
    pub fn exhaustive() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for bytes in byte_strings(b"ab", 4) {
            for dsize in 0..=6usize {
                cases.push(Self {
                    dsize,
                    buflen: dsize + 4,
                    fill: 0xAA,
                    bytes: bytes.clone(),
                });
            }
        }
        Box::new(cases.into_iter())
    }

    /// Run a Rust subject on this case.
    #[must_use]
    pub fn run_subject(&self, f: fn(&mut [u8], usize, &[u8]) -> usize) -> StrlcpyObs {
        let mut buffer = vec![self.fill; self.buflen];
        let ret = f(&mut buffer, self.dsize, self.normalised());
        StrlcpyObs { ret, buf: buffer }
    }

    /// Run the C oracle on this case.
    #[cfg(feature = "oracle-string-basic")]
    #[must_use]
    pub fn run_oracle(&self) -> StrlcpyObs {
        oracle_strlcpy(self.dsize, self.buflen, self.fill, self.normalised())
    }
}

impl Case for StrlcpyCase {
    type Obs = StrlcpyObs;

    fn shrink(&self) -> Vec<Self> {
        let mut candidates = Vec::new();
        if self.dsize > 0 {
            candidates.push(Self {
                dsize: self.dsize - 1,
                ..self.clone()
            });
        }
        if !self.bytes.is_empty() {
            candidates.push(Self {
                bytes: self.bytes[..self.bytes.len() - 1].to_vec(),
                ..self.clone()
            });
        }
        if self.buflen > self.dsize {
            candidates.push(Self {
                buflen: self.buflen - 1,
                ..self.clone()
            });
        }
        candidates
    }

    fn to_toml(&self) -> String {
        format!(
            "dsize = {}\nbuflen = {}\nfill = {}\n{}",
            self.dsize,
            self.buflen,
            self.fill,
            render_bytes(&self.bytes)
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("ret = {}, buf_hex = \"{}\"", obs.ret, hex_encode(&obs.buf))
    }
}

/// Call the C `strlcpy`.
#[cfg(feature = "oracle-string-basic")]
#[must_use]
pub fn oracle_strlcpy(dsize: usize, buflen: usize, fill: u8, bytes: &[u8]) -> StrlcpyObs {
    let mut buffer = vec![fill; buflen];
    let mut source = bytes.to_vec();
    source.push(0);
    // SAFETY: `buffer` is writable for `buflen` bytes and `source` is NUL-terminated; the case
    // invariant `buflen >= dsize` keeps the donor inside the destination allocation.
    let ret = unsafe {
        crate::decls::string_basic::oracle_strlcpy(
            buffer.as_mut_ptr().cast::<c_char>(),
            source.as_ptr().cast::<c_char>(),
            dsize,
        )
    };
    StrlcpyObs { ret, buf: buffer }
}

// ---------------------------------------------------------------------------------------------
// strnlen
// ---------------------------------------------------------------------------------------------

/// One `strnlen` case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrnlenCase {
    /// Maximum length passed to the C.
    pub maxlen: usize,
    /// Source bytes; the C sees a NUL after them and embedded NULs terminate.
    pub bytes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrnlenCaseRaw {
    maxlen: usize,
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    bytes_hex: Option<String>,
}

impl TryFrom<StrnlenCaseRaw> for StrnlenCase {
    type Error = String;

    fn try_from(raw: StrnlenCaseRaw) -> Result<Self, Self::Error> {
        Ok(Self {
            maxlen: raw.maxlen,
            bytes: parse_bytes(raw.bytes, raw.bytes_hex, "strnlen case")?,
        })
    }
}

impl<'de> Deserialize<'de> for StrnlenCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = StrnlenCaseRaw::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl StrnlenCase {
    /// A random case: length 0..23, `maxlen` 0..27, a 1-in-8 embedded NUL, any other byte.
    #[must_use]
    pub fn random(rng: &mut Rng) -> Self {
        let len = rng.below(24) as usize;
        let maxlen = rng.below(28) as usize;
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            if rng.below(8) == 0 {
                bytes.push(0);
            } else {
                bytes.push(rng.below(256) as u8);
            }
        }
        Self { maxlen, bytes }
    }

    /// All strings over `{0x00,0x61,0x80}` of length 0..=6, for the fixed `maxlen` domain.
    #[must_use]
    pub fn exhaustive() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for bytes in byte_strings(&[0x00, 0x61, 0x80], 6) {
            for &maxlen in &[0, 1, 2, 3, 7, 0x61, 0x80, 0xFF, 0x161] {
                cases.push(Self {
                    maxlen,
                    bytes: bytes.clone(),
                });
            }
        }
        Box::new(cases.into_iter())
    }

    /// Run a Rust subject on this case.
    #[must_use]
    pub fn run_subject(&self, f: fn(&[u8], usize) -> usize) -> usize {
        f(&self.bytes, self.maxlen)
    }

    /// Run the C oracle on this case.
    #[cfg(feature = "oracle-string-basic")]
    #[must_use]
    pub fn run_oracle(&self) -> usize {
        oracle_strnlen(&self.bytes, self.maxlen)
    }
}

impl Case for StrnlenCase {
    type Obs = usize;

    fn shrink(&self) -> Vec<Self> {
        let mut candidates = Vec::new();
        if self.maxlen > 0 {
            candidates.push(Self {
                maxlen: self.maxlen - 1,
                ..self.clone()
            });
        }
        if self.bytes.len() > 1 {
            candidates.push(Self {
                bytes: self.bytes[1..].to_vec(),
                ..self.clone()
            });
        }
        if !self.bytes.is_empty() {
            candidates.push(Self {
                bytes: self.bytes[..self.bytes.len() - 1].to_vec(),
                ..self.clone()
            });
        }
        candidates
    }

    fn to_toml(&self) -> String {
        format!("maxlen = {}\n{}", self.maxlen, render_bytes(&self.bytes))
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("ret = {obs}")
    }
}

/// Call the C `strnlen`.
#[cfg(feature = "oracle-string-basic")]
#[must_use]
pub fn oracle_strnlen(bytes: &[u8], maxlen: usize) -> usize {
    let mut source = bytes.to_vec();
    source.push(0);
    // SAFETY: `source` is NUL-terminated for the duration of the call; the donor reads at most
    // `maxlen` bytes and stops at the terminator.
    unsafe { crate::decls::string_basic::oracle_strnlen(source.as_ptr().cast::<c_char>(), maxlen) }
}

// ---------------------------------------------------------------------------------------------
// memchr
// ---------------------------------------------------------------------------------------------

/// One `memchr` case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemchrCase {
    /// Search byte, passed as a C `int`.
    pub c: i32,
    /// Haystack bytes.
    pub bytes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MemchrCaseRaw {
    c: i32,
    #[serde(default)]
    bytes: Option<String>,
    #[serde(default)]
    bytes_hex: Option<String>,
}

impl TryFrom<MemchrCaseRaw> for MemchrCase {
    type Error = String;

    fn try_from(raw: MemchrCaseRaw) -> Result<Self, Self::Error> {
        Ok(Self {
            c: raw.c,
            bytes: parse_bytes(raw.bytes, raw.bytes_hex, "memchr case")?,
        })
    }
}

impl<'de> Deserialize<'de> for MemchrCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = MemchrCaseRaw::deserialize(deserializer)?;
        Self::try_from(raw).map_err(serde::de::Error::custom)
    }
}

impl MemchrCase {
    /// A random case: length 0..23, any haystack byte, occasionally a `c` outside a byte.
    #[must_use]
    pub fn random(rng: &mut Rng) -> Self {
        let len = rng.below(24) as usize;
        let c = if rng.below(8) == 0 {
            rng.pick(&[-1, 0x161])
        } else {
            rng.below(256) as i32
        };
        let mut bytes = Vec::with_capacity(len);
        for _ in 0..len {
            bytes.push(rng.below(256) as u8);
        }
        Self { c, bytes }
    }

    /// All strings over `{0x00,0x61,0x80}` of length 0..=6, for the fixed `c` domain.
    #[must_use]
    pub fn exhaustive() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for bytes in byte_strings(&[0x00, 0x61, 0x80], 6) {
            for &c in &[0, 1, 2, 3, 7, 0x61, 0x80, 0xFF, 0x161, -1] {
                cases.push(Self {
                    c,
                    bytes: bytes.clone(),
                });
            }
        }
        Box::new(cases.into_iter())
    }

    /// Run a Rust subject on this case.
    #[must_use]
    pub fn run_subject(&self, f: fn(&[u8], i32) -> Option<usize>) -> Option<usize> {
        f(&self.bytes, self.c)
    }

    /// Run the C oracle on this case.
    #[cfg(feature = "oracle-string-basic")]
    #[must_use]
    pub fn run_oracle(&self) -> Option<usize> {
        oracle_memchr(&self.bytes, self.c)
    }
}

impl Case for MemchrCase {
    type Obs = Option<usize>;

    fn shrink(&self) -> Vec<Self> {
        let mut candidates = Vec::new();
        if self.bytes.len() > 1 {
            candidates.push(Self {
                bytes: self.bytes[1..].to_vec(),
                ..self.clone()
            });
        }
        if !self.bytes.is_empty() {
            candidates.push(Self {
                bytes: self.bytes[..self.bytes.len() - 1].to_vec(),
                ..self.clone()
            });
        }
        if self.c != 0 {
            candidates.push(Self {
                c: 0,
                ..self.clone()
            });
        }
        candidates
    }

    fn to_toml(&self) -> String {
        format!("c = {}\n{}", self.c, render_bytes(&self.bytes))
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        match obs {
            Some(index) => format!("ret = {index}"),
            None => "ret = -1".to_string(),
        }
    }
}

/// Call the C `memchr`; the result is an offset, `None` for a null pointer.
#[cfg(feature = "oracle-string-basic")]
#[must_use]
pub fn oracle_memchr(bytes: &[u8], c: i32) -> Option<usize> {
    // SAFETY: `bytes` is valid for `bytes.len()` reads for the duration of the call; a null result
    // is turned into `None` without dereferencing it.
    let pointer = unsafe {
        crate::decls::string_basic::oracle_memchr(bytes.as_ptr().cast::<c_void>(), c, bytes.len())
    };
    if pointer.is_null() {
        None
    } else {
        Some(pointer as usize - bytes.as_ptr() as usize)
    }
}

fn byte_strings(alphabet: &[u8], max_len: usize) -> Vec<Vec<u8>> {
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

#[cfg(test)]
#[path = "call_ut.rs"]
mod tests;
