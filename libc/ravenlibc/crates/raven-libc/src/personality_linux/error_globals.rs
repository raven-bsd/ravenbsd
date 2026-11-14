// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The GNU `error.h` data symbols and their helpers.
//!
//! The three C data objects live one per file (`error_print_progname.rs`,
//! `error_message_count.rs`, `error_one_per_line.rs`) because `mkmap` K4 permits at most one
//! global data symbol per archive member; the ticket's `error_globals.rs` name is kept for the
//! shared helpers.

unsafe extern "C" {
    /// The full program name, filled by C61's start (C64 owns the definition).
    pub(crate) static program_invocation_name: *const u8;
}

/// The current program name (empty when start has not filled it yet).
pub(crate) fn program_name() -> alloc::vec::Vec<u8> {
    // SAFETY: the symbol is C61's pointer; NULL is allowed and reads as empty.
    let pointer = unsafe { core::ptr::read(core::ptr::addr_of!(program_invocation_name)) };
    if pointer.is_null() {
        return alloc::vec::Vec::new();
    }
    // SAFETY: the startup contract is a NUL-terminated string.
    unsafe { crate::ffi::cstr::slice(pointer.cast()) }.to_vec()
}

#[cfg(test)]
#[path = "error_globals_ut.rs"]
mod tests;
