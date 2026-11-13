// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/internal/stdio_impl.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The `FILE` object: musl's state machine expressed with offsets into one buffer region.
//!
//! The donor keeps five pointers (`rpos/rend/wpos/wend/wbase`) into a buffer that has
//! [`UNGET`] bytes of pushback room before it. The indices here are offsets into the region
//! `[UNGET area | buffer]`, so the three standard streams can be built by `const` code with
//! their buffers inside [`crate::StdStreams`] and no self-referential pointers (`FILE` layout is
//! not ABI, A-stdio-17).
//!
//! Flag values and the state invariants are `donor-map/04` 4.1 and 4.8. Lowering notes:
//! `shend/shlim/shcnt`, `locale` and the `mustbezero_*` glibc padding fields are dropped; the
//! four function-pointer slots became `crate::backend::Backend`; `lock` starts at 0, never
//! `-1` (A-stdio-9); `FFINALLOCK`/`FLOCK` became [`crate::lock`] and [`crate::exit`].

use alloc::vec::Vec;
use core::sync::atomic::AtomicI32;

use raven_libc_ctype::MbState;

use crate::backend::Backend;
use crate::raw::{FileCell, RawBuf};
use crate::state::State;
use crate::traits::Env;

/// The pushback area before the buffer (`stdio_impl.h:7`).
pub const UNGET: usize = 8;

/// `F_PERM`: a standard stream; never freed, never removed from the set.
pub(crate) const F_PERM: u32 = 1;
/// `F_NORD`: reading is not allowed.
pub(crate) const F_NORD: u32 = 4;
/// `F_NOWR`: writing is not allowed.
pub(crate) const F_NOWR: u32 = 8;
/// End of file reached.
pub(crate) const F_EOF: u32 = 16;
/// A read or write error is sticky until `clearerr`.
pub(crate) const F_ERR: u32 = 32;
/// `setvbuf` was called; buffer policy is fixed.
pub(crate) const F_SVB: u32 = 64;
/// Append mode.
pub(crate) const F_APP: u32 = 128;
/// `__fsetlocking(BYCALLER)`: the caller owns the stream lock.
pub(crate) const F_BYCALLER: u32 = 256;
/// Allocate the buffer lazily at the first read or write (`donor-map/04` 3.5 rule 3).
pub(crate) const F_LAZYBUF: u32 = 512;

/// `MAYBE_WAITERS` of `stdio_impl.h:93`.
pub(crate) const MAYBE_WAITERS: i32 = 0x4000_0000;

/// The buffer region of a stream.
pub(crate) enum Buffer {
    /// No region yet (a lazy stream before its first operation).
    Empty,
    /// A heap region allocated by `smakebuf`.
    Heap(Vec<u8>),
    /// A caller or `StdStreams` region.
    Fixed(RawBuf),
}

impl Buffer {
    /// The whole region `[UNGET area | buffer]`.
    #[must_use]
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Empty => &[],
            Self::Heap(vec) => vec.as_slice(),
            Self::Fixed(raw) => raw.bytes(),
        }
    }

    /// The whole region, mutably.
    #[must_use]
    pub(crate) fn bytes_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Empty => &mut [],
            Self::Heap(vec) => vec.as_mut_slice(),
            Self::Fixed(raw) => raw.bytes_mut(),
        }
    }

    /// Replace the region.
    pub(crate) fn set_raw(&mut self, raw: RawBuf) {
        *self = Self::Fixed(raw);
    }

    /// Replace the region with an owned allocation.
    pub(crate) fn set_vec(&mut self, vec: Vec<u8>) {
        *self = Self::Heap(vec);
    }

    /// Drop the region (a closed stream or one that switched to unbuffered mode).
    pub(crate) fn clear(&mut self) {
        *self = Self::Empty;
    }
}

/// The mutable state of one `FILE`.
pub(crate) struct FileState<E: Env> {
    /// `F_*` flags.
    pub(crate) flags: u32,
    /// First unread byte (offset into the region).
    pub(crate) rpos: usize,
    /// One past the last buffered read byte.
    pub(crate) rend: usize,
    /// First unwritten byte (offset into the region).
    pub(crate) wpos: usize,
    /// One past the last writable byte.
    pub(crate) wend: usize,
    /// Start of the pending write run.
    pub(crate) wbase: usize,
    /// The buffer size, excluding the [`UNGET`] prefix.
    pub(crate) buf_size: usize,
    /// `-1` for byte buffering, `'\n'` for line buffering.
    pub(crate) lbf: i32,
    /// The fd, or `-1` for memory/cookie streams.
    pub(crate) fd: i32,
    /// Orientation: `0` none, `-1` byte, `1` wide.
    pub(crate) mode: i8,
    /// Owner tid plus MAYBE_WAITERS.
    pub(crate) lock: AtomicI32,
    /// Explicit `flockfile` nesting depth.
    pub(crate) lockcount: i64,
    /// The `popen` child pid, `0` otherwise.
    pub(crate) pipe_pid: i32,
    /// Cached offset for memory streams.
    pub(crate) off: i64,
    /// `fgetln`'s growing line buffer, freed by `fclose`.
    pub(crate) getln_buf: Option<Vec<u8>>,
    /// The multibyte state of the wide operations.
    pub(crate) mbstate: MbState,
    /// The conversion mode of the wide operations (H-3).
    pub(crate) mbmode: raven_libc_ctype::MbMode,
    /// The backend state machine.
    pub(crate) backend: Backend,
    /// The buffer region.
    pub(crate) buffer: Buffer,
    /// Run the lazy stdout tty probe on the first write (`buf.rs`).
    pub(crate) probe: bool,
    /// Open-file list previous link.
    pub(crate) prev: Option<&'static crate::file::File<E>>,
    /// Open-file list next link.
    pub(crate) next: Option<&'static crate::file::File<E>>,
}

impl<E: Env> FileState<E> {
    /// The zeroed initial state of a closed FILE.
    pub(crate) const fn empty() -> Self {
        Self {
            flags: 0,
            rpos: 0,
            rend: 0,
            wpos: 0,
            wend: 0,
            wbase: 0,
            buf_size: 0,
            lbf: -1,
            fd: -1,
            mode: 0,
            lock: AtomicI32::new(0),
            lockcount: 0,
            pipe_pid: 0,
            off: 0,
            getln_buf: None,
            mbstate: MbState::INITIAL,
            mbmode: raven_libc_ctype::MbMode::Utf8,
            backend: Backend::Closed,
            buffer: Buffer::Empty,
            probe: false,
            prev: None,
            next: None,
        }
    }

    /// Clear the read window.
    pub(crate) fn clear_read(&mut self) {
        self.rpos = 0;
        self.rend = 0;
    }

    /// Clear the write window.
    pub(crate) fn clear_write(&mut self) {
        self.wpos = 0;
        self.wbase = 0;
        self.wend = 0;
    }

    /// Whether pending output exists.
    pub(crate) fn has_pending_write(&self) -> bool {
        self.wpos != self.wbase
    }

    /// The number of unread buffered bytes, as a signed window (`rend - rpos`).
    pub(crate) fn unread(&self) -> isize {
        let rend = isize::try_from(self.rend).unwrap_or(isize::MAX);
        let rpos = isize::try_from(self.rpos).unwrap_or(isize::MAX);
        rend.wrapping_sub(rpos)
    }

    /// Set the byte orientation the way `mode |= mode - 1` does: `0` becomes `-1`, `-1`
    /// stays byte, `1` stays wide.
    pub(crate) fn orient_byte(&mut self) {
        self.mode |= self.mode.wrapping_sub(1);
    }
}

/// One stream.
pub struct File<E: Env> {
    pub(crate) state: &'static State<E>,
    pub(crate) cell: FileCell<FileState<E>>,
}

impl<E: Env> File<E> {
    /// A closed FILE bound to `state`, usable in a `static` initialiser.
    pub const fn new(state: &'static State<E>) -> Self {
        Self {
            state,
            cell: FileCell::new(FileState::empty()),
        }
    }

    /// A standard stream, usable in a `static` initialiser.
    ///
    /// The buffer region is attached by [`crate::StdStreams`] on first access because it lives
    /// beside the FILE in the same value and a `const fn` cannot take a reference to a field
    /// it is still constructing (the index representation keeps the state free of such
    /// pointers until then).
    pub(crate) const fn standard(
        state: &'static State<E>,
        fd: i32,
        flags: u32,
        lbf: i32,
        probe: bool,
        buf_size: usize,
        backend: Backend,
    ) -> Self {
        Self {
            state,
            cell: FileCell::new(FileState {
                flags,
                rpos: 0,
                rend: 0,
                wpos: 0,
                wend: 0,
                wbase: 0,
                buf_size,
                lbf,
                fd,
                mode: 0,
                lock: AtomicI32::new(0),
                lockcount: 0,
                pipe_pid: 0,
                off: 0,
                getln_buf: None,
                mbstate: MbState::INITIAL,
                mbmode: raven_libc_ctype::MbMode::Utf8,
                backend,
                buffer: Buffer::Empty,
                probe,
                prev: None,
                next: None,
            }),
        }
    }

    /// The mutable state; the caller holds the FILE lock (or owns the stream).
    pub(crate) fn st(&self) -> &FileState<E> {
        self.cell.get()
    }

    /// The mutable state; the caller holds the FILE lock (or owns the stream).
    pub(crate) fn st_mut(&self) -> &mut FileState<E> {
        self.cell.get_mut()
    }

    /// The state environment.
    #[must_use]
    pub fn env(&self) -> &E {
        self.state.env()
    }

    /// The crate state this FILE belongs to.
    #[must_use]
    pub fn state(&self) -> &'static State<E> {
        self.state
    }

    /// The current `F_*` flags.
    #[must_use]
    pub fn flags(&self) -> u32 {
        self.st().flags
    }

    /// The fd, or `-1` for a memory or cookie stream.
    #[must_use]
    pub fn fd(&self) -> i32 {
        self.st().fd
    }

    /// Whether this is one of the permanent standard streams.
    #[must_use]
    pub fn is_perm(&self) -> bool {
        self.st().flags & F_PERM != 0
    }

    /// Assert the invariants of `donor-map/04` 4.8 (debug builds and every unit test).
    ///
    /// # Panics
    /// Panics on a violation; this is a test and debug aid, never a shipped error path.
    pub fn check_invariants(&self) {
        if !cfg!(debug_assertions) && !cfg!(test) {
            return;
        }
        let st = self.st();
        let region = UNGET.saturating_add(st.buf_size);
        let read_mode = st.rend != 0;
        let write_mode = st.wend != 0;
        assert!(
            !(read_mode && (st.wend != 0 || st.wpos != 0 || st.wbase != 0)),
            "read and write windows coexist"
        );
        assert!(
            !(write_mode && (st.rpos != 0 || st.rend != 0)),
            "write and read windows coexist"
        );
        assert!(st.rpos <= st.rend, "rpos past rend");
        assert!(st.rend <= region, "rend past the region");
        assert!(st.wend <= region, "wend past the region");
        assert!(st.wbase <= st.wpos, "wbase past wpos");
        assert!(st.rpos <= region, "rpos past the region");
        if st.buf_size == 0 {
            assert!(region == UNGET, "unbuffered region is not the UNGET area");
        }
        let lock = st.lock.load(core::sync::atomic::Ordering::Relaxed);
        assert!(lock >= 0, "lock is negative");
        assert!(
            st.lockcount == 0 || (lock & !MAYBE_WAITERS) == self.env().tid(),
            "lockcount owned by another thread"
        );
        assert!(
            st.mode == 0 || st.mode == -1 || st.mode == 1,
            "orientation is not 0/-1/1"
        );
        if matches!(st.backend, Backend::Closed) {
            assert!(st.buf_size == 0, "a closed FILE still has a buffer");
            assert!(st.fd == -1, "a closed FILE still has an fd");
        }
    }
}

#[cfg(test)]
#[path = "file_ut.rs"]
mod tests;
