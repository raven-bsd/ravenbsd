// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ravenlibc `FILE` layer.
//!
//! This crate is the stream object of the C library: the musl five-pointer buffer state
//! machine with indices instead of pointers, the per-stream recursive owner-tid lock, the
//! open-file list and its exit/fork hooks, the fd, memory and cookie backends, wide stdio,
//! `<stdio_ext.h>`, and the [`glue_io`] adapters that connect the pure `printf`/`scanf`
//! engines of `raven-libc-fmt` to a stream.
//!
//! Everything system-specific goes through the [`SysFile`] trait and every lock wait through
//! the [`Lock`] trait; the shell (`raven-libc`, C60) implements both and exports the C names.
//! The crate itself knows no Raven syscall number, no `errno` cell and no global allocator.
//!
//! `donor-map/04-stdio.md` sections 3.4-3.7 and 4.2 are the specification. The FILE core is
//! transpiled from `musl:src/stdio/*` (MIT via `musl/COPYRIGHT`, H-1); the buffering policy
//! (`buf.rs`) is CLEANROOM on top of the OpenBSD/NetBSD behaviour; `funopen`, the glue and
//! the wide wrappers are CLEANROOM. The layout is not ABI (A-stdio-17).

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

extern crate alloc;

#[allow(unsafe_code)]
mod raw;

pub(crate) mod backend;
pub(crate) mod backend_fd;

pub mod block_io;
pub mod buf;
pub mod char_io;
pub mod close;
pub mod closeall;
pub mod cookie;
pub mod errors;
pub mod exit;
pub mod ext;
pub mod file;
pub mod flush;
pub mod funopen;
pub mod glue_io;
pub mod line_io;
pub mod lock;
pub mod mem;
pub mod misc;
pub mod mode;
pub mod ofl;
pub mod open;
pub mod perror;
pub mod popen;
pub mod printf_glue;
pub mod scanf_glue;
pub mod seek;
pub mod setvbuf;
pub mod state;
pub mod status;
pub mod std_streams;
pub mod traits;
pub mod ungetc;
pub mod wide;

#[cfg(test)]
#[path = "api_ut.rs"]
mod api_ut;
#[cfg(test)]
pub mod fake_ut;

pub use crate::std_streams::StdStreams;
pub use block_io::{fread, fwrite};
pub use buf::{BUFSIZ, STD_BUF, UNBUF_PRINT_BUF};
pub use char_io::{
    fgetc, fgetc_unlocked, fputc, fputc_unlocked, getc, getc_unlocked, getchar, getchar_unlocked,
    putc, putc_unlocked, putchar, putchar_unlocked,
};
pub use close::fclose_ca;
pub use close::{fclose, fcloseall};
pub use cookie::fopencookie;
pub use errors::{EOF, Fail, R};
pub use exit::{fork_child, fork_parent, fork_prepare, stdio_cleanup};
pub use file::{File, UNGET};
pub use flush::{fflush, fflush_all, fpurge};
pub use funopen::funopen;
pub use glue_io::{FileSink, FileSource};
pub use line_io::{LineBuf, fgetln, fgets, fputs, getdelim, getline, puts};
pub use lock::{flockfile, ftrylockfile, funlockfile};
pub use mem::{fmemopen, open_memstream, open_wmemstream};
pub use misc::{remove, rename, tmpfile};
pub use open::{fdopen, fopen, fopen_rb_ca, freopen};
pub use perror::perror;
pub use popen::{check_pipe, close_sibling_pipes};
pub use printf_glue::{vasprintf_core, vdprintf_fd, vfprintf_file, vfwprintf_file};
pub use scanf_glue::{vfscanf_file, vfwscanf_file};
pub use seek::{fgetpos, fseek, fseeko, fsetpos, ftell, ftello, rewind};
pub use setvbuf::{setbuf, setbuffer, setlinebuf, setvbuf};
pub use state::State;
pub use status::{clearerr, feof, ferror, fileno};
pub use traits::{Env, Lock, StatLite, SysFile};

/// Borrow a FILE pointer returned by [`fopen`]/[`fdopen`]/the memory constructors.
///
/// The pointer must come from this crate and stay alive until the stream is closed; the
/// shell and the tests use it to recover the safe handle from the C-shaped return value.
#[must_use]
pub fn file_from_ptr<E: Env>(pointer: *mut File<E>) -> &'static File<E> {
    raw::file_ref(pointer)
}
pub use ungetc::ungetc;
pub use wide::{fgetwc, fgetws, fputwc, fputws, fwide, getwc, getwchar, putwc, putwchar, ungetwc};
