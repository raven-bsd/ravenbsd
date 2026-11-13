// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/internal/pthread_impl.h, src/thread/pthread_create.c, src/process/fork.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! The one audited module of `raven-libc-thread`.
//!
//! It owns the entry and TLS-descriptor assembly, the `&Atomic` views of user lock words, the
//! TCB and robust-list pointer chasing, the process exit hook and the production [`SysKernel`].
//! Every unsafe block names the invariant that makes it sound.

#![allow(clippy::arithmetic_side_effects)] // address arithmetic, checked at the call site

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicI32, AtomicU32};

use raven_libc_tls::Pthread;

use crate::kernel::{CreateArgs, KernelCaps, ThreadKernel};
use crate::limits::{EINVAL, ENOMEM, ENOSYS, Errno, NS_PER_SEC};

pub mod entry;
pub mod futex;
pub mod tlsdesc;

pub use futex::{as_u32, at_i32, at_u32, load_usize, store_usize};

use crate::libc_state::LIBC;

/// Run `f` with the `Pthread` of `tp`.
///
/// The kernel guarantees that a live thread's thread pointer addresses its TCB for as long as the
/// thread runs; the closure must not retain the reference after it returns.
pub fn with_current<R>(tp: usize, f: impl FnOnce(&Pthread) -> R) -> R {
    // SAFETY: `tp` is the calling thread's thread pointer and the TCB is live for the call.
    unsafe { raven_libc_tls::raw::with_pthread(tp, f) }
}

/// Run `f` with a mutable `Pthread` of `tp`.
///
/// The caller must hold the thread-list lock or own the thread exclusively.
pub fn with_current_mut<R>(tp: usize, f: impl FnOnce(&mut Pthread) -> R) -> R {
    // SAFETY: as `with_current`; the exclusive access is the caller's responsibility (a new TCB
    // before it starts, or the current thread's own TCB).
    unsafe { raven_libc_tls::raw::with_pthread_mut(tp, f) }
}

/// A shared reference to the TCB at `address`.
///
/// `address` must be a live `Pthread` (a `pthread_t` that passed the stale-tid checks).
#[must_use]
pub fn tcb(address: usize) -> &'static Pthread {
    debug_assert!(address != 0);
    // SAFETY: the caller passes a live TCB address; the reference lives until the thread dies,
    // and every user of it holds the thread-list lock or owns the thread.
    unsafe { &*(address as *const Pthread) }
}

/// An exclusive reference to the TCB at `address`; see [`tcb`].
#[must_use]
pub fn tcb_mut(address: usize) -> &'static mut Pthread {
    debug_assert!(address != 0);
    // SAFETY: the caller owns the TCB exclusively (a new thread before publication, or itself).
    unsafe { &mut *(address as *mut Pthread) }
}

/// The next thread in the circular list (under the list lock).
#[must_use]
pub fn next_thread(current: &Pthread) -> &'static Pthread {
    // SAFETY: the list lock is held and every node points at a live TCB; a thread unlinks itself
    // only while holding that lock.
    unsafe { &*current.next }
}

/// The previous thread in the circular list (under the list lock).
#[must_use]
pub fn prev_thread(current: &Pthread) -> &'static Pthread {
    // SAFETY: see [`next_thread`].
    unsafe { &*current.prev }
}

/// Store the list links of `current`.
pub fn set_links(current: &Pthread, prev: usize, next: usize) {
    // SAFETY: the list lock is held, so no other thread reads or writes these two words.
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).prev = prev as *mut Pthread;
        (*pointer).next = next as *mut Pthread;
    }
}

/// Read the robust-list head of `current`.
#[must_use]
pub fn robust_head(current: &Pthread) -> usize {
    current.robust_head
}

/// Store the robust-list head of `current`.
pub fn set_robust_head(current: &Pthread, head: usize) {
    // SAFETY: the robust list is only mutated by the owner thread (and read by its own exit
    // path); the shared reference is used as a handle, never concurrently.
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).robust_head = head;
    }
}

/// Store the robust-list pending field of `current`.
pub fn set_robust_pending(current: &Pthread, pending: usize) {
    // SAFETY: as [`set_robust_head`].
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).robust_pending = pending;
    }
}

/// Read a `usize` from an address the caller knows is mapped and valid.
#[must_use]
pub fn read_usize(address: usize) -> usize {
    // SAFETY: the caller passes an address of a live robust-list node or list head.
    unsafe { *(address as *const usize) }
}

/// Write a `usize` to an address the caller knows is mapped and valid.
pub fn write_usize(address: usize, value: usize) {
    // SAFETY: see [`read_usize`]; the caller has exclusive access to the word.
    unsafe {
        *(address as *mut usize) = value;
    }
}

/// Install the process exit hook (`fn(i32) -> !`).
pub fn install_exit_hook(hook: fn(i32) -> !) {
    LIBC.set_exit_hook(hook as usize);
}

/// Call the process exit hook; never returns.
pub fn run_exit_hook(code: i32) -> ! {
    let slot = LIBC.exit_hook();
    if slot == 0 {
        crate::libc_state::exit_hook_default(code)
    } else {
        // SAFETY: only `install_exit_hook` stores a non-zero value, and a `fn(i32) -> !` has the
        // same representation as a usize on all supported targets.
        let hook: fn(i32) -> ! = unsafe { core::mem::transmute(slot) };
        hook(code)
    }
}

/// A safe wrapper around the static TLS descriptor entry, for tests.
#[must_use]
pub fn tlsdesc_static(descriptor: &mut [usize; 2]) -> usize {
    // SAFETY: the caller passes a two-word descriptor; the asm reads word 1 only.
    unsafe { tlsdesc::static_entry(descriptor.as_mut_ptr()) }
}

/// A safe wrapper around the dynamic TLS descriptor entry, for tests.
#[must_use]
pub fn tlsdesc_dynamic(descriptor: &mut [usize; 2]) -> usize {
    // SAFETY: the descriptor is live and the thread pointer addresses a live DTV (the caller
    // installed it with `init_main_thread`).
    unsafe { tlsdesc::dynamic_entry(descriptor.as_mut_ptr()) }
}

/// The production `ThreadKernel`: every method that has no Raven primitive returns `ENOSYS` or
/// the documented fallback; no kernel flag is invented.
#[derive(Clone, Copy, Debug, Default)]
pub struct SysKernel;

impl SysKernel {
    /// The production kernel.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// What the current kernel actually provides: only PG-1 on the two register architectures,
    /// and even there multi-thread TLS is not scheduled yet, so the conservative answer is none.
    #[must_use]
    pub const fn caps_const() -> KernelCaps {
        KernelCaps::minimal()
    }
}

impl ThreadKernel for SysKernel {
    fn caps(&self) -> KernelCaps {
        Self::caps_const()
    }

    fn thread_pointer(&self) -> usize {
        raven_libc_sys::tp::thread_pointer() as usize
    }

    fn set_thread_pointer(&self, tp: usize) -> Result<(), Errno> {
        if !self.caps().tls {
            return Err(ENOSYS);
        }
        // SAFETY: the caller is initialising the calling thread's TCB at `tp`; the kernel keeps
        // it for the thread's lifetime (PG-1).
        unsafe { raven_libc_sys::tp::set_thread_pointer(tp as *mut u8) }
            .map_err(|error| Errno(error.0))
    }

    fn tid_self(&self) -> Result<i32, Errno> {
        Err(ENOSYS)
    }

    fn create(&self, _args: &CreateArgs) -> Result<u32, Errno> {
        // PG-2 (thread create, clear-on-exit) and a `Handle::SELF_PROC` accessor in C54 are both
        // absent. Refuse rather than start a thread that shares the creator's TLS block.
        Err(ENOSYS)
    }

    fn exit(&self, code: i32) -> ! {
        raven_libc_sys::sys::thread_exit(code)
    }

    fn futex_wait(
        &self,
        word: &AtomicU32,
        expected: u32,
        rel_ns: Option<u64>,
    ) -> Result<(), Errno> {
        // A-thr-5: `None` maps to the kernel's forever value; `Some(0)` is a caller bug.
        let timeout = rel_ns.unwrap_or(0);
        if rel_ns == Some(0) {
            return Err(EINVAL);
        }
        raven_libc_sys::sys::futex_wait(word, expected, timeout).map_err(|error| Errno(error.0))
    }

    fn futex_wake(&self, word: &AtomicU32, n: u32) -> u32 {
        raven_libc_sys::sys::futex_wake(word, n)
            .map(|count| count as u32)
            .unwrap_or(0)
    }

    fn futex_requeue(
        &self,
        _word: &AtomicU32,
        _wake: u32,
        _to: &AtomicU32,
        _max: u32,
    ) -> Result<u32, Errno> {
        Err(ENOSYS)
    }

    fn map(&self, len: usize) -> Result<usize, Errno> {
        raven_libc_sys::pages::SysPageProvider::new()
            .map_pages(len, 16, 3)
            .ok_or(ENOMEM)
    }

    fn unmap(&self, addr: usize, len: usize) -> Result<(), Errno> {
        if raven_libc_sys::pages::SysPageProvider::new().unmap(addr, len) {
            Ok(())
        } else {
            Err(EINVAL)
        }
    }

    fn protect_none(&self, addr: usize, len: usize) -> Result<(), Errno> {
        if raven_libc_sys::pages::SysPageProvider::new().protect(addr, len, 0) {
            Ok(())
        } else {
            Err(EINVAL)
        }
    }

    fn kick(&self, _handle: u32, _bits: u64) -> Result<(), Errno> {
        Err(ENOSYS)
    }

    fn yield_now(&self) {
        raven_libc_sys::sys::nanosleep(0).ok();
    }

    fn now_ns(&self, clock: u32) -> u64 {
        match raven_libc_sys::sys::clock_get(clock) {
            Ok(time) => {
                let sec = u64::try_from(time.tv_sec).unwrap_or(0);
                let nsec = u64::try_from(time.tv_nsec).unwrap_or(0);
                sec.saturating_mul(NS_PER_SEC).saturating_add(nsec)
            }
            Err(_) => 0,
        }
    }

    fn set_robust_list(&self, _head: usize) -> Result<(), Errno> {
        Err(ENOSYS)
    }

    fn start_trampoline(&self) -> fn(usize) -> ! {
        production_trampoline
    }
}

/// The production trampoline: the asm stub jumps here with the `StartArgs` address.
fn production_trampoline(args: usize) -> ! {
    crate::thread::thread_start(&SysKernel::new(), args)
}

const _: () = assert!(core::mem::size_of::<Pthread>() == 512);

#[cfg(test)]
#[path = "raw_ut.rs"]
mod tests;

/// A writable static read-write lock. A plain `RwLock` static would be placed in read-only
/// memory by the compiler (its fields are plain integers); this cell is the writable home of the
/// process-global key and creation-inhibit locks.
struct StaticRwLock(UnsafeCell<crate::rwlock::RwLock>);

// SAFETY: the embedded lock's word protocol serialises every writer and reader; the cell is
// only ever exposed as `&RwLock` and mutated through its atomics.
unsafe impl Sync for StaticRwLock {}

static KEY_LOCK_CELL: StaticRwLock = StaticRwLock(UnsafeCell::new(crate::rwlock::RwLock::new()));
static PTC_LOCK_CELL: StaticRwLock = StaticRwLock(UnsafeCell::new(crate::rwlock::RwLock::new()));

/// The process-wide key table lock.
#[must_use]
pub fn key_lock() -> &'static crate::rwlock::RwLock {
    // SAFETY: the cell holds a live `RwLock` for the process lifetime.
    unsafe { &*KEY_LOCK_CELL.0.get() }
}

/// The process-wide thread-creation inhibit lock.
#[must_use]
pub fn ptc_lock() -> &'static crate::rwlock::RwLock {
    // SAFETY: the cell holds a live `RwLock` for the process lifetime.
    unsafe { &*PTC_LOCK_CELL.0.get() }
}

/// Read a 32-bit word at a known-mapped address.
#[must_use]
pub fn read_u32(address: usize) -> u32 {
    // SAFETY: the caller passes the address of a live lock word.
    unsafe { *(address as *const u32) }
}

/// Write a 32-bit word at a known-mapped address.
pub fn write_u32(address: usize, value: u32) {
    // SAFETY: see [`read_u32`]; the caller has exclusive access to the word.
    unsafe {
        *(address as *mut u32) = value;
    }
}

/// The address of the robust-list head field of `current`.
#[must_use]
pub fn robust_head_addr(current: &Pthread) -> usize {
    current as *const Pthread as usize + core::mem::offset_of!(Pthread, robust_head)
}

/// Store the robust-list offset of `current`.
pub fn set_robust_off(current: &Pthread, offset: i32) {
    // SAFETY: the owner thread writes its own robust offset before publishing a node.
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).robust_off = offset;
    }
}

/// A shared view of a condition-variable waiter node on a waiting thread's stack.
///
/// The node is linked into the condition variable's list only while that thread is blocked, and
/// the list protocol guarantees that the signaller observes it before the owner returns.
#[must_use]
pub fn waiter(address: usize) -> &'static crate::cond::Waiter {
    // SAFETY: the address comes from a live `Waiter` on the waiting thread's stack and the
    // condition-variable list lock or barrier protocol keeps it alive for the access.
    unsafe { &*(address as *const crate::cond::Waiter) }
}

/// A mutable view of a waiter node; see [`waiter`].
#[must_use]
pub fn waiter_mut(address: usize) -> &'static mut crate::cond::Waiter {
    // SAFETY: the condition-variable list protocol gives this thread exclusive access (the node
    // is either still private to its owner or detached from the list).
    unsafe { &mut *(address as *mut crate::cond::Waiter) }
}

/// A shared view of a local reference counter addressed by a waiter's `notify` word.
#[must_use]
pub fn counter(address: usize) -> &'static core::sync::atomic::AtomicI32 {
    // SAFETY: the address was stored from a live `AtomicI32` on the signaller's stack, and the
    // signaller waits for the count to drain before returning.
    unsafe { &*(address as *const core::sync::atomic::AtomicI32) }
}

/// A shared view of a barrier instance on the instance owner's stack.
#[must_use]
pub fn barrier_instance(address: usize) -> &'static crate::barrier::Instance {
    // SAFETY: the barrier protocol keeps the instance alive until its `finished` count drains;
    // the address is only published in `_b_inst` for that interval.
    unsafe { &*(address as *const crate::barrier::Instance) }
}

/// A `StartArgs` view at `address`; the address was produced by `create`.
#[must_use]
pub fn start_args(address: usize) -> &'static crate::thread::create::StartArgs {
    // SAFETY: `create` carved the structure below the new thread's stack top and published it
    // before the kernel started the thread; it lives until the thread exits.
    unsafe { &*(address as *const crate::thread::create::StartArgs) }
}

/// Write the three words of a `StartArgs`.
pub fn write_start_args(address: usize, trampoline: usize, start: usize, arg: usize) {
    // SAFETY: the caller reserved 24 bytes at `address` below the new stack top.
    unsafe {
        let pointer = address as *mut crate::thread::create::StartArgs;
        (*pointer).trampoline = trampoline;
        (*pointer).start = start;
        (*pointer).arg = arg;
    }
}

/// Call a `StartFn` installed by `create`.
///
/// # Safety
///
/// `function` must be a valid pointer to a function with the `StartFn` signature that outlives
/// the call.
pub unsafe fn call_start(function: usize, arg: usize) -> usize {
    // SAFETY: the caller guarantees the function pointer; it was stored by `create` from a
    // monomorphised Rust `fn(usize) -> usize`.
    let start: crate::thread::create::StartFn = unsafe { core::mem::transmute(function) };
    start(arg)
}

/// Safe wrapper for `thread_start`: the function was stored by `create`.
#[must_use]
pub fn call_start_safe(function: usize, arg: usize) -> usize {
    // SAFETY: `create` stored a monomorphised `StartFn` and the thread owns the structure.
    unsafe { call_start(function, arg) }
}

/// Execute the initial TLS copy over a writable area.
///
/// The caller guarantees `mem..mem + layout.total` is writable and private.
pub fn init_tls(
    mem: usize,
    layout: &raven_libc_tls::StaticLayout,
    modules: &[raven_libc_tls::TlsModuleIn],
) -> Result<raven_libc_tls::init::CopyPlan, Errno> {
    // SAFETY: the caller guarantees the mapping; the module images are readable for their
    // filesz by the ELF program-header contract.
    unsafe {
        raven_libc_tls::raw::copy_tls(mem, layout, modules, core::mem::size_of::<Pthread>())
            .map_err(|error| Errno(error.errno()))
    }
}

/// The static `builtin_tls` buffer.
#[must_use]
pub fn builtin_tls() -> usize {
    raven_libc_tls::raw::BUILTIN_TLS.as_ptr() as usize
}

/// A shared `&AtomicU32` view of a lock word at a known-mapped address.
#[must_use]
pub fn atomic_u32_at(address: usize) -> &'static AtomicU32 {
    // SAFETY: the caller passes the address of a live 4-byte-aligned lock word.
    unsafe { &*(address as *const AtomicU32) }
}

/// The cleanup-list head of `current`.
#[must_use]
pub fn cancelbuf(current: &Pthread) -> usize {
    current.cancelbuf
}

/// Store the cleanup-list head of `current`.
pub fn set_cancelbuf(current: &Pthread, value: usize) {
    // SAFETY: the owner thread mutates its own cleanup list; no other thread reads it.
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).cancelbuf = value;
    }
}

/// A cleanup node at `address`.
#[must_use]
pub fn cleanup_node(address: usize) -> &'static crate::thread::cancel::Cleanup {
    // SAFETY: the list contains only live `Cleanup` nodes on waiting threads' stacks.
    unsafe { &*(address as *const crate::thread::cancel::Cleanup) }
}

/// The TSD value of `current` at `index`.
#[must_use]
pub fn tsd_value(current: &Pthread, index: usize) -> usize {
    if current.tsd == 0 || index >= crate::limits::PTHREAD_KEYS_MAX {
        0
    } else {
        read_usize(
            current
                .tsd
                .wrapping_add(index.wrapping_mul(core::mem::size_of::<usize>())),
        )
    }
}

/// Store the TSD value of `current` at `index`.
pub fn set_tsd_value(current: &Pthread, index: usize, value: usize) {
    if current.tsd != 0 && index < crate::limits::PTHREAD_KEYS_MAX {
        write_usize(
            current
                .tsd
                .wrapping_add(index.wrapping_mul(core::mem::size_of::<usize>())),
            value,
        );
    }
}

/// Store the `tsd_used` flag.
pub fn set_tsd_used(current: &Pthread, value: bool) {
    // SAFETY: the owner thread writes its own TSD-used flag.
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).tsd_used = value;
    }
}

/// Store the TSD array address.
pub fn set_tsd_base(current: &Pthread, value: usize) {
    // SAFETY: the TCB is not yet published (create) or is the owner's own (main init).
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        (*pointer).tsd = value;
    }
}

/// Call an encoded destructor pointer.
pub fn call_destructor(encoded: usize, value: usize) {
    // SAFETY: the key table stores only `fn(usize)` pointers under this encoding.
    let function: fn(usize) = unsafe { core::mem::transmute(encoded) };
    function(value);
}

/// Zero `len` bytes at a mapped address.
pub fn zero_bytes(address: usize, len: usize) {
    // SAFETY: the caller passes a mapped writable range.
    unsafe {
        core::ptr::write_bytes(address as *mut u8, 0, len);
    }
}

/// Reinterpret an `&AtomicU32` word as `&AtomicI32` (same size and alignment).
#[must_use]
pub fn as_i32(word: &AtomicU32) -> &'static AtomicI32 {
    // SAFETY: both atomics have identical size and alignment; the bit pattern is the value.
    unsafe { &*(word as *const AtomicU32 as *const AtomicI32) }
}

/// Write the 16-byte thread name.
pub fn set_name(current: &Pthread, name: &[u8]) {
    // SAFETY: the name field is a fixed array inside the live TCB; the caller owns the write.
    unsafe {
        let pointer = current as *const Pthread as *mut Pthread;
        for index in 0..16 {
            if let Some(slot) = (*pointer).name.get_mut(index) {
                *slot = 0;
            }
        }
        for (index, byte) in name
            .iter()
            .take(crate::thread::name::NAME_MAX_LEN)
            .enumerate()
        {
            if let Some(slot) = (*pointer).name.get_mut(index) {
                *slot = *byte;
            }
        }
    }
}

/// Copy the 16-byte thread name into `buffer`, NUL terminated; returns the stored length.
pub fn get_name(current: &Pthread, buffer: &mut [u8]) -> usize {
    let mut length = 0usize;
    for (index, byte) in current.name.iter().enumerate() {
        if *byte == 0 || index + 1 >= buffer.len() {
            break;
        }
        if let Some(slot) = buffer.get_mut(index) {
            *slot = *byte;
        }
        length = index.wrapping_add(1);
    }
    if let Some(slot) = buffer.get_mut(length) {
        *slot = 0;
    }
    length
}

/// Call a handler stored as an address (atfork and cleanup tables).
pub fn call_handler(address: usize) {
    // SAFETY: the table stores only `fn()` pointers under this encoding.
    let function: fn() = unsafe { core::mem::transmute(address) };
    function();
}

/// A `&'static AtomicI32` for an address registered by `fork::register_lock`.
#[must_use]
pub fn atomic_i32_static(address: usize) -> &'static AtomicI32 {
    // SAFETY: the registry stores `&'static AtomicI32` addresses and never removes an entry.
    unsafe { &*(address as *const AtomicI32) }
}

/// Re-copy the recorded TLS images into the calling thread's block.
pub fn reset_tls_now(tp: usize, modules: &[raven_libc_tls::TlsModuleIn]) {
    // SAFETY: `tp` is the calling thread's thread pointer and the images are readable for their
    // filesz by the ELF program-header contract.
    let _ = unsafe { raven_libc_tls::raw::reset_tls(tp, modules) };
}

/// A `Dtv` view at a published pointer.
#[must_use]
pub fn dtv_ref(address: usize) -> &'static raven_libc_tls::Dtv {
    // SAFETY: `tls_access` only calls this with a non-null DTV pointer published with Release,
    // which lives as long as the thread.
    unsafe { &*(address as *const raven_libc_tls::Dtv) }
}

/// Install a DTV pointer on a TCB with Release ordering (tests and the loader seam).
pub fn install_dtv_pointer(current: &Pthread, pointer: *mut raven_libc_tls::Dtv) {
    current
        .dtv
        .store(pointer, core::sync::atomic::Ordering::Release);
}

/// The creation-inhibit lock's current word (tests).
#[must_use]
pub fn ptc_state() -> i32 {
    ptc_lock()
        .lock_word()
        .load(core::sync::atomic::Ordering::Acquire)
}

/// The thread-list lock's current owner word (tests).
#[must_use]
pub fn list_state() -> i32 {
    crate::thread::list::owner()
}

/// Load a `usize` stored in two `AtomicU32` words.
#[must_use]
pub fn load_usize_atomic(words: &[AtomicU32], index: usize) -> usize {
    let low = u64::from(
        words
            .get(index)
            .map_or(0, |word| word.load(core::sync::atomic::Ordering::Acquire)),
    );
    let high = u64::from(
        words
            .get(index.wrapping_add(1))
            .map_or(0, |word| word.load(core::sync::atomic::Ordering::Acquire)),
    );
    (low | (high << 32)) as usize
}

/// Store a `usize` in two `AtomicU32` words.
pub fn store_usize_atomic(words: &[AtomicU32], index: usize, value: usize) {
    let bits = value as u64;
    if let Some(word) = words.get(index) {
        word.store(bits as u32, core::sync::atomic::Ordering::Release);
    }
    if let Some(word) = words.get(index.wrapping_add(1)) {
        word.store((bits >> 32) as u32, core::sync::atomic::Ordering::Release);
    }
}
