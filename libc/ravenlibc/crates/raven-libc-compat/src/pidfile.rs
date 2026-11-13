// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1999, 2016, 2026 The NetBSD Foundation, Inc.  All rights reserved.
//
// RavenBSD: behaviour from netbsd:lib/libutil/pidfile.c @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: pidfile.c,v 1.20 2026/06/11 21:13:47 roy Exp $); the OpenBSD
// `pidfile()` protocol of donor-map 15 3.B is not in the pinned checkout. No code is copied:
// the state machine is the ticket's section 4.3 plus the donor's atexit/cleanup rule.
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
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pidfile` (donor-map 15 T3, 02 B12).
//!
//! Protocol: path `/var/run/<basename>.pid` (basename defaults to the caller's program name),
//! contents `"<pid>\n"`, an `atexit` registration, and a cleanup that unlinks only when the file
//! still holds the creating pid. A write failure removes the file and keeps the error.
//!
//! `PidState` additionally carries the `atexit` hook: `PidfileOps::atexit` takes a plain `fn()`
//! (the seam of ticket 4.2), so the shell passes its trampoline here; the crate never keeps
//! process-global state.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;

use crate::seams::PidfileOps;

/// `/var/run/` (`_PATH_VARRUN` in the donor).
pub const PATH_VARRUN: &[u8] = b"/var/run/";
/// The `getprogname()` fallback when the caller passes no name and no program name.
pub const DEFAULT_PROGNAME: &[u8] = b"unknown";

/// The state of one created pidfile.
#[derive(Clone, Debug)]
pub struct PidState {
    /// The path written.
    pub path: Vec<u8>,
    /// The pid stored in the file (`-1` before a successful create).
    pub pid: i32,
    /// The `atexit` hook the shell wants registered.
    pub atexit_hook: fn(),
}

impl PidState {
    /// A state with the given `atexit` hook.
    #[must_use]
    pub fn new(atexit_hook: fn()) -> Self {
        Self {
            path: Vec::new(),
            pid: -1,
            atexit_hook,
        }
    }

    /// Whether a pidfile was created successfully.
    #[must_use]
    pub fn active(&self) -> bool {
        self.pid >= 0
    }
}

impl Default for PidState {
    fn default() -> Self {
        Self::new(noop)
    }
}

fn noop() {}

fn build_path(basename: Option<&[u8]>, progname: &[u8]) -> Vec<u8> {
    let name = basename.unwrap_or(if progname.is_empty() {
        DEFAULT_PROGNAME
    } else {
        progname
    });
    let mut path = Vec::with_capacity(PATH_VARRUN.len().wrapping_add(name.len()).wrapping_add(4));
    path.extend_from_slice(PATH_VARRUN);
    path.extend_from_slice(name);
    path.extend_from_slice(b".pid");
    path
}

fn pid_contents(pid: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buf = [0u8; 16];
    let mut index = buf.len();
    let mut magnitude = pid.unsigned_abs();
    loop {
        index = index.wrapping_sub(1);
        buf[index] = b'0'.wrapping_add(u8::try_from(magnitude % 10).unwrap_or(0));
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[index..]);
    out.push(b'\n');
    out
}

/// `pidfile`: create `/var/run/<name>.pid` and register the cleanup hook.
///
/// On a write failure the path is unlinked and the error is returned (the donor keeps `errno`).
pub fn pidfile<O: PidfileOps>(
    o: &mut O,
    basename: Option<&[u8]>,
    progname: &[u8],
    state: &mut PidState,
) -> Result<(), Errno> {
    let path = build_path(basename, progname);
    state.path = path.clone();
    state.pid = -1;
    o.atexit(state.atexit_hook)?;
    let pid = o.getpid();
    let contents = pid_contents(pid);
    if let Err(error) = o.create_write(&path, &contents) {
        o.unlink(&path);
        state.path.clear();
        return Err(error);
    }
    state.pid = pid;
    Ok(())
}

/// The `atexit` cleanup: unlink only when the stored pid is still this process.
pub fn cleanup<O: PidfileOps>(o: &mut O, state: &PidState) {
    if !state.path.is_empty() && state.pid == o.getpid() {
        o.unlink(&state.path);
    }
}

#[cfg(test)]
#[path = "pidfile_ut.rs"]
mod tests;
