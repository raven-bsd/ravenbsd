// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stack-buffered printing over `debug_write`.

use core::fmt;

use raven_abi::errno::Errno;

const BUFFER_SIZE: usize = 256;

/// Format into `args` and emit the bytes through `emit`, retrying partial writes.
pub(crate) fn write_formatted(
    args: fmt::Arguments<'_>,
    emit: &mut dyn FnMut(&[u8]) -> Result<usize, Errno>,
) {
    let mut writer = Writer {
        buffer: [0u8; BUFFER_SIZE],
        len: 0,
        emit,
    };
    let _ = fmt::write(&mut writer, args);
    writer.flush();
}

/// The runtime's internal print entry point used by the macros.
pub fn _print(args: fmt::Arguments<'_>) {
    write_formatted(args, &mut |bytes| crate::sys::debug_write(bytes));
}

struct Writer<'a> {
    buffer: [u8; BUFFER_SIZE],
    len: usize,
    emit: &'a mut dyn FnMut(&[u8]) -> Result<usize, Errno>,
}

impl Writer<'_> {
    fn push(&mut self, byte: u8) {
        if let Some(slot) = self.buffer.get_mut(self.len) {
            *slot = byte;
            self.len += 1;
        }
        if byte == b'\n' || self.len == BUFFER_SIZE {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.len == 0 {
            return;
        }
        let mut offset = 0usize;
        while offset < self.len {
            let Some(chunk) = self.buffer.get(offset..self.len) else {
                break;
            };
            match (self.emit)(chunk) {
                Ok(0) | Err(_) => break,
                Ok(written) if written <= chunk.len() => offset += written,
                Ok(_) => break,
            }
        }
        self.len = 0;
    }
}

impl fmt::Write for Writer<'_> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            self.push(byte);
        }
        Ok(())
    }
}

/// Print to the debug console.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        $crate::print::_print(core::format_args!($($arg)*))
    };
}

/// Print a line to the debug console.
#[macro_export]
macro_rules! println {
    () => {
        $crate::print::_print(core::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::print::_print(core::format_args!("{}\n", core::format_args!($($arg)*)))
    };
}

/// Print to the debug console (same console as `print!`).
#[macro_export]
macro_rules! eprint {
    ($($arg:tt)*) => {
        $crate::print::_print(core::format_args!($($arg)*))
    };
}

/// Print a line to the debug console (same console as `println!`).
#[macro_export]
macro_rules! eprintln {
    () => {
        $crate::print::_print(core::format_args!("\n"))
    };
    ($($arg:tt)*) => {
        $crate::print::_print(core::format_args!("{}\n", core::format_args!($($arg)*)))
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(args: fmt::Arguments<'_>) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        write_formatted(args, &mut |bytes| {
            chunks.push(bytes.to_vec());
            Ok(bytes.len())
        });
        chunks
    }

    #[test]
    fn plain_text_is_one_chunk() {
        assert_eq!(collect(format_args!("hello")), vec![b"hello".to_vec()]);
    }

    #[test]
    fn newline_flushes_immediately() {
        assert_eq!(
            collect(format_args!("a\nb")),
            vec![b"a\n".to_vec(), b"b".to_vec()]
        );
    }

    #[test]
    fn long_text_is_chunked_at_256_bytes() {
        let text = "x".repeat(300);
        let chunks = collect(format_args!("{text}"));
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].len(), 256);
        assert_eq!(chunks[1].len(), 44);
    }

    #[test]
    fn partial_writes_are_retried() {
        let mut chunks = Vec::new();
        write_formatted(format_args!("abcdef"), &mut |bytes| {
            chunks.push(bytes[..1].to_vec());
            Ok(1)
        });
        assert_eq!(chunks.concat(), b"abcdef");
    }

    #[test]
    fn errors_stop_emission_without_panicking() {
        let mut calls = 0;
        write_formatted(format_args!("abc"), &mut |_| {
            calls += 1;
            Err(Errno::EIO)
        });
        assert_eq!(calls, 1);
    }
}
