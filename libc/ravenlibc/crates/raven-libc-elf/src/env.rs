// SPDX-License-Identifier: BSD-2-Clause
//
// Copyright 1996, 1997, 1998, 1999, 2000 John D. Polstra.
// Copyright 2003 Alexander Kabaev <kan@FreeBSD.ORG>.
// Copyright 2009-2013 Konstantin Belousov <kib@FreeBSD.ORG>.
// Copyright 2012 John Marino <draco@marino.st>.
// Copyright 2014-2017 The FreeBSD Foundation
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/rtld.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: 133344405f532bd54ff3629a77d4c4e062f01fdb)
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
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
//! The `LD_*` environment: the kept variables, the secure-exec column and the parse result.
//!
//! The table has one row per kept variable; in secure mode (`trust == false`) a row whose
//! `unsecure` column is set is ignored and its index in `envp` is returned for removal from
//! `environ` (A-ld-13). The first occurrence of a name wins, like `getenv`.

use alloc::vec::Vec;

use crate::debug::DebugWords;

/// One kept environment variable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LdVar {
    /// The variable name without the `LD_` prefix.
    pub name: &'static str,
    /// Cleared in secure mode.
    pub unsecure: bool,
    /// The loader may update it after start-up.
    pub can_update: bool,
}

/// The table, in the donor's order.
pub const LD_VARS: &[LdVar] = &[
    LdVar {
        name: "BIND_NOW",
        unsecure: false,
        can_update: true,
    },
    LdVar {
        name: "PRELOAD",
        unsecure: true,
        can_update: false,
    },
    LdVar {
        name: "LIBRARY_PATH",
        unsecure: true,
        can_update: true,
    },
    LdVar {
        name: "DEBUG",
        unsecure: true,
        can_update: true,
    },
    LdVar {
        name: "LIBRARY_PATH_RPATH",
        unsecure: true,
        can_update: true,
    },
    LdVar {
        name: "DYNAMIC_WEAK",
        unsecure: true,
        can_update: true,
    },
    LdVar {
        name: "TRACE_LOADED_OBJECTS",
        unsecure: false,
        can_update: false,
    },
    LdVar {
        name: "TRACE_LOADED_OBJECTS_PROGNAME",
        unsecure: false,
        can_update: false,
    },
    LdVar {
        name: "TRACE_LOADED_OBJECTS_FMT1",
        unsecure: false,
        can_update: false,
    },
    LdVar {
        name: "TRACE_LOADED_OBJECTS_FMT2",
        unsecure: false,
        can_update: false,
    },
    LdVar {
        name: "TRACE_LOADED_OBJECTS_ALL",
        unsecure: false,
        can_update: false,
    },
    LdVar {
        name: "SHOW_AUXV",
        unsecure: true,
        can_update: false,
    },
    LdVar {
        name: "STATIC_TLS_EXTRA",
        unsecure: true,
        can_update: false,
    },
];

/// The default static-TLS surplus, in bytes (13 3.9).
pub const DEFAULT_STATIC_TLS_EXTRA: u64 = 1664;

/// The parsed configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvConfig {
    /// `LD_BIND_NOW` is present.
    pub bind_now: bool,
    /// `LD_PRELOAD`, split on spaces and colons.
    pub preload: Vec<Vec<u8>>,
    /// `LD_LIBRARY_PATH`.
    pub library_path: Option<Vec<u8>>,
    /// `LD_LIBRARY_PATH_RPATH` starts with `y`, `Y` or `1`.
    pub library_path_rpath: bool,
    /// `LD_DEBUG` words.
    pub debug: DebugWords,
    /// `LD_TRACE_LOADED_OBJECTS` is present.
    pub trace_loaded_objects: bool,
    /// `LD_TRACE_LOADED_OBJECTS_PROGNAME`.
    pub trace_prog: Option<Vec<u8>>,
    /// `LD_TRACE_LOADED_OBJECTS_FMT1`.
    pub trace_fmt1: Option<Vec<u8>>,
    /// `LD_TRACE_LOADED_OBJECTS_FMT2`.
    pub trace_fmt2: Option<Vec<u8>>,
    /// `LD_TRACE_LOADED_OBJECTS_ALL` is present.
    pub trace_all: bool,
    /// `LD_SHOW_AUXV` is present.
    pub show_auxv: bool,
    /// `LD_STATIC_TLS_EXTRA`.
    pub static_tls_extra: Option<u64>,
    /// Weak definitions do not stop the search (`LD_DYNAMIC_WEAK` absent; the donor default).
    pub dynamic_weak: bool,
}

fn split(bytes: &[u8]) -> impl Iterator<Item = &[u8]> {
    bytes.split(|byte| *byte == b' ' || *byte == b':' || *byte == b'\t')
}

/// Parse an unsigned integer the way the donor's `parse_integer` does: `0x` hex, leading `0`
/// octal, otherwise decimal; an empty value, a trailing character or an overflow is `None`.
pub fn parse_integer(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() {
        return Some(0);
    }
    let (digits, radix) = if let Some(rest) = bytes
        .strip_prefix(b"0x")
        .or_else(|| bytes.strip_prefix(b"0X"))
    {
        (rest, 16u32)
    } else if bytes.len() > 1 && bytes.first() == Some(&b'0') {
        (bytes.get(1..)?, 8u32)
    } else {
        (bytes, 10u32)
    };
    if digits.is_empty() {
        return Some(0);
    }
    let mut value: u64 = 0;
    for byte in digits {
        let digit = match *byte {
            b'0'..=b'9' => u32::from(byte.wrapping_sub(b'0')),
            b'a'..=b'f' => u32::from(byte.wrapping_sub(b'a')).wrapping_add(10),
            b'A'..=b'F' => u32::from(byte.wrapping_sub(b'A')).wrapping_add(10),
            _ => return None,
        };
        if digit >= radix {
            return None;
        }
        value = value
            .checked_mul(u64::from(radix))?
            .checked_add(u64::from(digit))?;
    }
    Some(value)
}

/// The default configuration: every flag off except `dynamic_weak`, which the donor enables
/// when `LD_DYNAMIC_WEAK` is absent.
impl Default for EnvConfig {
    fn default() -> Self {
        Self {
            bind_now: false,
            preload: Vec::new(),
            library_path: None,
            library_path_rpath: false,
            debug: DebugWords::default(),
            trace_loaded_objects: false,
            trace_prog: None,
            trace_fmt1: None,
            trace_fmt2: None,
            trace_all: false,
            show_auxv: false,
            static_tls_extra: None,
            dynamic_weak: true,
        }
    }
}

impl EnvConfig {
    /// Parse `envp`; the returned indices name the variables the caller removes from `environ`.
    pub fn parse(envp: &[&[u8]], trust: bool) -> (Self, Vec<usize>) {
        let mut config = Self::default();
        let mut remove = Vec::new();
        let mut seen: Vec<&str> = Vec::new();
        for (index, entry) in envp.iter().enumerate() {
            let Some(separator) = entry.iter().position(|byte| *byte == b'=') else {
                continue;
            };
            let key = entry.get(..separator).unwrap_or(&[]);
            let value = entry.get(separator.saturating_add(1)..).unwrap_or(&[]);
            let Some(name) = key.strip_prefix(b"LD_") else {
                continue;
            };
            let Ok(name_text) = core::str::from_utf8(name) else {
                continue;
            };
            let Some(variable) = LD_VARS.iter().find(|variable| variable.name == name_text) else {
                continue;
            };
            if seen.contains(&name_text) {
                continue;
            }
            seen.push(name_text);
            if !trust && variable.unsecure {
                remove.push(index);
                continue;
            }
            match variable.name {
                "BIND_NOW" => config.bind_now = true,
                "PRELOAD" => {
                    config.preload = split(value)
                        .filter(|part| !part.is_empty())
                        .map(|part| part.to_vec())
                        .collect();
                }
                "LIBRARY_PATH" => config.library_path = Some(value.to_vec()),
                "DEBUG" => config.debug = DebugWords::parse(value),
                "LIBRARY_PATH_RPATH" => {
                    config.library_path_rpath =
                        matches!(value.first().copied(), Some(b'y') | Some(b'Y') | Some(b'1'));
                }
                "DYNAMIC_WEAK" => config.dynamic_weak = false,
                "TRACE_LOADED_OBJECTS" => config.trace_loaded_objects = true,
                "TRACE_LOADED_OBJECTS_PROGNAME" => config.trace_prog = Some(value.to_vec()),
                "TRACE_LOADED_OBJECTS_FMT1" => config.trace_fmt1 = Some(value.to_vec()),
                "TRACE_LOADED_OBJECTS_FMT2" => config.trace_fmt2 = Some(value.to_vec()),
                "TRACE_LOADED_OBJECTS_ALL" => config.trace_all = true,
                "SHOW_AUXV" => config.show_auxv = true,
                "STATIC_TLS_EXTRA" => config.static_tls_extra = parse_integer(value),
                _ => {}
            }
        }
        (config, remove)
    }

    /// The static TLS surplus to reserve.
    pub fn static_tls_surplus(&self) -> u64 {
        self.static_tls_extra.unwrap_or(DEFAULT_STATIC_TLS_EXTRA)
    }
}

#[cfg(test)]
#[path = "env_ut.rs"]
mod tests;
