// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1990, 1993, 1994 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: module root of the freebsd:lib/libutil pty ports
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `openpty`, `login_tty` and `forkpty` (donor-map 15 T3, 02 X48).
//!
//! The FreeBSD files use only `posix_openpt`, `grantpt`, `unlockpt`, `ptsname`, `open` and
//! `ioctl(TIOCSWINSZ)`; the group lookup of the `tty` group is dropped (Raven has no `tty`
//! group and `grantpt` is the server's job). `forkpty` leaves `PtyOps::fork`'s `ENOSYS`
//! untouched (A-bsd-8) and closes both ends on that error.
//!
//! `login_tty`'s donor order is `setsid`, `tcsetsid`/`TIOCSCTTY`, `dup2` onto 0, 1, 2, then
//! `close(fd)` when `fd > 2`. A failing `setsid` is tolerated (the donor falls back to
//! `getsid(0)`; the seam has no `getsid`, so the error is ignored). The child of `forkpty`
//! returns `(ForkResult::Child, -1)` because the master end was closed.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
mod shared;

pub mod forkpty;
pub mod login_tty;
pub mod openpty;

pub use forkpty::forkpty;
pub use login_tty::login_tty;
pub use openpty::openpty;

/// The Linux `struct winsize` (C36 has no record; the shell declares the matching C type).
pub use crate::seams::Winsize;

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
