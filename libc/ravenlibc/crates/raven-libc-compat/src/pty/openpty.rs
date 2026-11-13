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

//! `openpty` (freebsd:lib/libutil/pty.c, BSD-3).

use raven_libc_abi::errno::Errno;

use crate::seams::{PtyOps, Termios, Winsize};

use super::shared::{O_NOCTTY, O_RDWR, PTNAME_CAP, copy_name};

/// `openpty`.
///
/// Opens a master/slave pair and returns `(master, slave)`. Every failing step closes what was
/// already opened (`bad: close(master)` in the donor).
pub fn openpty<P: PtyOps>(
    pty: &mut P,
    name: Option<&mut [u8]>,
    term: Option<&Termios>,
    win: Option<&Winsize>,
) -> Result<(i32, i32), Errno> {
    let master = pty.posix_openpt(O_RDWR | O_NOCTTY)?;
    let mut pts_name = [0u8; PTNAME_CAP];
    let mut length = 0usize;
    let step = (|| {
        pty.grantpt(master)?;
        pty.unlockpt(master)?;
        length = pty.ptsname_r(master, &mut pts_name)?;
        let slave = pty.open(pts_name.get(..length).unwrap_or(&[]), O_RDWR)?;
        Ok(slave)
    })();
    let slave = match step {
        Ok(slave) => slave,
        Err(error) => {
            pty.close(master);
            return Err(error);
        }
    };
    if let Some(dst) = name
        && let Err(error) = copy_name(dst, pts_name.get(..length).unwrap_or(&[]))
    {
        pty.close(master);
        pty.close(slave);
        return Err(error);
    }
    if let Some(term) = term {
        // The donor ignores a tcsetattr failure.
        let _ = pty.tcsetattr(slave, term);
    }
    if let Some(win) = win {
        // The donor ignores an ioctl failure.
        let _ = pty.set_winsize(slave, win);
    }
    Ok((master, slave))
}
