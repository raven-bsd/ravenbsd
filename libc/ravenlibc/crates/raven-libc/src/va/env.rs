// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! [`CEnv`]: the [`FmtEnv`] implementation over C memory.
//!
//! This is the only pointer-dereferencing code of the variadic module: the formatting engine
//! ([`raven_libc_fmt`]) never forms a pointer, so `%s`, `%n`, `%m` and the `scanf` stores all
//! arrive here as `usize` values plus a length (`donor-map/04` A-stdio-2, C59 section 4.7).
//!
//! Bounds and NULL rules: [`CEnv::cstr_len`] is `strnlen` and never reads past `max` (T17);
//! a NULL pointer argument is never dereferenced; a NULL store target is a no-op; `%m` text
//! and `%m` allocation come from the two function pointers the shell supplies (C60 passes
//! the real `strerror` text of C40 and `malloc`; tests pass fakes).
//!
//! Every block is `unsafe` and carries a `// SAFETY:` comment; the caller of a `FmtEnv`
//! method is the engine, which obtained each pointer value from the argument list.

use raven_libc_fmt::{FloatKind, FmtEnv, StoreSize, VaReader};

use crate::va::VaReaderExt;

/// A [`FmtEnv`] over one [`VaReaderExt`] reader and C memory.
pub struct CEnv<R: VaReaderExt> {
    /// The real `va_list` reader of the current architecture.
    pub reader: R,
    /// The `%m` message text: `fn(errnum, buf) -> len`, C40's `strerror` text in the shell.
    pub errno_text: fn(i32, &mut [u8; 64]) -> usize,
    /// The allocator `%m`/`%ms` uses; `malloc` in the shell, a fake in tests.
    pub alloc: fn(usize) -> *mut u8,
}

impl<R: VaReaderExt> CEnv<R> {
    /// Build an environment over `reader` and the two shell services.
    #[must_use]
    pub const fn new(
        reader: R,
        errno_text: fn(i32, &mut [u8; 64]) -> usize,
        alloc: fn(usize) -> *mut u8,
    ) -> Self {
        Self {
            reader,
            errno_text,
            alloc,
        }
    }

    /// The reader, for a test that wants to inspect the position.
    #[must_use]
    pub const fn reader(&self) -> &R {
        &self.reader
    }

    /// The reader, mutably.
    pub const fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

impl<R: VaReaderExt> VaReader for CEnv<R> {
    fn next_u64(&mut self) -> u64 {
        self.reader.next_u64()
    }

    fn next_f64_bits(&mut self) -> u64 {
        self.reader.next_f64_bits()
    }

    fn next_ld_bits(&mut self) -> u128 {
        self.reader.next_ld_bits()
    }

    fn clone_state(&self) -> Self {
        Self {
            reader: self.reader.clone_state(),
            errno_text: self.errno_text,
            alloc: self.alloc,
        }
    }
}

impl<R: VaReaderExt> FmtEnv for CEnv<R> {
    fn cstr_len(&mut self, ptr: usize, max: usize) -> usize {
        if ptr == 0 {
            return 0;
        }
        let base = ptr as *const u8;
        let mut len = 0usize;
        while len < max {
            // SAFETY: the engine guarantees that `ptr` points to at least `max` readable
            // bytes or to a NUL before them, and T17 caps the walk at `max`.
            let byte = unsafe { core::ptr::read(base.add(len)) };
            if byte == 0 {
                break;
            }
            len = len.wrapping_add(1);
        }
        len
    }

    fn read_bytes(&mut self, ptr: usize, offset: usize, buf: &mut [u8]) -> usize {
        let len = buf.len();
        if ptr == 0 || len == 0 {
            return 0;
        }
        let source = ptr.wrapping_add(offset) as *const u8;
        // SAFETY: the engine asks for the bytes it has already validated with `cstr_len` or
        // with the `%s` precision, so `len` bytes at `ptr + offset` are readable.
        unsafe { core::ptr::copy_nonoverlapping(source, buf.as_mut_ptr(), len) };
        len
    }

    fn wstr_len(&mut self, ptr: usize, max: usize) -> usize {
        if ptr == 0 {
            return 0;
        }
        let base = ptr as *const u32;
        let mut len = 0usize;
        while len < max {
            // SAFETY: as `cstr_len`, over 4-byte units; `max` caps the walk (T17).
            let unit = unsafe { core::ptr::read_unaligned(base.add(len)) };
            if unit == 0 {
                break;
            }
            len = len.wrapping_add(1);
        }
        len
    }

    fn read_wchars(&mut self, ptr: usize, offset: usize, buf: &mut [u32]) -> usize {
        if ptr == 0 {
            return 0;
        }
        let count = buf.len();
        let mut written = 0usize;
        while written < count {
            let source =
                ptr.wrapping_add(offset.wrapping_add(written).wrapping_mul(4)) as *const u32;
            // SAFETY: the engine asks for units it has validated with `wstr_len`; each is a
            // 4-byte little-endian unit.
            let unit = unsafe { core::ptr::read_unaligned(source) };
            let Some(slot) = buf.get_mut(written) else {
                break;
            };
            *slot = unit;
            written = written.wrapping_add(1);
        }
        written
    }

    fn store_count(&mut self, ptr: usize, size: StoreSize, value: i64) {
        if ptr == 0 {
            return;
        }
        let target = ptr as *mut u8;
        // SAFETY: a non-NULL `%n` target points to a writable object of the size the length
        // modifier names; the store is unaligned and truncating.
        unsafe {
            match size {
                StoreSize::Hh => core::ptr::write_unaligned(target.cast::<i8>(), value as i8),
                StoreSize::H => core::ptr::write_unaligned(target.cast::<i16>(), value as i16),
                StoreSize::Int => core::ptr::write_unaligned(target.cast::<i32>(), value as i32),
                StoreSize::L | StoreSize::Ll | StoreSize::J | StoreSize::Z | StoreSize::T => {
                    core::ptr::write_unaligned(target.cast::<i64>(), value);
                }
            }
        }
    }

    fn errno_text(&mut self, errnum: i32, buf: &mut [u8; 64]) -> usize {
        (self.errno_text)(errnum, buf)
    }

    fn store_int(&mut self, ptr: usize, size: StoreSize, value: u64) {
        if ptr == 0 {
            return;
        }
        let target = ptr as *mut u8;
        // SAFETY: a `scanf` destination points to a writable object of the conversion's size;
        // the store is unaligned and truncating.
        unsafe {
            match size {
                StoreSize::Hh => core::ptr::write_unaligned(target.cast::<u8>(), value as u8),
                StoreSize::H => core::ptr::write_unaligned(target.cast::<u16>(), value as u16),
                StoreSize::Int => core::ptr::write_unaligned(target.cast::<u32>(), value as u32),
                StoreSize::L | StoreSize::Ll | StoreSize::J | StoreSize::Z | StoreSize::T => {
                    core::ptr::write_unaligned(target.cast::<u64>(), value);
                }
            }
        }
    }

    fn store_chars(&mut self, ptr: usize, data: &[u8]) {
        if ptr == 0 {
            return;
        }
        // SAFETY: the engine passes a `scanf` string destination with at least `data.len()`
        // writable bytes (plus the terminator the engine stores separately).
        unsafe {
            core::ptr::copy_nonoverlapping(data.as_ptr(), ptr as *mut u8, data.len());
        }
    }

    fn store_wchars(&mut self, ptr: usize, data: &[u32]) {
        if ptr == 0 {
            return;
        }
        let mut index = 0usize;
        while index < data.len() {
            let Some(unit) = data.get(index) else {
                break;
            };
            let target = ptr.wrapping_add(index.wrapping_mul(4)) as *mut u32;
            // SAFETY: the wide destination has one 4-byte unit per element of `data`.
            unsafe { core::ptr::write_unaligned(target, *unit) };
            index = index.wrapping_add(1);
        }
    }

    fn store_float_bits(&mut self, ptr: usize, kind: FloatKind, bits: u128) {
        if ptr == 0 {
            return;
        }
        let target = ptr as *mut u8;
        match kind {
            // `FloatKind::F32` exists for `scanf` `%f` (a `float` destination), which C43's
            // engine stores through this method.
            FloatKind::F32 => {
                // SAFETY: the destination is a writable `float`.
                unsafe { core::ptr::write_unaligned(target.cast::<u32>(), bits as u32) };
            }
            FloatKind::F64 => {
                // SAFETY: the destination is a writable `double`.
                unsafe { core::ptr::write_unaligned(target.cast::<u64>(), bits as u64) };
            }
            FloatKind::LongDouble => store_long_double(target, bits),
        }
    }

    fn alloc_store(&mut self, dest_ptr: usize, bytes: &[u8], wide: bool) -> bool {
        if dest_ptr == 0 {
            return false;
        }
        let terminator = if wide { 4usize } else { 1usize };
        let Some(size) = bytes.len().checked_add(terminator) else {
            return false;
        };
        let memory = (self.alloc)(size);
        if memory.is_null() {
            return false;
        }
        // SAFETY: the allocator returned `size` writable bytes; the copy and the terminator
        // stay inside them; `dest_ptr` is a writable pointer object.
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), memory, bytes.len());
            core::ptr::write_bytes(memory.add(bytes.len()), 0, terminator);
            core::ptr::write_unaligned(dest_ptr as *mut *mut u8, memory);
        }
        true
    }
}

/// Store a `long double` image: 16 bytes on aarch64/riscv64, 10 meaningful bytes plus 6 zero
/// bytes on x86_64 (the x87 padding, `donor-map/04` 3.8).
#[cfg(target_arch = "x86_64")]
fn store_long_double(target: *mut u8, bits: u128) {
    // SAFETY: the destination is a writable `long double` (16 bytes on the stack).
    unsafe {
        core::ptr::write_unaligned(target.cast::<u64>(), bits as u64);
        core::ptr::write_unaligned(target.add(8).cast::<u16>(), (bits >> 64) as u16);
        core::ptr::write_bytes(target.add(10), 0, 6);
    }
}

/// Store a binary128 image unchanged (aarch64 and riscv64).
#[cfg(not(target_arch = "x86_64"))]
fn store_long_double(target: *mut u8, bits: u128) {
    // SAFETY: the destination is a writable 16-byte `long double`.
    unsafe { core::ptr::write_unaligned(target.cast::<u128>(), bits) };
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "env_ut.rs"]
mod tests;
