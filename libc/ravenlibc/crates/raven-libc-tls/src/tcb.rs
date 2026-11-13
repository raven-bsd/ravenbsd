// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/internal/pthread_impl.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
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

//! The thread control block (`struct pthread`).
//!
//! `PART 1` is a compiler-visible ABI on x86-64: `self_ptr` at 0, the DTV pointer at 8, the
//! thread list links at 16/24, `sysinfo` at 32 and the stack canary at 40 (`%fs:0x28`). Fields
//! after `errno_val` are private to ravenlibc; the ticket assigns them no fixed offset and this
//! file orders them for packing so that the whole TCB stays within 512 bytes.
//!
//! Variant I (AArch64, RISC-V) places the structure below `tp` and moves `canary` and `dtv` to
//! the end, so `dtv` is the last word and `tp - 8` addresses it (`pthread_impl.h:64-67`).

use core::sync::atomic::{AtomicI32, AtomicPtr, AtomicU32, AtomicU64};

use crate::dtv::Dtv;

/// The thread is exiting; the joiner owns the mapping.
pub const DT_EXITED: i32 = 0;
/// The thread is in its exit sequence.
pub const DT_EXITING: i32 = 1;
/// The thread is joinable.
pub const DT_JOINABLE: i32 = 2;
/// The thread is detached and frees itself.
pub const DT_DETACHED: i32 = 3;

/// Size of the opaque signal-private block reserved for `raven-libc-signal` (C58).
pub const SIG_PRIVATE_WORDS: usize = 32;
/// Size of that block in bytes.
pub const SIG_PRIVATE_BYTES: usize = SIG_PRIVATE_WORDS * core::mem::size_of::<usize>();
/// Thread name capacity including the terminating NUL.
pub const NAME_LEN: usize = 16;

/// The ravenlibc thread control block.
///
/// Every field is described by the table of C57 section 4.2.2; the struct-level allow keeps the
/// ABI table in one place instead of repeating it on both cfg variants.
#[allow(missing_docs)]
#[cfg(target_arch = "x86_64")]
#[repr(C, align(16))]
pub struct Pthread {
    /// `%fs:0`; `init` writes the address of this field.
    pub self_ptr: usize,
    /// Published DTV pointer (`dlopen` stores a grown one with Release, A-tls-1).
    pub dtv: AtomicPtr<Dtv>,
    /// Previous thread in the circular list (under the list lock).
    pub prev: *mut Pthread,
    /// Next thread in the circular list (under the list lock).
    pub next: *mut Pthread,
    /// Always zero; kept for glibc-shaped compilers.
    pub sysinfo: usize,
    /// Stack protector value (`%fs:0x28`), seeded by C50.
    pub canary: usize,
    /// 30-bit thread id; zero while exiting, -1 in a fork child for dead threads.
    pub tid: AtomicI32,
    /// The `errno` cell; C54 reads it through `errno_offset_from_tp`.
    pub errno_val: i32,

    // The remaining fields have no ABI offset; they are grouped by size for packing.
    pub cancelbuf: usize,
    pub tsd: usize,
    pub robust_head: usize,
    pub robust_pending: usize,
    pub sigmask: AtomicU64,
    pub sigpending: AtomicU64,
    pub altstack_sp: usize,
    pub altstack_size: usize,
    pub stack_base: usize,
    pub stack_size: usize,
    pub guard_size: usize,
    pub map_base: usize,
    pub map_size: usize,
    pub startfn: usize,
    pub startarg: usize,
    pub result: usize,
    pub locale_ptr: usize,
    pub dlerror_buf: usize,

    pub detach_state: AtomicI32,
    pub cancel: AtomicU32,
    pub robust_off: i32,
    pub altstack_flags: i32,
    pub killlock: AtomicI32,
    pub exit_word: AtomicI32,
    pub join_word: AtomicI32,
    pub handle: u32,
    pub h_errno_val: i32,

    pub tsd_used: bool,
    pub atfork_depth: u8,

    pub name: [u8; NAME_LEN],
    pub sig_private: [usize; SIG_PRIVATE_WORDS],
}

/// The ravenlibc thread control block, variant I (AArch64 and RISC-V).
///
/// See the x86-64 variant and C57 section 4.2.2 for the field table.
#[allow(missing_docs)]
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
#[repr(C, align(16))]
pub struct Pthread {
    /// `tp - size_of::<Pthread>()`; compilers may read it on variant I.
    pub self_ptr: usize,
    /// Previous thread in the circular list (under the list lock).
    pub prev: *mut Pthread,
    /// Next thread in the circular list (under the list lock).
    pub next: *mut Pthread,
    /// Always zero; kept for glibc-shaped compilers.
    pub sysinfo: usize,
    /// 30-bit thread id; zero while exiting, -1 in a fork child for dead threads.
    pub tid: AtomicI32,
    /// The `errno` cell; `errno_offset_from_tp` is negative on variant I.
    pub errno_val: i32,

    pub cancelbuf: usize,
    pub tsd: usize,
    pub robust_head: usize,
    pub robust_pending: usize,
    pub sigmask: AtomicU64,
    pub sigpending: AtomicU64,
    pub altstack_sp: usize,
    pub altstack_size: usize,
    pub stack_base: usize,
    pub stack_size: usize,
    pub guard_size: usize,
    pub map_base: usize,
    pub map_size: usize,
    pub startfn: usize,
    pub startarg: usize,
    pub result: usize,
    pub locale_ptr: usize,
    pub dlerror_buf: usize,

    pub detach_state: AtomicI32,
    pub cancel: AtomicU32,
    pub robust_off: i32,
    pub altstack_flags: i32,
    pub killlock: AtomicI32,
    pub exit_word: AtomicI32,
    pub join_word: AtomicI32,
    pub handle: u32,
    pub h_errno_val: i32,

    pub tsd_used: bool,
    pub atfork_depth: u8,

    pub name: [u8; NAME_LEN],
    pub sig_private: [usize; SIG_PRIVATE_WORDS],

    /// Stack protector value; second-to-last so `dtv` sits at `tp - 8`.
    pub canary: usize,
    /// Published DTV pointer; the last field.
    pub dtv: AtomicPtr<Dtv>,
}

impl Pthread {
    /// A zeroed TCB with `self_ptr` and the joinable state set.
    ///
    /// The caller still owns the list links, the robust-list self pointer and the TLS image; this
    /// constructor only gives safe code a valid starting value.
    #[must_use]
    pub const fn new(self_ptr: usize) -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self {
                self_ptr,
                dtv: AtomicPtr::new(core::ptr::null_mut()),
                prev: core::ptr::null_mut(),
                next: core::ptr::null_mut(),
                sysinfo: 0,
                canary: 0,
                tid: AtomicI32::new(0),
                errno_val: 0,
                cancelbuf: 0,
                tsd: 0,
                robust_head: 0,
                robust_pending: 0,
                sigmask: AtomicU64::new(0),
                sigpending: AtomicU64::new(0),
                altstack_sp: 0,
                altstack_size: 0,
                stack_base: 0,
                stack_size: 0,
                guard_size: 0,
                map_base: 0,
                map_size: 0,
                startfn: 0,
                startarg: 0,
                result: 0,
                locale_ptr: 0,
                dlerror_buf: 0,
                detach_state: AtomicI32::new(DT_JOINABLE),
                cancel: AtomicU32::new(0),
                robust_off: 0,
                altstack_flags: 0,
                killlock: AtomicI32::new(0),
                exit_word: AtomicI32::new(0),
                join_word: AtomicI32::new(0),
                handle: 0,
                h_errno_val: 0,
                tsd_used: false,
                atfork_depth: 0,
                name: [0; NAME_LEN],
                sig_private: [0; SIG_PRIVATE_WORDS],
            }
        }
        #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
        {
            Self {
                self_ptr,
                prev: core::ptr::null_mut(),
                next: core::ptr::null_mut(),
                sysinfo: 0,
                tid: AtomicI32::new(0),
                errno_val: 0,
                cancelbuf: 0,
                tsd: 0,
                robust_head: 0,
                robust_pending: 0,
                sigmask: AtomicU64::new(0),
                sigpending: AtomicU64::new(0),
                altstack_sp: 0,
                altstack_size: 0,
                stack_base: 0,
                stack_size: 0,
                guard_size: 0,
                map_base: 0,
                map_size: 0,
                startfn: 0,
                startarg: 0,
                result: 0,
                locale_ptr: 0,
                dlerror_buf: 0,
                detach_state: AtomicI32::new(DT_JOINABLE),
                cancel: AtomicU32::new(0),
                robust_off: 0,
                altstack_flags: 0,
                killlock: AtomicI32::new(0),
                exit_word: AtomicI32::new(0),
                join_word: AtomicI32::new(0),
                handle: 0,
                h_errno_val: 0,
                tsd_used: false,
                atfork_depth: 0,
                name: [0; NAME_LEN],
                sig_private: [0; SIG_PRIVATE_WORDS],
                canary: 0,
                dtv: AtomicPtr::new(core::ptr::null_mut()),
            }
        }
    }

    /// `size_of::<Pthread>()`.
    #[must_use]
    pub const fn size() -> usize {
        core::mem::size_of::<Self>()
    }

    /// The thread pointer of this TCB (`TP_ADJ`, `pthread_impl.h:117-121`).
    #[must_use]
    pub fn thread_pointer(&self) -> usize {
        crate::tcb::pthread_to_tp(self as *const Self as usize)
    }

    /// The detach state word.
    #[must_use]
    pub fn detach_state(&self) -> i32 {
        self.detach_state
            .load(core::sync::atomic::Ordering::Acquire)
    }

    /// Publish a new detach state, returning the previous one.
    pub fn swap_detach_state(&self, new: i32, order: core::sync::atomic::Ordering) -> i32 {
        self.detach_state.swap(new, order)
    }

    /// Whether this TCB still has a live kernel thread identity.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        self.tid.load(core::sync::atomic::Ordering::Acquire) > 0
    }
}

/// `TP_ADJ(p)`: variant II returns the TCB address; variant I returns the TCB address plus the
/// structure size.
#[must_use]
pub const fn pthread_to_tp(pthread: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        pthread.wrapping_add(crate::consts::TP_OFFSET)
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        pthread
            .wrapping_add(core::mem::size_of::<Pthread>())
            .wrapping_add(crate::consts::TP_OFFSET)
    }
}

/// `__pthread_self()`: the TCB address for a thread pointer.
#[must_use]
pub const fn tp_to_pthread(tp: usize) -> usize {
    #[cfg(target_arch = "x86_64")]
    {
        tp.wrapping_sub(crate::consts::TP_OFFSET)
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        tp.wrapping_sub(core::mem::size_of::<Pthread>())
            .wrapping_sub(crate::consts::TP_OFFSET)
    }
}

/// The `errno` cell offset from `tp` (positive 52 on x86-64, negative on variant I).
#[must_use]
pub const fn errno_offset_from_tp() -> isize {
    #[cfg(target_arch = "x86_64")]
    {
        52
    }
    #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
    {
        core::mem::offset_of!(Pthread, errno_val) as isize
            - core::mem::size_of::<Pthread>() as isize
    }
}

/// The `errno` cell address for a thread pointer.
#[must_use]
pub fn errno_location(tp: usize) -> *mut i32 {
    tp.wrapping_add_signed(errno_offset_from_tp()) as *mut i32
}

// PART 1 offsets are ABI. Variant I has no compiler-visible offset other than `self_ptr`; the
// two words at the end are the documented variant-I ABI (`dtv` at `tp - 8`).
#[cfg(target_arch = "x86_64")]
const _: () = {
    assert!(core::mem::offset_of!(Pthread, self_ptr) == 0);
    assert!(core::mem::offset_of!(Pthread, dtv) == 8);
    assert!(core::mem::offset_of!(Pthread, prev) == 16);
    assert!(core::mem::offset_of!(Pthread, next) == 24);
    assert!(core::mem::offset_of!(Pthread, sysinfo) == 32);
    assert!(core::mem::offset_of!(Pthread, canary) == 40);
    assert!(core::mem::offset_of!(Pthread, tid) == 48);
    assert!(core::mem::offset_of!(Pthread, errno_val) == 52);
    assert!(core::mem::offset_of!(Pthread, errno_val) == 52);
    assert!(core::mem::align_of::<Pthread>() == 16);
    assert!(core::mem::size_of::<Pthread>() == 512);
};

#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const _: () = {
    assert!(core::mem::offset_of!(Pthread, self_ptr) == 0);
    assert!(core::mem::offset_of!(Pthread, prev) == 8);
    assert!(core::mem::offset_of!(Pthread, next) == 16);
    assert!(core::mem::offset_of!(Pthread, sysinfo) == 24);
    assert!(core::mem::offset_of!(Pthread, dtv) == 504);
    assert!(core::mem::offset_of!(Pthread, canary) == 496);
    assert!(core::mem::offset_of!(Pthread, dtv) + core::mem::size_of::<usize>() == 512);
    assert!(core::mem::align_of::<Pthread>() == 16);
    assert!(core::mem::size_of::<Pthread>() == 512);
};

const _: () = assert!(SIG_PRIVATE_BYTES >= 176);

/// `PTHREAD_KEYS_MAX` words per thread. The value is the C36/ABI value (`abi/limits.toml`).
pub const TSD_WORDS: usize = raven_libc_abi::limits::PTHREAD_KEYS_MAX as usize;
#[cfg(test)]
#[path = "tcb_ut.rs"]
mod tests;
