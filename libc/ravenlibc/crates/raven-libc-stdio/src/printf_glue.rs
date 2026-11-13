// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `printf` glue: `vfprintf`, `vfwprintf`, `vdprintf` and `vasprintf` over the C43 engine.
//!
//! The engine is `raven-libc-fmt`; this module owns only the FILE-side rules of the donor
//! wrapper (`musl:src/stdio/vfprintf.c:664-704`, `vdprintf.c`, `vasprintf.c`, read for the
//! three ideas): a dry first pass validates positional arguments before a byte is written
//! (`vformat_dry` clones the reader), the stream's error flag is saved and restored around
//! the call, and an unbuffered stream is fed through [`crate::glue_io::FileSink`]'s 1024-byte
//! local buffer so the message leaves in one write (A-stdio-12). The glue holds the FILE lock
//! for the whole call and the sink never re-takes it (T20).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use alloc::vec::Vec;

use raven_libc_fmt::glue::vsnprintf_core;
use raven_libc_fmt::{
    CountingSink, FloatFmt, FmtEnv, FmtErr, FmtOptions, Sink, SinkError, vformat, vformat_dry,
};

use crate::buf::UNBUF_PRINT_BUF;
use crate::file::{F_ERR, File};
use crate::glue_io::FileSink;
use crate::lock::FileGuard;
use crate::traits::Env;

/// The FILE lock the glue holds around an engine call.
fn prepare_write<E: Env>(file: &File<E>) -> Result<FileGuard<'_, E>, FmtErr> {
    let guard = FileGuard::new(file);
    if file.st().mode == 1 {
        file.st_mut().flags |= F_ERR;
        return Err(FmtErr::Invalid);
    }
    if file.st().wend == 0 && crate::mode::towrite(file).is_err() {
        return Err(FmtErr::Sink);
    }
    Ok(guard)
}

/// `vfprintf`.
///
/// # Errors
/// The engine error, or [`FmtErr::Sink`] when a write failed.
pub fn vfprintf_file<E: Env, A: FmtEnv, F: FloatFmt>(
    file: &File<E>,
    fmt: &[u8],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<i32, FmtErr> {
    let _guard = prepare_write(file)?;
    let olderr = file.st().flags & F_ERR;
    file.st_mut().flags &= !F_ERR;
    vformat_dry(fmt, args)?;
    let mut sink = FileSink::new(file);
    let result = vformat(&mut sink, fmt, args, opts, floats);
    let finished = sink.finish();
    let had_error = file.st().flags & F_ERR != 0;
    file.st_mut().flags |= olderr;
    match result {
        Ok(value) if finished.is_err() || had_error => {
            let _ = value;
            Err(FmtErr::Sink)
        }
        other => other,
    }
}

/// `vfwprintf`.
///
/// # Errors
/// The engine error, or [`FmtErr::Sink`] when a write failed.
pub fn vfwprintf_file<E: Env, A: FmtEnv, F: FloatFmt>(
    file: &File<E>,
    fmt: &[u32],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<i32, FmtErr> {
    let _guard = FileGuard::new(file);
    if file.st().mode == -1 {
        file.st_mut().flags |= F_ERR;
        return Err(FmtErr::Invalid);
    }
    if file.st().mode == 0 {
        let _ = crate::wide::fwide(file, 1);
    }
    if file.st().wend == 0 && crate::mode::towrite(file).is_err() {
        return Err(FmtErr::Sink);
    }
    let olderr = file.st().flags & F_ERR;
    file.st_mut().flags &= !F_ERR;
    vformat_dry(fmt, args)?;
    let mut sink = FileSink::new(file);
    let result = vformat(&mut sink, fmt, args, opts, floats);
    let finished = sink.finish();
    let had_error = file.st().flags & F_ERR != 0;
    file.st_mut().flags |= olderr;
    match result {
        Ok(_) if finished.is_err() || had_error => Err(FmtErr::Sink),
        other => other,
    }
}

/// The sink of `vdprintf`: a 1024-byte buffer over a raw descriptor.
struct FdSink<'a, E: Env> {
    env: &'a E,
    fd: i32,
    buffer: [u8; UNBUF_PRINT_BUF],
    used: usize,
    failed: bool,
}

impl<'a, E: Env> FdSink<'a, E> {
    fn new(env: &'a E, fd: i32) -> Self {
        Self {
            env,
            fd,
            buffer: [0; UNBUF_PRINT_BUF],
            used: 0,
            failed: false,
        }
    }

    fn flush(&mut self) -> Result<(), SinkError> {
        if self.used == 0 {
            return if self.failed { Err(SinkError) } else { Ok(()) };
        }
        let used = self.used;
        self.used = 0;
        match self.env.write2(self.fd, &[], &self.buffer[..used]) {
            Ok(count) if count == used => Ok(()),
            _ => {
                self.failed = true;
                Err(SinkError)
            }
        }
    }
}

impl<E: Env> Sink<u8> for FdSink<'_, E> {
    fn write(&mut self, s: &[u8]) -> Result<(), SinkError> {
        let mut rest = s;
        while !rest.is_empty() {
            let space = UNBUF_PRINT_BUF.saturating_sub(self.used);
            if space == 0 {
                self.flush()?;
                continue;
            }
            let take = space.min(rest.len());
            self.buffer[self.used..self.used + take].copy_from_slice(&rest[..take]);
            self.used += take;
            rest = &rest[take..];
        }
        Ok(())
    }
}

/// `vdprintf`: format to a descriptor.
///
/// # Errors
/// The engine error or [`FmtErr::Sink`].
pub fn vdprintf_fd<E: Env, A: FmtEnv, F: FloatFmt>(
    env: &E,
    fd: i32,
    fmt: &[u8],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<i32, FmtErr> {
    vformat_dry(fmt, args)?;
    let mut sink = FdSink::new(env, fd);
    let result = vformat(&mut sink, fmt, args, opts, floats);
    let flushed = sink.flush();
    match result {
        Ok(_) if flushed.is_err() => Err(FmtErr::Sink),
        other => other,
    }
}

/// `vasprintf`: count, allocate, format again with a cloned reader (A-stdio-4).
///
/// The returned vector includes the NUL terminator.
///
/// # Errors
/// The engine error or [`FmtErr::NoMem`] when the result cannot be allocated.
pub fn vasprintf_core<A: FmtEnv, F: FloatFmt>(
    fmt: &[u8],
    args: &mut A,
    opts: &FmtOptions<'_>,
    floats: &F,
) -> Result<Vec<u8>, FmtErr> {
    let mut counter_args = args.clone_state();
    let mut counter = CountingSink::<u8>::new();
    let length = vformat(&mut counter, fmt, &mut counter_args, opts, floats)?;
    if length < 0 {
        return Err(FmtErr::Overflow);
    }
    let length = usize::try_from(length).map_err(|_| FmtErr::Overflow)?;
    let cap = length.checked_add(1).ok_or(FmtErr::Overflow)?;
    let mut buffer = Vec::new();
    if buffer.try_reserve_exact(cap).is_err() {
        return Err(FmtErr::NoMem);
    }
    buffer.resize(cap, 0);
    let mut formatter_args = args.clone_state();
    let written = vsnprintf_core(&mut buffer, cap, fmt, &mut formatter_args, opts, floats)?;
    if written < 0 {
        return Err(FmtErr::Overflow);
    }
    buffer.truncate(cap);
    Ok(buffer)
}

#[cfg(test)]
#[path = "printf_glue_ut.rs"]
mod tests;
