// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Best-effort UEFI console mirror for loader log lines.
//!
//! The polled UART (COM1/PL011) is the authoritative output; the UEFI `ConOut` copy is only a
//! mirror before `ExitBootServices`. `EFI_SIMPLE_TEXT_OUTPUT_PROTOCOL->OutputString` may return
//! `EFI_DEVICE_ERROR` on a console that is not ready, and a console write failure is not a loader
//! failure. FreeBSD's `cnputc` (`sys/kern/kern_cons.c:522`) is the same best-effort sink: it does
//! not abort the kernel on a console error.

use core::fmt::Write;

/// Write one line to a console sink, returning whether the sink accepted it.
///
/// A failed mirror is deliberately swallowed; the caller has already written the bytes to the
/// polled UART. `fmt::Error` is the only error a `core::fmt::Write` sink can report.
pub fn write_best_effort<W: Write>(writer: &mut W, text: &str) -> bool {
    writer.write_str(text).is_ok()
}

#[cfg(test)]
#[path = "console_mirror_ut.rs"]
mod tests;
