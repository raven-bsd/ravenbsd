// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2010 The NetBSD Foundation, Inc.
// All rights reserved.
// This code is derived from software contributed to The NetBSD Foundation
// by Matthias Scheler.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/stdlib/_env.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: _env.c,v 1.15 2024/01/02 19:27:26 christos Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! The environment table.
//!
//! Deviation D-2 (A-stdlib-9): no string is ever freed and no stored string is overwritten.
//! Every `setenv` allocates a fresh `Box<[u8]>`, `Box::leak`s it and replaces the list slot;
//! `getenv` pointers therefore stay valid forever. The donor's red-black ownership tree exists
//! only to decide `free()` and is not translated (`// RavenBSD: not translated: ownership tree,
//! no frees (D-2)`).

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::errno::{EINVAL, ENOMEM};

/// The process environment as a list of NUL-terminated `"NAME=value"` strings.
#[derive(Clone, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct EnvTable {
    entries: Vec<&'static [u8]>,
    generation: u64,
}

impl EnvTable {
    /// An empty table; the shell adopts the kernel-provided `environ` with [`Self::adopt_initial`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
            generation: 0,
        }
    }

    /// Copy the initial environment list (not the bytes) into the table.
    pub fn adopt_initial(&mut self, strings: &[&'static [u8]]) {
        self.entries.clear();
        self.entries.extend_from_slice(strings);
        self.generation = self.generation.wrapping_add(1);
    }

    /// Replace the list after the program assigned `environ = other`.
    pub fn reload(&mut self, strings: &[&'static [u8]]) {
        self.adopt_initial(strings);
    }

    /// The published `environ` array.
    #[must_use]
    pub fn as_slice(&self) -> &[&'static [u8]] {
        &self.entries
    }

    /// Bumped on every change.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `getenv`: the value slice without its NUL, or `None`.
    #[must_use]
    pub fn getenv(&self, name: &[u8]) -> Option<&'static [u8]> {
        let name = valid_name(name)?;
        self.find(name)
    }

    /// `getenv_r`: copy the value and its NUL into `buf`; returns the value length.
    ///
    /// # Errors
    ///
    /// `ERANGE` when `buf` cannot hold the value and its NUL, `ENOENT` for an invalid or absent
    /// name. The shell returns 0 on success.
    pub fn getenv_r(&self, name: &[u8], buf: &mut [u8]) -> Result<usize, Errno> {
        let Some(name) = valid_name(name) else {
            return Err(errno(raven_libc_abi::errno::ENOENT));
        };
        let Some(value) = self.find(name) else {
            return Err(errno(raven_libc_abi::errno::ENOENT));
        };
        let needed = value.len().wrapping_add(1);
        if buf.len() < needed {
            return Err(errno(raven_libc_abi::errno::ERANGE));
        }
        if let Some(slot) = buf.get_mut(..value.len()) {
            slot.copy_from_slice(value);
        }
        if let Some(slot) = buf.get_mut(value.len()) {
            *slot = 0;
        }
        Ok(value.len())
    }

    /// `setenv`.
    ///
    /// # Errors
    ///
    /// `EINVAL` for an empty name or one containing `=`, `ENOMEM` when the new string cannot be
    /// allocated.
    pub fn setenv(&mut self, name: &[u8], value: &[u8], overwrite: bool) -> Result<(), Errno> {
        let name = valid_name(name).ok_or_else(|| errno(EINVAL))?;
        if let Some(index) = self.find_position(name) {
            if !overwrite {
                return Ok(());
            }
            let leaked = make_entry(name, value)?;
            if let Some(slot) = self.entries.get_mut(index) {
                *slot = leaked;
            }
            self.generation = self.generation.wrapping_add(1);
            return Ok(());
        }
        let leaked = make_entry(name, value)?;
        self.entries.push(leaked);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// `putenv`: store the caller's `'static` string, or remove the variable when `s` has no `=`
    /// (RD-26).
    ///
    /// # Errors
    ///
    /// `EINVAL` for an empty or malformed name.
    pub fn putenv(&mut self, s: &'static [u8]) -> Result<(), Errno> {
        let body = strip_nul(s);
        let Some(equal) = body.iter().position(|byte| *byte == b'=') else {
            // RavenBSD: putenv("NAME") removes NAME (RD-26, glibc behaviour).
            return self.unsetenv(body);
        };
        if equal == 0 {
            return Err(errno(EINVAL));
        }
        let name = match body.get(..equal) {
            Some(name) => name,
            None => return Err(errno(EINVAL)),
        };
        if let Some(index) = self.find_position(name) {
            if let Some(slot) = self.entries.get_mut(index) {
                *slot = s;
            }
            self.generation = self.generation.wrapping_add(1);
            return Ok(());
        }
        self.entries.push(s);
        self.generation = self.generation.wrapping_add(1);
        Ok(())
    }

    /// `unsetenv`: remove every entry with this name.
    ///
    /// # Errors
    ///
    /// `EINVAL` for an empty name or one containing `=`.
    pub fn unsetenv(&mut self, name: &[u8]) -> Result<(), Errno> {
        let name = valid_name(name).ok_or_else(|| errno(EINVAL))?;
        let before = self.entries.len();
        let mut index = 0usize;
        while index < self.entries.len() {
            let matches = self
                .entries
                .get(index)
                .is_some_and(|entry| entry_name(entry) == Some(name));
            if matches {
                self.entries.remove(index);
            } else {
                index = index.wrapping_add(1);
            }
        }
        if self.entries.len() != before {
            self.generation = self.generation.wrapping_add(1);
        }
        Ok(())
    }

    /// `clearenv`: forget the array, not the strings.
    pub fn clearenv(&mut self) {
        if self.entries.is_empty() {
            return;
        }
        self.entries.clear();
        self.generation = self.generation.wrapping_add(1);
    }

    fn find(&self, name: &[u8]) -> Option<&'static [u8]> {
        self.entries
            .iter()
            .find(|entry| entry_name(entry) == Some(name))
            .and_then(|entry| entry_value(entry))
    }

    fn find_position(&self, name: &[u8]) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry_name(entry) == Some(name))
    }
}

impl Default for EnvTable {
    fn default() -> Self {
        Self::new()
    }
}

/// A name is non-empty and contains no `=` (A-stdlib-14).
fn valid_name(name: &[u8]) -> Option<&[u8]> {
    if name.is_empty() || name.contains(&b'=') || name.contains(&0) {
        return None;
    }
    Some(name)
}

/// The NUL-free part of a `'static` environment string.
fn strip_nul(s: &[u8]) -> &[u8] {
    match s.iter().position(|byte| *byte == 0) {
        Some(end) => s.get(..end).unwrap_or(&[]),
        None => s,
    }
}

/// The name part of `"NAME=value\0"`.
fn entry_name(entry: &[u8]) -> Option<&[u8]> {
    let equal = entry.iter().position(|byte| *byte == b'=')?;
    entry.get(..equal)
}

/// The value part without the trailing NUL.
fn entry_value(entry: &'static [u8]) -> Option<&'static [u8]> {
    let equal = entry.iter().position(|byte| *byte == b'=')?;
    let rest = entry.get(equal.wrapping_add(1)..)?;
    Some(strip_nul(rest))
}

/// Allocate and leak `"name=value\0"`.
fn make_entry(name: &[u8], value: &[u8]) -> Result<&'static [u8], Errno> {
    let length = name
        .len()
        .checked_add(value.len())
        .and_then(|sum| sum.checked_add(2))
        .ok_or_else(|| errno(ENOMEM))?;
    let mut buffer: Vec<u8> = Vec::new();
    buffer
        .try_reserve_exact(length)
        .map_err(|_| errno(ENOMEM))?;
    buffer.extend_from_slice(name);
    buffer.push(b'=');
    buffer.extend_from_slice(value);
    buffer.push(0);
    Ok(Box::leak(buffer.into_boxed_slice()))
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;
