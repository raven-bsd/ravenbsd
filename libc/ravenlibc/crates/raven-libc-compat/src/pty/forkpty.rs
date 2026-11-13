// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1990, 1993, 1994 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libutil/pty.c and freebsd:lib/libutil/login_tty.c
// @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tags: $FreeBSD$; the FreeBSD files carry the SPDX line BSD-3-Clause)
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

//! `forkpty` (freebsd:lib/libutil/pty.c, BSD-3).

use raven_libc_abi::errno::Errno;

use crate::seams::{ForkResult, PtyOps, Termios, Winsize};

use super::login_tty::login_tty;
use super::openpty::openpty;

/// `forkpty`.
///
/// Returns `(ForkResult::Parent(pid), master)` in the parent and `(ForkResult::Child, -1)` in
/// the child. `PtyOps::fork` is `Err(ENOSYS)` until PG-45; both ends are closed on that error.
pub fn forkpty<P: PtyOps>(
    pty: &mut P,
    name: Option<&mut [u8]>,
    term: Option<&Termios>,
    win: Option<&Winsize>,
) -> Result<(ForkResult, i32), Errno> {
    let (master, slave) = openpty(pty, name, term, win)?;
    match pty.fork() {
        Err(error) => {
            pty.close(master);
            pty.close(slave);
            Err(error)
        }
        Ok(ForkResult::Child) => {
            pty.close(master);
            // The donor ignores login_tty's failure in the child.
            let _ = login_tty(pty, slave);
            Ok((ForkResult::Child, -1))
        }
        Ok(ForkResult::Parent(pid)) => {
            pty.close(slave);
            Ok((ForkResult::Parent(pid), master))
        }
    }
}
