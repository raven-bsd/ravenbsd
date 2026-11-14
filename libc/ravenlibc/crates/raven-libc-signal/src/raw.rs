// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The audited raw module (DR-39): assembly entry points, TCB field views and frame copies.
//!
//! Every `unsafe` block names its invariant. The safe modules reach this module through the
//! narrow functions below; no other file in the crate is allowed to use `unsafe`.
//!
//! The TCB view functions take a thread pointer supplied by [`crate::SigKernel::thread_pointer`].
//! On the host the fake kernel supplies a pointer to a real `Pthread`; in production it is
//! `%fs:0` / `tpidr_el0` / `tp` through `raven-libc-sys`.

use core::sync::atomic::{AtomicUsize, Ordering};

use raven_libc_abi::structs::{JmpBuf, McontextT, SiginfoT, UcontextT};
use raven_libc_tls::raw as tls_raw;
use raven_libc_tls::tcb;

use crate::critical::SigThreadState;

#[cfg(target_arch = "aarch64")]
mod context_aarch64;
#[cfg(target_arch = "riscv64")]
mod context_riscv64;
#[cfg(target_arch = "x86_64")]
mod context_x86_64;

#[cfg(target_arch = "aarch64")]
mod entry_aarch64;
#[cfg(target_arch = "riscv64")]
mod entry_riscv64;
#[cfg(target_arch = "x86_64")]
mod entry_x86_64;

#[cfg(target_arch = "aarch64")]
mod setjmp_aarch64;
#[cfg(target_arch = "riscv64")]
mod setjmp_riscv64;
#[cfg(target_arch = "x86_64")]
mod setjmp_x86_64;

#[cfg(target_arch = "aarch64")]
mod sigsetjmp_aarch64;
#[cfg(target_arch = "riscv64")]
mod sigsetjmp_riscv64;
#[cfg(target_arch = "x86_64")]
mod sigsetjmp_x86_64;

/// Offset of `__fl` inside the 392-byte `jmp_buf` (`__jb` is 256 bytes).
pub const JMP_FL_OFFSET: usize = 256;
/// Offset of `__ss[0]`, the saved mask word.
pub const JMP_SS_OFFSET: usize = 264;
/// Offset of `__ss[1]`, the scratch callee-saved slot used by `sigsetjmp`.
pub const JMP_SS_SCRATCH: usize = 272;
/// Offset of our `__ss[2]`, the mask-word pointer saved by `sigsetjmp`.
pub const JMP_SS_MASK_PTR: usize = 280;

/// The context area is registered at `tp + SIG_CTX_AREA_OFFSET` (PROPOSED, RD-new 3).
pub const SIG_CTX_AREA_OFFSET: usize = 4096;

/// `Pthread` size sanity: the private signal block is 32 words.
const _: () = assert!(tcb::SIG_PRIVATE_BYTES == 256);

// ---------------------------------------------------------------------------------------------
// TCB words
// ---------------------------------------------------------------------------------------------

/// Read the thread's signal mask word.
#[must_use]
pub fn mask_load(tp: usize) -> u64 {
    // SAFETY: `tp` belongs to the calling thread and is a live `Pthread` (kernel/host contract).
    unsafe { tls_raw::with_pthread(tp, |pthread| pthread.sigmask.load(Ordering::Acquire)) }
}

/// Store the thread's signal mask word.
pub fn mask_store(tp: usize, value: u64) {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            pthread.sigmask.store(value, Ordering::Release);
        });
    }
}

/// Atomically OR the thread's signal mask word, returning the old value.
pub fn mask_fetch_or(tp: usize, bits: u64) -> u64 {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            pthread.sigmask.fetch_or(bits, Ordering::AcqRel)
        })
    }
}

/// Atomically AND the thread's signal mask word, returning the old value.
pub fn mask_fetch_and(tp: usize, bits: u64) -> u64 {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            pthread.sigmask.fetch_and(bits, Ordering::AcqRel)
        })
    }
}

/// Swap the thread's signal mask word.
pub fn mask_swap(tp: usize, value: u64) -> u64 {
    // SAFETY: as `mask_load`.
    unsafe { tls_raw::with_pthread(tp, |pthread| pthread.sigmask.swap(value, Ordering::AcqRel)) }
}

/// Read the thread-directed pending word.
#[must_use]
pub fn pending_load(tp: usize) -> u64 {
    // SAFETY: as `mask_load`.
    unsafe { tls_raw::with_pthread(tp, |pthread| pthread.sigpending.load(Ordering::Acquire)) }
}

/// Store the thread-directed pending word.
pub fn pending_store(tp: usize, value: u64) {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            pthread.sigpending.store(value, Ordering::Release);
        });
    }
}

/// OR a bit into the thread-directed pending word.
pub fn pending_or(tp: usize, bits: u64) -> u64 {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            pthread.sigpending.fetch_or(bits, Ordering::AcqRel)
        })
    }
}

/// AND the thread-directed pending word.
pub fn pending_and(tp: usize, bits: u64) -> u64 {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            pthread.sigpending.fetch_and(bits, Ordering::AcqRel)
        })
    }
}

/// The thread's kernel handle (0 when not assigned).
#[must_use]
pub fn thread_handle(tp: usize) -> u32 {
    // SAFETY: as `mask_load`.
    unsafe { tls_raw::with_pthread(tp, |pthread| pthread.handle) }
}

/// Read `(sp, size, flags)` of the thread's alternate stack.
#[must_use]
pub fn altstack_get(tp: usize) -> (usize, usize, i32) {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            (
                pthread.altstack_sp,
                pthread.altstack_size,
                pthread.altstack_flags,
            )
        })
    }
}

/// Store `(sp, size, flags)` of the thread's alternate stack.
pub fn altstack_set(tp: usize, sp: usize, size: usize, flags: i32) {
    // SAFETY: as `mask_load`.
    unsafe {
        tls_raw::with_pthread_mut(tp, |pthread| {
            pthread.altstack_sp = sp;
            pthread.altstack_size = size;
            pthread.altstack_flags = flags;
        });
    }
}

/// Execute `f` with the thread's [`SigThreadState`] view.
pub fn with_state<R>(tp: usize, f: impl FnOnce(&mut SigThreadState) -> R) -> R {
    // SAFETY: `sig_private` is 32 words (256 bytes), 8-aligned, and `SigThreadState` is
    // const-asserted to fit and to need no stronger alignment; the TCB outlives the call.
    unsafe {
        tls_raw::with_pthread_mut(tp, |pthread| {
            let pointer = pthread.sig_private.as_mut_ptr().cast::<SigThreadState>();
            f(&mut *pointer)
        })
    }
}

/// Store a `mcontext_t` into the registered kernel area.
pub fn store_ctx(tp: usize, machine: &McontextT) {
    let target = ctx_area(tp);
    // SAFETY: the registered area is a live writable `McontextT` (the host fake owns it; the
    // kernel contract promises the same in production).
    unsafe {
        core::ptr::copy_nonoverlapping(machine, target, 1);
    }
}

/// Read the registered kernel area.
#[must_use]
pub fn load_ctx(tp: usize) -> McontextT {
    // SAFETY: as `store_ctx`; the area is readable for one `McontextT`.
    unsafe { *ctx_area(tp) }
}

/// Store the one pending/deferred `SiginfoT` slot.
pub fn store_pending_info(tp: usize, info: &SiginfoT) {
    with_state(tp, |state| state.set_info(info));
}

/// Read the one pending/deferred `SiginfoT` slot.
#[must_use]
pub fn load_pending_info(tp: usize) -> SiginfoT {
    with_state(tp, |state| *state.info())
}

// ---------------------------------------------------------------------------------------------
// Kernel context area (PROPOSED RD-new 3: `mcontext_t` layout)
// ---------------------------------------------------------------------------------------------

/// The address of the thread's kernel-saved context area.
#[must_use]
pub fn ctx_area(tp: usize) -> *mut McontextT {
    let stored = with_state(tp, |state| state.ctx_area());
    if stored == 0 {
        tp.wrapping_add(SIG_CTX_AREA_OFFSET) as *mut McontextT
    } else {
        stored as *mut McontextT
    }
}

/// Register the context area address in the thread state.
pub fn set_ctx_area(tp: usize, address: usize) {
    with_state(tp, |state| state.set_ctx_area(address));
}

/// COPY-IN: copy the kernel area into a `ucontext_t`.
pub fn copy_ctx_in(tp: usize, context: &mut UcontextT) {
    let source = ctx_area(tp) as *const u8;
    // SAFETY: the area has `size_of::<McontextT>()` readable bytes (PG-7 contract) and the
    // destination is a live `ucontext_t` of at least the same size; the two never overlap.
    unsafe {
        core::ptr::copy_nonoverlapping(
            source,
            crate::frame::uc_mcontext_ptr_mut(context).cast::<u8>(),
            core::mem::size_of::<McontextT>(),
        );
    }
}

/// COPY-OUT: copy the (possibly handler-modified) context back to the kernel area.
pub fn copy_ctx_out(tp: usize, context: &UcontextT) {
    let target = ctx_area(tp) as *mut u8;
    // SAFETY: as `copy_ctx_in`; the area is free while the handler runs (COPY-IN rule).
    unsafe {
        core::ptr::copy_nonoverlapping(
            crate::frame::uc_mcontext_ptr(context).cast::<u8>(),
            target,
            core::mem::size_of::<McontextT>(),
        );
    }
}

/// Copy a caller-owned `mcontext_t` into a `ucontext_t`.
pub fn copy_mcontext_in(source: &McontextT, context: &mut UcontextT) {
    // SAFETY: both sides cover `size_of::<McontextT>()` bytes and are distinct objects; the
    // `ucontext_t` reserves the same layout (ABI const asserts).
    unsafe {
        core::ptr::copy_nonoverlapping(
            (source as *const McontextT).cast::<u8>(),
            crate::frame::uc_mcontext_ptr_mut(context).cast::<u8>(),
            core::mem::size_of::<McontextT>(),
        );
    }
}

/// Read the `mcontext_t` of a `ucontext_t`.
#[must_use]
pub fn read_mcontext(context: &UcontextT) -> McontextT {
    // SAFETY: the pointer addresses the machine-context area inside the `ucontext_t`; the
    // object is readable for the whole `McontextT` and correctly aligned by the ABI asserts.
    unsafe { *crate::frame::uc_mcontext_ptr(context) }
}

/// Write a whole `SigInfo` into the siginfo slot of a `ucontext_t`'s caller-provided record.
pub fn write_siginfo(target: &mut SiginfoT, info: &SiginfoT) {
    *target = *info;
}

// ---------------------------------------------------------------------------------------------
// Hooks set by the shell (C64); defaults make the crate usable alone
// ---------------------------------------------------------------------------------------------

/// `CancelHook` from C57 `on_kick`.
pub type CancelHook = fn(pc: &mut usize, bits: u32) -> bool;
/// `SynccallHook` from C57.
pub type SynccallHook = fn();
/// `TestCancelHook` from C57 `__testcancel`.
pub type TestCancelHook = fn();
/// Default stop action when PG-10 is absent.
pub type StopHook = fn(sig: i32);
/// Kept for symmetry with C41.
pub type ExitHook = fn(status: i32) -> !;

static CANCEL_HOOK: AtomicUsize = AtomicUsize::new(0);
static SYNCCALL_HOOK: AtomicUsize = AtomicUsize::new(0);
static TESTCANCEL_HOOK: AtomicUsize = AtomicUsize::new(0);
static STOP_HOOK: AtomicUsize = AtomicUsize::new(0);
static EXIT_HOOK: AtomicUsize = AtomicUsize::new(0);

/// Install (or clear) the cancellation hook.
pub fn set_cancel_hook(hook: Option<CancelHook>) {
    CANCEL_HOOK.store(hook.map_or(0, |f| f as usize), Ordering::Release);
}

/// Install the synccall hook.
pub fn set_synccall_hook(hook: Option<SynccallHook>) {
    SYNCCALL_HOOK.store(hook.map_or(0, |f| f as usize), Ordering::Release);
}

/// Install the `__testcancel` hook.
pub fn set_testcancel_hook(hook: Option<TestCancelHook>) {
    TESTCANCEL_HOOK.store(hook.map_or(0, |f| f as usize), Ordering::Release);
}

/// Install the stop hook.
pub fn set_stop_hook(hook: Option<StopHook>) {
    STOP_HOOK.store(hook.map_or(0, |f| f as usize), Ordering::Release);
}

/// Install the exit hook.
pub fn set_exit_hook(hook: Option<ExitHook>) {
    EXIT_HOOK.store(hook.map_or(0, |f| f as usize), Ordering::Release);
}

/// Run the cancellation hook; `false` when none is installed.
pub fn call_cancel_hook(pc: &mut usize, bits: u32) -> bool {
    let raw = CANCEL_HOOK.load(Ordering::Acquire);
    if raw == 0 {
        return false;
    }
    // SAFETY: only `set_cancel_hook` writes the cell, with the same `CancelHook` type.
    let hook: CancelHook = unsafe { core::mem::transmute(raw) };
    hook(pc, bits)
}

/// Run the synccall hook.
pub fn call_synccall_hook() {
    let raw = SYNCCALL_HOOK.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    // SAFETY: as `call_cancel_hook`.
    let hook: SynccallHook = unsafe { core::mem::transmute(raw) };
    hook();
}

/// Run the `__testcancel` hook.
pub fn call_testcancel_hook() {
    let raw = TESTCANCEL_HOOK.load(Ordering::Acquire);
    if raw == 0 {
        return;
    }
    // SAFETY: as `call_cancel_hook`.
    let hook: TestCancelHook = unsafe { core::mem::transmute(raw) };
    hook();
}

/// Run the stop hook.
pub fn call_stop_hook(sig: i32) {
    let raw = STOP_HOOK.load(Ordering::Acquire);
    if raw == 0 {
        if cfg!(debug_assertions) {
            raven_libc_sys::sys::debug_write(b"ravenlibc: stop signal ignored (PG-10 absent)\n")
                .ok();
        }
        return;
    }
    // SAFETY: as `call_cancel_hook`.
    let hook: StopHook = unsafe { core::mem::transmute(raw) };
    hook(sig);
}

/// Run the exit hook if installed (the entry path uses `SigKernel::terminate` otherwise).
pub fn call_exit_hook(status: i32) -> bool {
    let raw = EXIT_HOOK.load(Ordering::Acquire);
    if raw == 0 {
        return false;
    }
    // SAFETY: as `call_cancel_hook`.
    let hook: ExitHook = unsafe { core::mem::transmute(raw) };
    hook(status);
}

// ---------------------------------------------------------------------------------------------
// Assembly entry points
// ---------------------------------------------------------------------------------------------

unsafe extern "C" {
    fn __rl_setjmp(buffer: *mut u8) -> i32;
    fn __rl_longjmp(buffer: *const u8, value: i32) -> !;
    fn __rl_sigsetjmp(buffer: *mut u8, save: i32, mask: *mut u64) -> i32;
    fn __rl_getcontext(context: *mut u8) -> i32;
    fn __rl_setcontext(context: *const u8) -> !;
    fn __rl_swapcontext(old: *mut u8, new: *const u8) -> i32;
    fn __rl_call_handler(
        stack: usize,
        handler: usize,
        arg1: usize,
        arg2: usize,
        arg3: usize,
    ) -> usize;
    fn __rl_sigreturn_stub() -> !;
    fn __raven_sigentry();
    fn __rl_start_context();
    fn __rl_current_sp() -> usize;
    /// The function pointer the `__raven_sigentry` stub calls.
    static __rl_entry_fn: AtomicUsize;
}

/// Save the callee-saved context into `buffer`.
#[inline(always)]
pub fn setjmp_save(buffer: &mut JmpBuf) -> i32 {
    // SAFETY: `buffer` is a live 392-byte jmp_buf; the asm writes only the first 8 words and
    // the caller's frame stays alive across the matching `longjmp_restore`.
    unsafe { __rl_setjmp(buffer.bytes.as_mut_ptr().cast::<u8>()) }
}

/// Restore a context saved by [`setjmp_save`]; never returns.
#[inline(always)]
pub fn longjmp_restore(buffer: &JmpBuf, value: i32) -> ! {
    // SAFETY: `buffer` was filled by `setjmp_save` on a frame that is still alive (C contract).
    unsafe { __rl_longjmp(buffer.bytes.as_ptr().cast::<u8>(), value) }
}

/// `sigsetjmp` against a live mask reference.
#[inline(always)]
pub fn sigsetjmp_save(buffer: &mut JmpBuf, save: i32, mask: &mut u64) -> i32 {
    // SAFETY: the buffer is live and the mask reference outlives the jump target.
    unsafe {
        __rl_sigsetjmp(
            buffer.bytes.as_mut_ptr().cast::<u8>(),
            save,
            mask as *mut u64,
        )
    }
}

/// `sigsetjmp` against the calling thread's TCB mask (production path).
#[inline(always)]
pub fn sigsetjmp_current(buffer: &mut JmpBuf, save: i32) -> i32 {
    let tp = raven_libc_sys::tp::thread_pointer() as usize;
    let pointer = mask_word_ptr(tp);
    // SAFETY: `tp` is the calling thread's TCB and the mask pointer stays valid.
    unsafe { __rl_sigsetjmp(buffer.bytes.as_mut_ptr().cast::<u8>(), save, pointer) }
}

/// `getcontext` asm.
#[inline(always)]
pub fn getcontext_save(context: &mut UcontextT) -> i32 {
    // SAFETY: `context` is a live `ucontext_t`; the asm writes inside the object.
    unsafe { __rl_getcontext(crate::frame::uc_mcontext_ptr_mut(context).cast::<u8>()) }
}

/// `setcontext` asm; never returns.
#[inline(always)]
pub fn setcontext_restore(context: &UcontextT) -> ! {
    // SAFETY: the context is a live `ucontext_t` captured by `getcontext_save` or built by
    // `makecontext_core` with a valid stack and entry address.
    unsafe { __rl_setcontext(crate::frame::uc_mcontext_ptr(context).cast::<u8>()) }
}

/// `swapcontext` asm.
#[inline(always)]
pub fn swapcontext_save(old: &mut UcontextT, new: &UcontextT) -> i32 {
    // SAFETY: both contexts are live; the asm saves into `old` and branches into `new`.
    unsafe {
        __rl_swapcontext(
            crate::frame::uc_mcontext_ptr_mut(old).cast::<u8>(),
            crate::frame::uc_mcontext_ptr(new).cast::<u8>(),
        )
    }
}

/// Switch to `stack` (16-aligned) and call `handler(arg1, arg2, arg3)`, returning its result.
#[must_use]
pub fn call_handler(stack: usize, handler: usize, arg1: usize, arg2: usize, arg3: usize) -> usize {
    if handler <= 1 {
        return 0;
    }
    // SAFETY: `stack` is either the application's alternate stack (with room) or the interrupted
    // stack below its live frame; `handler` is the application function address.
    unsafe { __rl_call_handler(stack, handler, arg1, arg2, arg3) }
}

/// `SIGRETURN` (syscall 0x82, PROPOSED PG-7).
pub fn sigreturn() -> ! {
    // SAFETY: the kernel contract restores the context area and never returns here.
    unsafe { __rl_sigreturn_stub() }
}

/// Set the C-ABI function the `__raven_sigentry` stub calls (the shell's entry body).
pub fn set_entry_function(function: usize) {
    // SAFETY: the cell is defined by the linked asm object with the same type.
    unsafe { __rl_entry_fn.store(function, Ordering::Release) };
}

/// The registered entry function address (0 until the shell installs it).
#[must_use]
pub fn entry_function() -> usize {
    // SAFETY: as `set_entry_function`; the load is atomic.
    unsafe { __rl_entry_fn.load(Ordering::Acquire) }
}

/// The address of the assembly entry stub the kernel resumes at.
#[must_use]
pub fn sigentry_address() -> usize {
    __raven_sigentry as *const () as usize
}

/// The address of the `makecontext` start stub.
#[must_use]
pub fn start_context_address() -> usize {
    __rl_start_context as *const () as usize
}

/// A pointer to the thread's `sigmask` word (the asm `sigsetjmp` tail needs a plain address).
#[must_use]
pub fn mask_word_ptr(tp: usize) -> *mut u64 {
    // SAFETY: `tp` is a live `Pthread`; the field is inside it, 8-aligned and never moves.
    unsafe {
        tls_raw::with_pthread(tp, |pthread| {
            (&pthread.sigmask as *const core::sync::atomic::AtomicU64)
                .cast_mut()
                .cast::<u64>()
        })
    }
}

/// The stack pointer through the asm helper (a second implementation used by frame tests).
#[must_use]
pub fn stack_pointer_asm() -> usize {
    // SAFETY: reading the stack pointer has no memory effect.
    unsafe { __rl_current_sp() }
}

/// The calling thread's stack pointer (used by the `sigaltstack` checks).
#[must_use]
pub fn current_stack_pointer() -> usize {
    let value: usize;
    // SAFETY: reading the stack pointer has no effect on memory; `nomem`/`nostack` describe it.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!("mov {}, rsp", out(reg) value, options(nomem, nostack, preserves_flags));
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!("mov {}, sp", out(reg) value, options(nomem, nostack, preserves_flags));
        #[cfg(target_arch = "riscv64")]
        core::arch::asm!("mv {}, sp", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

const _: () = assert!(core::mem::size_of::<SigThreadState>() <= tcb::SIG_PRIVATE_BYTES);
const _: () =
    assert!(core::mem::align_of::<SigThreadState>() <= core::mem::align_of::<tcb::Pthread>());

#[cfg(test)]
#[path = "raw_ut.rs"]
mod raw_ut;
