// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C-semantics layer over [`crate::compat::Heap`].
//!
//! The C-ABI shell exports these methods; every rule of doc 08 section 4.4 is implemented here and
//! tested one function per rule.

use crate::compat::checks::{MAX_ALLOC, align_ok, alignment_overflows, array_size, posix_align_ok};
use crate::compat::info::narrow;
use crate::compat::{Heap, Mallinfo, Mallinfo2, reallocf, recallocarray};
use crate::consts::{PAGE_SIZE, align_up};
use crate::errors::Message;
use crate::provider::Platform;
use crate::raw;

/// The C-semantics allocator front end.
pub struct Api<H: Heap, X: Platform> {
    heap: H,
    platform: X,
}

impl<H: Heap, X: Platform> Api<H, X> {
    /// Wrap a backend and its platform.
    pub const fn new(heap: H, platform: X) -> Self {
        Self { heap, platform }
    }

    /// The wrapped backend.
    #[must_use]
    pub const fn heap(&self) -> &H {
        &self.heap
    }

    fn fail(&self, errno: i32) -> usize {
        self.platform.set_errno(errno);
        0
    }

    /// `malloc(n)`.
    pub fn malloc(&self, n: usize) -> usize {
        if n > MAX_ALLOC {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let result = self.heap.malloc(n);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    /// `free(p)`; `NULL` is a no-op and leaves `errno` untouched.
    pub fn free(&self, p: usize) {
        if p == 0 {
            return;
        }
        self.heap.free(p);
    }

    /// `realloc(p, n)`; `realloc(p, 0)` returns a minimum allocation and does not free `p`.
    pub fn realloc(&self, p: usize, n: usize) -> usize {
        if p == 0 {
            return self.malloc(n);
        }
        if n == 0 {
            let result = self.heap.malloc(1);
            if result == 0 {
                self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
            }
            return result;
        }
        if n > MAX_ALLOC {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let result = self.heap.realloc(p, n);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    /// `calloc(n, m)`.
    pub fn calloc(&self, n: usize, m: usize) -> usize {
        if array_size(n, m).is_none() {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let result = self.heap.calloc(n, m);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    /// `reallocarray(p, n, m)`; overflow leaves `p` valid.
    pub fn reallocarray(&self, p: usize, n: usize, m: usize) -> usize {
        let Some(bytes) = array_size(n, m) else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        self.realloc(p, bytes)
    }

    /// `recallocarray(p, old_n, new_n, size)`.
    pub fn recallocarray(&self, p: usize, old_n: usize, new_n: usize, size: usize) -> usize {
        if p == 0 {
            return self.calloc(new_n, size);
        }
        let Ok((old_bytes, new_bytes)) = recallocarray::recalloc_sizes(old_n, new_n, size) else {
            let errno = if array_size(new_n, size).is_none() {
                raven_libc_abi::errno::ENOMEM
            } else {
                raven_libc_abi::errno::EINVAL
            };
            return self.fail(errno);
        };
        let Some(old_bytes) = old_bytes else {
            return self.calloc(new_n, size);
        };
        if new_bytes <= old_bytes {
            let difference = old_bytes - new_bytes;
            if difference < old_bytes / 2 && difference < PAGE_SIZE {
                // SAFETY: `p` is a live allocation of `old_bytes` per the caller contract.
                raw::fill(p.wrapping_add(new_bytes), 0, difference);
                return p;
            }
        }
        let new_pointer = self.malloc(new_bytes);
        if new_pointer == 0 {
            return 0;
        }
        let copy = if new_bytes < old_bytes {
            new_bytes
        } else {
            old_bytes
        };
        raw::copy(new_pointer, p, copy);
        if new_bytes > old_bytes {
            raw::fill(
                new_pointer.wrapping_add(old_bytes),
                0,
                new_bytes - old_bytes,
            );
        }
        raw::fill_volatile(p, 0, old_bytes);
        self.free(p);
        new_pointer
    }

    /// `reallocf(p, n)`.
    pub fn reallocf(&self, p: usize, n: usize) -> usize {
        reallocf::reallocf(&self.heap, p, n)
    }

    /// `freezero(p, size)`.
    pub fn freezero(&self, p: usize, size: usize) {
        if p == 0 {
            return;
        }
        // SAFETY: the caller asserts `size` bytes are live at `p`.
        raw::fill_volatile(p, 0, size);
        self.free(p);
    }

    /// `posix_memalign(&p, align, size)`; returns the error number and never sets `errno`.
    pub fn posix_memalign(&self, out: &mut usize, align: usize, size: usize) -> i32 {
        if !posix_align_ok(align) {
            return raven_libc_abi::errno::EINVAL;
        }
        if size > MAX_ALLOC || alignment_overflows(align, size) {
            return raven_libc_abi::errno::ENOMEM;
        }
        let saved = self.platform.get_errno();
        let result = self.heap.memalign(align, size);
        self.platform.set_errno(saved);
        if result == 0 {
            return raven_libc_abi::errno::ENOMEM;
        }
        *out = result;
        0
    }

    /// `aligned_alloc(align, size)`; any size is accepted, `align` must be a power of two.
    pub fn aligned_alloc(&self, align: usize, size: usize) -> usize {
        if !align_ok(align) {
            return self.fail(raven_libc_abi::errno::EINVAL);
        }
        if size > MAX_ALLOC || alignment_overflows(align, size) {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let result = self.heap.memalign(align, size);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    /// `memalign(align, size)`; a non-power-of-two alignment is rounded up.
    pub fn memalign(&self, align: usize, size: usize) -> usize {
        if align == 0 {
            return self.fail(raven_libc_abi::errno::EINVAL);
        }
        let Some(rounded) = align.checked_next_power_of_two() else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        if size > MAX_ALLOC || alignment_overflows(rounded, size) {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        }
        let result = self.heap.memalign(rounded, size);
        if result == 0 {
            self.platform.set_errno(raven_libc_abi::errno::ENOMEM);
        }
        result
    }

    /// `valloc(size)`.
    pub fn valloc(&self, size: usize) -> usize {
        self.memalign(PAGE_SIZE, size)
    }

    /// `pvalloc(size)`.
    pub fn pvalloc(&self, size: usize) -> usize {
        let Some(rounded) = align_up(size, PAGE_SIZE) else {
            return self.fail(raven_libc_abi::errno::ENOMEM);
        };
        self.valloc(rounded)
    }

    /// `malloc_usable_size(p)`.
    pub fn malloc_usable_size(&self, p: usize) -> usize {
        if p == 0 { 0 } else { self.heap.usable_size(p) }
    }

    /// `mallinfo()`.
    #[must_use]
    pub fn mallinfo(&self) -> Mallinfo {
        narrow(self.heap.info())
    }

    /// `mallinfo2()`.
    #[must_use]
    pub fn mallinfo2(&self) -> Mallinfo2 {
        self.heap.info()
    }

    /// `mallopt(param, value)`; accepted numbers return 1 with no effect.
    pub fn mallopt(&self, param: i32, _value: i32) -> i32 {
        match param {
            1 | -1 | -2 | -3 | -5 | -6 | -7 | -8 | -100 | -101 | -102 => 1,
            _ => 0,
        }
    }

    /// `malloc_trim(pad)`.
    pub fn malloc_trim(&self, _pad: usize) -> i32 {
        i32::from(self.heap.trim())
    }

    /// `malloc_stats()`; output goes through `Platform::write_stderr`, never stdio.
    pub fn malloc_stats(&self) {
        let info = self.heap.info();
        let mut line = Message::new();
        line.push_str("Arena ");
        line.push_dec(info.arena);
        line.push(b'\n');
        line.push_str("in use bytes ");
        line.push_dec(info.uordblks);
        line.push(b'\n');
        line.push_str("system bytes ");
        line.push_dec(info.arena);
        line.push(b'\n');
        self.platform.write_stderr(line.as_bytes());
    }

    /// C23 `free_sized`.
    pub fn free_sized(&self, p: usize, size: usize) {
        self.heap.free_sized(p, size);
    }

    /// C23 `free_aligned_sized`.
    pub fn free_aligned_sized(&self, p: usize, align: usize, size: usize) {
        self.heap.free_aligned_sized(p, align, size);
    }

    /// `fork` prepare.
    pub fn prefork(&self) {
        self.heap.prefork();
    }

    /// `fork` parent.
    pub fn postfork_parent(&self) {
        self.heap.postfork_parent();
    }

    /// `fork` child.
    pub fn postfork_child(&self) {
        self.heap.postfork_child();
    }
}

#[cfg(test)]
#[path = "api_ut.rs"]
mod tests;
