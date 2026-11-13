// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2015-2016 Nuxi, https://nuxi.nl/
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/stdlib/realpath.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (function `realpath1` only; the Nuxi file carries no per-file revision tag)
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

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // RavenBSD: donor path index arithmetic

//! `realpath` as a userspace walker.
//!
//! Adaptations: `lstat`/`readlink`/`getcwd` are [`Walker`] methods; the
//! `__sys___realpathat` fast path (`realpath.c:236`) is deleted; the three `PATH_MAX` stack
//! arrays become one `[u8; PATH_MAX]` plus a heap scratch for a symlink target; `MAXSYMLINKS` is
//! 40 (H-19) and the failing path stays in `out` on error (T-13).

use alloc::vec::Vec;

use raven_libc_abi::errno::{EACCES, EINVAL, ELOOP, ENAMETOOLONG, ENOENT, ENOTDIR, Errno};
use raven_libc_abi::limits::PATH_MAX;

/// `MAXSYMLINKS` (H-19).
pub const MAXSYMLINKS: usize = 40;

/// The file kinds `realpath` needs.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum FileKind {
    /// A regular file.
    Regular,
    /// A directory.
    Dir,
    /// A symbolic link.
    Symlink,
    /// Anything else (device, socket, ...).
    Other,
}

/// The filesystem seam.
pub trait Walker {
    /// `lstat` the path and return its kind.
    fn lstat_kind(&mut self, path: &[u8]) -> Result<FileKind, Errno>;
    /// `readlink` into `buf`; returns the target length.
    fn readlink(&mut self, path: &[u8], buf: &mut [u8]) -> Result<usize, Errno>;
    /// `getcwd` into `buf`; returns the length.
    fn getcwd(&mut self, buf: &mut [u8]) -> Result<usize, Errno>;
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

/// The ticket's `PATH_MAX` as a `usize`.
const CAP: usize = PATH_MAX as usize;

/// Resolve `path` into `out`; returns the length written (without a NUL).
///
/// `out` must hold `PATH_MAX` bytes. An empty path is `ENOENT`; a missing component is the
/// walker's error; more than `MAXSYMLINKS` links is `ELOOP`.
///
/// # Errors
///
/// `EINVAL` for a too-short `out`, `ENOENT` for an empty path, `ENAMETOOLONG`, `ELOOP`,
/// `ENOTDIR`, or the walker's error.
pub fn realpath<W: Walker>(walker: &mut W, path: &[u8], out: &mut [u8]) -> Result<usize, Errno> {
    if out.len() < CAP {
        return Err(errno(EINVAL));
    }
    if path.is_empty() {
        return Err(errno(ENOENT));
    }

    let mut left = [0u8; CAP];
    let mut left_len;
    let mut resolved_len;
    if path.first() == Some(&b'/') {
        if let Some(slot) = out.first_mut() {
            *slot = b'/';
        }
        if path.len() == 1 {
            return Ok(1);
        }
        left_len = path.len().wrapping_sub(1);
        if left_len >= CAP {
            return Err(errno(ENAMETOOLONG));
        }
        copy(&mut left, 0, path, 1, left_len);
        resolved_len = 1;
    } else {
        match walker.getcwd(out) {
            Ok(length) => resolved_len = length,
            Err(error) => {
                if let Some(slot) = out.first_mut() {
                    *slot = b'.';
                }
                if let Some(nul) = out.get_mut(1) {
                    *nul = 0;
                }
                return Err(error);
            }
        }
        if resolved_len >= CAP {
            return Err(errno(ENAMETOOLONG));
        }
        left_len = path.len();
        if left_len >= CAP {
            return Err(errno(ENAMETOOLONG));
        }
        copy(&mut left, 0, path, 0, left_len);
    }

    let mut symlinks = 0usize;
    while left_len != 0 {
        let slash = left
            .get(..left_len)
            .and_then(|slice| slice.iter().position(|byte| *byte == b'/'));
        let next_token_len = slash.unwrap_or(left_len);

        if out
            .get(resolved_len.wrapping_sub(1))
            .copied()
            .unwrap_or(b'/')
            != b'/'
        {
            if resolved_len.wrapping_add(1) >= CAP {
                return Err(errno(ENAMETOOLONG));
            }
            if let Some(slot) = out.get_mut(resolved_len) {
                *slot = b'/';
            }
            resolved_len = resolved_len.wrapping_add(1);
        }
        let prev_len = resolved_len;
        let token = left.get(..next_token_len).unwrap_or(&[]);
        let token_empty = token.is_empty();
        let token_dot = token == b".";
        let token_dotdot = token == b"..";
        let consume = |left: &mut [u8; CAP], left_len: &mut usize| {
            if let Some(position) = slash {
                *left_len = left_len.wrapping_sub(position).wrapping_sub(1);
                copy_within(left, 0, position.wrapping_add(1), *left_len);
            } else {
                *left_len = 0;
            }
        };
        if token_empty {
            consume(&mut left, &mut left_len);
            continue;
        }
        if token_dot {
            consume(&mut left, &mut left_len);
            continue;
        }
        if token_dotdot {
            consume(&mut left, &mut left_len);
            if resolved_len > 1 {
                resolved_len = resolved_len.wrapping_sub(1);
                if let Some(slot) = out.get_mut(resolved_len) {
                    *slot = 0;
                }
                let cut = out
                    .get(..resolved_len)
                    .and_then(|slice| slice.iter().rposition(|byte| *byte == b'/'))
                    .map_or(0, |position| position.wrapping_add(1));
                resolved_len = cut;
            }
            continue;
        }
        if resolved_len.wrapping_add(next_token_len) >= CAP {
            return Err(errno(ENAMETOOLONG));
        }
        copy(out, resolved_len, token, 0, next_token_len);
        resolved_len = resolved_len.wrapping_add(next_token_len);
        consume(&mut left, &mut left_len);

        let kind = match walker.lstat_kind(out.get(..resolved_len).unwrap_or(&[])) {
            Ok(kind) => kind,
            Err(error) => {
                if error.0 == EACCES as u16 || error.0 == ENOTDIR as u16 {
                    let prev = if prev_len > 1 {
                        prev_len.wrapping_sub(1)
                    } else {
                        prev_len
                    };
                    resolved_len = prev;
                    if let Some(slot) = out.get_mut(resolved_len) {
                        *slot = 0;
                    }
                }
                return Err(error);
            }
        };
        if kind == FileKind::Symlink {
            symlinks = symlinks.wrapping_add(1);
            if symlinks > MAXSYMLINKS {
                return Err(errno(ELOOP));
            }
            let mut target = Vec::new();
            if target.try_reserve_exact(CAP).is_err() {
                return Err(errno(ENOMEM_LOCAL));
            }
            target.resize(CAP, 0);
            let length = walker.readlink(out.get(..resolved_len).unwrap_or(&[]), &mut target)?;
            if length == 0 {
                return Err(errno(ENOENT));
            }
            if length >= CAP {
                return Err(errno(ENAMETOOLONG));
            }
            if target.first() == Some(&b'/') {
                resolved_len = 1;
                if let Some(slot) = out.get_mut(1) {
                    *slot = 0;
                }
            } else {
                let cut = out
                    .get(..resolved_len)
                    .and_then(|slice| slice.iter().rposition(|byte| *byte == b'/'))
                    .map_or(0, |position| position.wrapping_add(1));
                resolved_len = cut;
                if let Some(slot) = out.get_mut(resolved_len) {
                    *slot = 0;
                }
            }
            let mut new_left: Vec<u8> = Vec::new();
            let target_slice = target.get(..length).unwrap_or(&[]);
            if new_left.try_reserve_exact(CAP).is_err() {
                return Err(errno(ENOMEM_LOCAL));
            }
            new_left.extend_from_slice(target_slice);
            if slash.is_some() {
                if target_slice.last() != Some(&b'/') {
                    new_left.push(b'/');
                }
                new_left.extend_from_slice(left.get(..left_len).unwrap_or(&[]));
            }
            if new_left.len() >= CAP {
                return Err(errno(ENAMETOOLONG));
            }
            left_len = new_left.len();
            copy(&mut left, 0, &new_left, 0, left_len);
        } else if kind != FileKind::Dir && slash.is_some() {
            return Err(errno(ENOTDIR));
        }
    }

    if resolved_len > 1 && out.get(resolved_len.wrapping_sub(1)) == Some(&b'/') {
        resolved_len = resolved_len.wrapping_sub(1);
    }
    if let Some(slot) = out.get_mut(resolved_len) {
        *slot = 0;
    }
    Ok(resolved_len)
}

/// A local `ENOMEM` value (the `alloc` failure path; `raven-libc-abi` exports it).
const ENOMEM_LOCAL: i32 = raven_libc_abi::errno::ENOMEM;

fn copy(dst: &mut [u8], dst_start: usize, src: &[u8], src_start: usize, len: usize) {
    let Some(source) = src.get(src_start..src_start.wrapping_add(len)) else {
        return;
    };
    let Some(target) = dst.get_mut(dst_start..dst_start.wrapping_add(len)) else {
        return;
    };
    target.copy_from_slice(source);
}

fn copy_within(buf: &mut [u8], dst: usize, src: usize, len: usize) {
    if src.wrapping_add(len) <= buf.len() && dst.wrapping_add(len) <= buf.len() {
        buf.copy_within(src..src.wrapping_add(len), dst);
    }
}

#[cfg(test)]
#[path = "realpath_ut.rs"]
mod tests;
