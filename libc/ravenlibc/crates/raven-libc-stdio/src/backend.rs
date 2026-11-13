// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The backend enum: a typed dispatch table instead of musl's four function-pointer slots.
//!
//! `Fd` is the descriptor backend, `Mem` the `fmemopen`/`open_memstream` cookie, `Cookie` the
//! GNU `fopencookie` callbacks, `Fun` the BSD `funopen` callbacks, and `Closed` the state a
//! FILE reaches after `fclose` (list walkers skip it, `fclose.c` comment).
//!
//! The enum is taken out of the FILE state with `mem::replace` for the duration of one
//! operation so the backend implementation can borrow the rest of the state mutably; this is
//! the Rust equivalent of the donor's function-pointer call with `FILE *`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::EBADF;

use crate::backend_fd;
use crate::cookie::CookieState;
use crate::file::File;
use crate::funopen::FunState;
use crate::mem::MemCookie;
use crate::traits::Env;

/// The backend of one stream.
pub(crate) enum Backend {
    /// The descriptor backend (`__stdio_read`, `__stdio_write`, `__stdio_seek`, `__stdio_close`).
    Fd,
    /// A memory stream (`fmemopen`, `open_memstream`, `open_wmemstream`).
    Mem(MemCookie),
    /// A GNU cookie stream (`fopencookie`).
    Cookie(CookieState),
    /// A BSD `funopen` stream.
    Fun(FunState),
    /// A closed stream.
    Closed,
}

/// The result of a backend operation.
pub(crate) type BackendResult<T> = Result<T, Errno>;

/// Run one read through the backend.
pub(crate) fn read<E: Env>(file: &File<E>, buf: &mut [u8]) -> BackendResult<usize> {
    let env = file.env();
    let st = file.st_mut();
    let mut backend = core::mem::replace(&mut st.backend, Backend::Closed);
    let result = match &mut backend {
        Backend::Fd => backend_fd::read(st, env, buf),
        Backend::Mem(cookie) => cookie.read(st, buf),
        Backend::Cookie(cookie) => cookie.read(st, buf),
        Backend::Fun(callbacks) => callbacks.read(st, buf),
        Backend::Closed => Ok(0),
    };
    st.backend = backend;
    result
}

/// Run one write through the backend.
pub(crate) fn write<E: Env>(file: &File<E>, buf: &[u8]) -> BackendResult<usize> {
    let env = file.env();
    let st = file.st_mut();
    let mut backend = core::mem::replace(&mut st.backend, Backend::Closed);
    let result = match &mut backend {
        Backend::Fd => {
            if st.probe {
                st.probe = false;
                if st.flags & crate::file::F_SVB == 0 && !env.isatty(st.fd) {
                    st.lbf = -1;
                }
            }
            backend_fd::write(st, env, buf)
        }
        Backend::Mem(cookie) => cookie.write(st, buf),
        Backend::Cookie(cookie) => cookie.write(st, buf),
        Backend::Fun(callbacks) => callbacks.write(st, buf),
        Backend::Closed => Ok(0),
    };
    st.backend = backend;
    result
}

/// Run one seek through the backend.
pub(crate) fn seek<E: Env>(file: &File<E>, off: i64, whence: i32) -> BackendResult<i64> {
    let env = file.env();
    let st = file.st_mut();
    let mut backend = core::mem::replace(&mut st.backend, Backend::Closed);
    let result = match &mut backend {
        Backend::Fd => env.seek(st.fd, off, whence),
        Backend::Mem(cookie) => cookie.seek(st, off, whence),
        Backend::Cookie(cookie) => cookie.seek(st, off, whence),
        Backend::Fun(callbacks) => callbacks.seek(st, off, whence),
        Backend::Closed => Err(Errno(EBADF as u16)),
    };
    st.backend = backend;
    result
}

/// Run one close through the backend.
pub(crate) fn close<E: Env>(file: &File<E>) -> BackendResult<()> {
    let env = file.env();
    let st = file.st_mut();
    let mut backend = core::mem::replace(&mut st.backend, Backend::Closed);
    let result = match &mut backend {
        Backend::Fd => env.close(st.fd),
        Backend::Mem(cookie) => cookie.close(st),
        Backend::Cookie(cookie) => cookie.close(st),
        Backend::Fun(callbacks) => callbacks.close(st),
        Backend::Closed => Ok(()),
    };
    st.backend = backend;
    result
}
