// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/stdin.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor file has no per-file header; it is covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.
//
//! The three standard streams and their in-value `.bss` buffers (`musl:stdin.c`, `stdout.c`,
//! `stderr.c`; `donor-map/04` 3.5 rules 1-2).
//!
//! `stdin` and `stdout` own `UNGET + 1024` bytes inside the same value (no allocation on the
//! first `printf`, H-19); `stderr` owns only the `UNGET` prefix and is always unbuffered
//! (T12). `stdout` starts line buffered (`lbf = '\n'`) with the lazy inverted probe of
//! [`crate::buf`]; it does **not** carry `F_LAZYBUF`.
//!
//! The buffers are attached on the first accessor call: a `const fn` cannot point a FILE at a
//! sibling field of the value it is building, and the index representation exists precisely to
//! avoid self-referential pointers (`donor-map/04` 4.1). After the attach the value never
//! moves (the shell keeps it in static storage), so the region address is stable.

use core::sync::atomic::{AtomicU8, Ordering};

use crate::backend::Backend;
use crate::buf::STD_BUF;
use crate::file::{F_NORD, F_NOWR, F_PERM, File, UNGET};
use crate::raw::BufStore;
use crate::state::{State, StdKind};
use crate::traits::Env;

/// The state of one lazily attached standard buffer.
const UNBOUND: u8 = 0;
/// A thread is attaching the buffer.
const BINDING: u8 = 1;
/// The buffer is attached.
const BOUND: u8 = 2;

/// The three standard streams of one libc instance.
pub struct StdStreams<E: Env> {
    state: &'static State<E>,
    stdin: File<E>,
    stdout: File<E>,
    stderr: File<E>,
    stdin_buf: BufStore<{ UNGET + STD_BUF }>,
    stdout_buf: BufStore<{ UNGET + STD_BUF }>,
    stderr_buf: BufStore<UNGET>,
    stdin_bound: AtomicU8,
    stdout_bound: AtomicU8,
    stderr_bound: AtomicU8,
}

impl<E: Env> StdStreams<E> {
    /// Build the three streams; usable in a `static` initialiser.
    pub const fn new(state: &'static State<E>) -> Self {
        Self {
            state,
            stdin: File::standard(state, 0, F_PERM | F_NOWR, -1, false, STD_BUF, Backend::Fd),
            stdout: File::standard(
                state,
                1,
                F_PERM | F_NORD,
                b'\n' as i32,
                true,
                STD_BUF,
                Backend::Fd,
            ),
            stderr: File::standard(state, 2, F_PERM | F_NORD, -1, false, 0, Backend::Fd),
            stdin_buf: BufStore::new(),
            stdout_buf: BufStore::new(),
            stderr_buf: BufStore::new(),
            stdin_bound: AtomicU8::new(UNBOUND),
            stdout_bound: AtomicU8::new(UNBOUND),
            stderr_bound: AtomicU8::new(UNBOUND),
        }
    }

    /// `stdin`.
    pub fn stdin(&'static self) -> &'static File<E> {
        self.attach(StdKind::In);
        &self.stdin
    }

    /// `stdout`.
    pub fn stdout(&'static self) -> &'static File<E> {
        self.attach(StdKind::Out);
        &self.stdout
    }

    /// `stderr`.
    pub fn stderr(&'static self) -> &'static File<E> {
        self.attach(StdKind::Err);
        &self.stderr
    }

    /// The three streams in `stdin`, `stdout`, `stderr` order.
    pub fn all(&'static self) -> [&'static File<E>; 3] {
        [self.stdin(), self.stdout(), self.stderr()]
    }

    /// The crate state.
    #[must_use]
    pub fn state(&self) -> &'static State<E> {
        self.state
    }

    /// Attach the buffer of one stream once.
    fn attach(&'static self, kind: StdKind) {
        match kind {
            StdKind::In => attach_one(
                &self.stdin_bound,
                &self.stdin,
                &self.stdin_buf,
                self.state,
                StdKind::In,
                STD_BUF,
            ),
            StdKind::Out => attach_one(
                &self.stdout_bound,
                &self.stdout,
                &self.stdout_buf,
                self.state,
                StdKind::Out,
                STD_BUF,
            ),
            StdKind::Err => attach_one(
                &self.stderr_bound,
                &self.stderr,
                &self.stderr_buf,
                self.state,
                StdKind::Err,
                0,
            ),
        }
    }
}

/// Attach one buffer region to one standard stream.
fn attach_one<E: Env, const N: usize>(
    flag: &AtomicU8,
    file: &'static File<E>,
    buffer: &'static BufStore<N>,
    state: &'static State<E>,
    kind: StdKind,
    size: usize,
) {
    let mut observed = flag.load(Ordering::Acquire);
    loop {
        match observed {
            BOUND => break,
            BINDING => {
                core::hint::spin_loop();
                observed = flag.load(Ordering::Acquire);
            }
            _ => {
                if flag
                    .compare_exchange(UNBOUND, BINDING, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    {
                        let st = file.st_mut();
                        st.buffer.set_raw(buffer.raw());
                        st.buf_size = size;
                        if size == 0 {
                            st.rpos = UNGET;
                            st.rend = UNGET;
                        } else if kind == StdKind::In
                            && st.lbf < 0
                            && st.flags & crate::file::F_SVB == 0
                            && state.env().isatty(st.fd)
                        {
                            // A terminal stdin is line buffered (C11 7.21.3p7; BSD's
                            // `__smakebuf` does the same for every stream).
                            st.lbf = i32::from(b'\n');
                        }
                    }
                    flag.store(BOUND, Ordering::Release);
                    break;
                }
                observed = flag.load(Ordering::Acquire);
            }
        }
    }
    state.bind_std(kind, file);
}

#[cfg(test)]
#[path = "std_streams_ut.rs"]
mod tests;
