// SPDX-License-Identifier: BSD-2-Clause
/*
 *	$OpenBSD: lockf.c,v 1.5 2008/06/26 05:42:05 ray Exp $
 *	$NetBSD: lockf.c,v 1.1 1997/12/20 20:23:18 kleink Exp $
 */
/*-
 * Copyright (c) 1997 The NetBSD Foundation, Inc.
 * All rights reserved.
 *
 * This code is derived from software contributed to The NetBSD Foundation
 * by Klaus Klein.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
 * ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
 * TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
 * PURPOSE ARE DISCLAIMED.  IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
 * BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
 * CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
 * SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
 * INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
 * CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
 * ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
 * POSSIBILITY OF SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from openbsd:lib/libc/gen/lockf.c @
// 4ce71f2817f675745b0596aaf04de4a122b1cc52; the record-lock mapping is kept
// with `F_TEST` asking for `F_WRLCK` (A-sys-12, so shared locks are detected),
// and the `F_TEST` failure errno is `EACCES` instead of `EAGAIN`.

//! `lockf` over `fcntl` record locks.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_libc_abi::structs::Flock;

/// One `fcntl` call the lock operation needs.
pub trait RecordLock {
    /// Perform `fcntl(fd, command, &mut lock)`.
    fn fcntl(&mut self, command: i32, lock: &mut Flock) -> Result<(), Errno>;
    /// The calling pid, for the `F_TEST` self-lock rule.
    fn pid(&self) -> i32;
}

/// `lockf(fd, function, size)`, dispatched over [`RecordLock`].
pub fn lockf(fd: i32, function: i32, size: i64, record: &mut dyn RecordLock) -> Result<(), Errno> {
    let mut lock = Flock {
        l_type: 0,
        l_whence: fcntl::SEEK_CUR as i16,
        l_start: 0,
        l_len: size,
        l_pid: 0,
    };
    let command = match function {
        fcntl::F_ULOCK => {
            lock.l_type = fcntl::F_UNLCK as i16;
            fcntl::F_SETLK
        }
        fcntl::F_LOCK => {
            lock.l_type = fcntl::F_WRLCK as i16;
            fcntl::F_SETLKW
        }
        fcntl::F_TLOCK => {
            lock.l_type = fcntl::F_WRLCK as i16;
            fcntl::F_SETLK
        }
        fcntl::F_TEST => {
            lock.l_type = fcntl::F_WRLCK as i16;
            record.fcntl(fcntl::F_GETLK, &mut lock)?;
            if lock.l_type == fcntl::F_UNLCK as i16 || lock.l_pid == record.pid() {
                return Ok(());
            }
            return Err(Errno::EACCES);
        }
        _ => return Err(Errno::EINVAL),
    };
    let _ = fd;
    record.fcntl(command, &mut lock)
}

/// The per-fd lock state used by [`crate::platform::Platform::lockf`] until PG-21.
pub struct UnsupportedLock {
    /// The pid returned to `F_TEST`.
    pub pid: i32,
}

impl RecordLock for UnsupportedLock {
    fn fcntl(&mut self, _command: i32, _lock: &mut Flock) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn pid(&self) -> i32 {
        self.pid
    }
}

#[cfg(test)]
#[path = "lockf_ut.rs"]
mod tests;
