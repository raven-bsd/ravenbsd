// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The `FILE` adapters of the `printf`/`scanf` engines (`donor-map/04` R41, CLEANROOM).
//!
//! [`FileSink`] implements `Sink<u8>` and `Sink<u32>`: a buffered write is the unlocked
//! `__fwritex`; an **unbuffered** stream is not written byte by byte (A-stdio-12) — the sink
//! collects into [`crate::buf::UNBUF_PRINT_BUF`] (1024) bytes and issues one `write2` at
//! [`FileSink::finish`] or when the buffer fills, so one `fprintf(stderr, ...)` reaches the fd
//! atomically. [`FileSource`] implements `ScanSource<u8>`/`ScanSource<u32>` over
//! `getc_unlocked`/`ungetc` and `fgetwc_unlocked`/`ungetwc` with a consumed counter.
//!
//! Both run under the lock the **caller** holds and never take it (T20).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

// The sink's buffer offsets are fixed-size array indices.

use raven_libc_ctype::wcrtomb;
use raven_libc_fmt::{ScanSource, Sink, SinkError};

use crate::buf::UNBUF_PRINT_BUF;
use crate::file::File;
use crate::traits::Env;

/// The output side of the formatting engine over one FILE.
pub struct FileSink<'a, E: Env> {
    file: &'a File<E>,
    buffer: [u8; UNBUF_PRINT_BUF],
    used: usize,
    unbuffered: bool,
    failed: bool,
}

impl<'a, E: Env> FileSink<'a, E> {
    /// A sink for `file`; the caller holds the FILE lock and has switched the stream to write
    /// mode when that matters.
    pub fn new(file: &'a File<E>) -> Self {
        if file.st().wend == 0 {
            let _ = crate::mode::towrite(file);
        }
        let unbuffered = file.st().buf_size == 0;
        Self {
            file,
            buffer: [0; UNBUF_PRINT_BUF],
            used: 0,
            unbuffered,
            failed: false,
        }
    }

    /// Flush the local buffer of an unbuffered stream.
    ///
    /// # Errors
    /// [`SinkError`] when the descriptor write failed.
    pub fn finish(&mut self) -> Result<(), SinkError> {
        if !self.unbuffered || self.used == 0 {
            return if self.failed { Err(SinkError) } else { Ok(()) };
        }
        let used = self.used;
        self.used = 0;
        match self
            .file
            .env()
            .write2(self.file.st().fd, &[], &self.buffer[..used])
        {
            Ok(count) if count == used => Ok(()),
            _ => {
                self.file.st_mut().flags |= crate::file::F_ERR;
                self.failed = true;
                Err(SinkError)
            }
        }
    }

    /// Whether a write failed.
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Write bytes, buffering the unbuffered case.
    fn push(&mut self, data: &[u8]) -> Result<(), SinkError> {
        if !self.unbuffered {
            return match crate::block_io::fwritex(self.file, data) {
                Ok(count) if count == data.len() => Ok(()),
                Ok(_) | Err(_) => {
                    self.failed = true;
                    Err(SinkError)
                }
            };
        }
        let mut rest = data;
        while !rest.is_empty() {
            let space = UNBUF_PRINT_BUF.saturating_sub(self.used);
            if space == 0 {
                self.finish()?;
                continue;
            }
            let take = space.min(rest.len());
            if let Some(slot) = self.buffer.get_mut(self.used..self.used + take) {
                if let Some(source) = rest.get(..take) {
                    slot.copy_from_slice(source);
                }
            }
            self.used += take;
            rest = rest.get(take..).unwrap_or(&[]);
        }
        Ok(())
    }
}

impl<E: Env> Sink<u8> for FileSink<'_, E> {
    fn write(&mut self, s: &[u8]) -> Result<(), SinkError> {
        self.push(s)
    }

    fn pad(&mut self, ch: u8, n: usize) -> Result<(), SinkError> {
        const CHUNK: usize = 32;
        let block = [ch; CHUNK];
        let mut left = n;
        while left > 0 {
            let k = left.min(CHUNK);
            self.push(&block[..k])?;
            left -= k;
        }
        Ok(())
    }
}

impl<E: Env> Sink<u32> for FileSink<'_, E> {
    fn write(&mut self, s: &[u32]) -> Result<(), SinkError> {
        let mode = self.file.st().mbmode;
        let mut bytes = [0u8; 4];
        for &unit in s {
            match wcrtomb(mode, unit, &mut bytes) {
                Ok(len) => self.push(&bytes[..len])?,
                Err(_) => {
                    self.failed = true;
                    return Err(SinkError);
                }
            }
        }
        Ok(())
    }

    fn pad(&mut self, ch: u32, n: usize) -> Result<(), SinkError> {
        const CHUNK: usize = 32;
        let block = [ch; CHUNK];
        let mut left = n;
        while left > 0 {
            let k = left.min(CHUNK);
            self.write(&block[..k])?;
            left -= k;
        }
        Ok(())
    }
}

/// The input side of the `scanf` engine over one FILE.
pub struct FileSource<'a, E: Env> {
    file: &'a File<E>,
    consumed: usize,
}

impl<'a, E: Env> FileSource<'a, E> {
    /// A source over `file`; the caller holds the FILE lock.
    pub fn new(file: &'a File<E>) -> Self {
        Self { file, consumed: 0 }
    }

    /// Units consumed by successful `getc` calls (minus pushbacks).
    #[must_use]
    pub fn consumed(&self) -> usize {
        self.consumed
    }

    /// The FILE.
    #[must_use]
    pub fn file(&self) -> &'a File<E> {
        self.file
    }
}

impl<E: Env> ScanSource<u8> for FileSource<'_, E> {
    fn getc(&mut self) -> Option<u8> {
        match crate::char_io::getc_unlocked(self.file) {
            Ok(c) => {
                self.consumed = self.consumed.saturating_add(1);
                Some(c as u8)
            }
            Err(_) => None,
        }
    }

    fn ungetc(&mut self, c: u8) {
        if crate::ungetc::ungetc(self.file, i32::from(c)).is_ok() {
            self.consumed = self.consumed.saturating_sub(1);
        }
    }
}

impl<E: Env> ScanSource<u32> for FileSource<'_, E> {
    fn getc(&mut self) -> Option<u32> {
        match crate::wide::fgetwc_unlocked(self.file) {
            Ok(c) => {
                self.consumed = self.consumed.saturating_add(1);
                Some(c)
            }
            Err(_) => None,
        }
    }

    fn ungetc(&mut self, c: u32) {
        if crate::wide::ungetwc(self.file, c).is_ok() {
            self.consumed = self.consumed.saturating_sub(1);
        }
    }
}

#[cfg(test)]
#[path = "glue_io_ut.rs"]
mod tests;
