// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `backtrace` and `backtrace_symbols` text (donor-map 15 T5, 02 X53, doc 15 4.7).
//!
//! CLEANROOM frame-pointer walk (about 60 lines in the donor plan): no unwinder, no
//! `libgcc_s`, no `libunwind`. `backtrace` reads memory through the caller's `read_word`
//! closure, so the crate never dereferences a pointer; it stops at a NULL frame, an unaligned
//! frame, a non-increasing frame or `frames.len()`. `backtrace_symbols` formats one line per
//! frame as `"<object>(<symbol>+0x<offset>) [0x<address>]"`; the shell supplies `dladdr`.

use alloc::vec::Vec;

/// `backtrace`.
///
/// `first_fp` is the caller's frame pointer; `read_word` reads one machine word at an address
/// (`None` when the address is not mapped). Returns the number of frames stored.
pub fn backtrace(
    frames: &mut [usize],
    first_fp: usize,
    read_word: &mut dyn FnMut(usize) -> Option<usize>,
) -> usize {
    let word = core::mem::size_of::<usize>();
    let mut fp = first_fp;
    let mut previous = 0usize;
    let mut count = 0usize;
    while count < frames.len() && fp != 0 && fp.is_multiple_of(word) && fp > previous {
        let Some(next) = read_word(fp) else {
            break;
        };
        let Some(return_address) = read_word(fp.wrapping_add(word)) else {
            break;
        };
        if let Some(slot) = frames.get_mut(count) {
            *slot = return_address;
        }
        count = count.wrapping_add(1);
        previous = fp;
        fp = next;
    }
    count
}

#[allow(clippy::indexing_slicing)] // the digit is masked to four bits
fn push_hex(out: &mut Vec<u8>, value: usize) {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut started = false;
    for shift in (0..core::mem::size_of::<usize>()).rev() {
        let digit = (value >> shift.wrapping_mul(4)) & 0xf;
        if digit != 0 || started || shift == 0 {
            out.push(HEX[digit]);
            started = true;
        }
    }
}

/// `backtrace_symbols`: one text line per frame.
///
/// The format is `"<object>(<symbol>+0x<off>) [0x<addr>]"`; an empty object or symbol still
/// yields the parentheses and the bracket, exactly as the ticket's format string says.
pub fn format_symbol(out: &mut Vec<u8>, object: &[u8], symbol: &[u8], offset: usize, addr: usize) {
    out.extend_from_slice(object);
    out.push(b'(');
    out.extend_from_slice(symbol);
    out.extend_from_slice(b"+0x");
    push_hex(out, offset);
    out.extend_from_slice(b") [0x");
    push_hex(out, addr);
    out.push(b']');
}

/// One `backtrace_symbols` block: the lines separated by newlines, the way the shell's
/// `malloc`-ed string table holds them.
#[must_use]
pub fn format_symbols(frames: &[usize], object: &[u8], symbol: &[u8], offset: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for (index, &addr) in frames.iter().enumerate() {
        if index != 0 {
            out.push(b'\n');
        }
        format_symbol(&mut out, object, symbol, offset, addr);
    }
    out
}

#[cfg(test)]
#[path = "execinfo_ut.rs"]
mod tests;
