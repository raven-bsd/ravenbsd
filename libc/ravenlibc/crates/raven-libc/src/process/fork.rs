// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `fork(2)` on the kernel `PROC_FORK` primitive.
//!
//! The kernel clone is copy-on-write and creates no thread, so the caller has to start the
//! child's first thread itself. The naked entry captures the caller's callee-saved frame,
//! `PROC_FORK` clones it, and `THREAD_CREATE_EX` starts a child thread on that same cloned
//! frame with the caller's TLS base. The child trampoline returns zero from this same call;
//! the parent continues with the child pid.
use core::ffi::c_int;

use raven_abi::errno::Errno as AbiErrno;
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, ThreadCreateEx};
use raven_abi::types::Handle;

/// Raw `proc_fork(src, flags) -> child process handle` (kernel `process_ctl.rs`, S50b).
const PROC_FORK: usize = 0x39;
/// Raw `proc_handle_info(handle) -> pid`.
const PROC_HANDLE_INFO: usize = 0x3E;

/// Return the child pid on success, or `EINVAL` for a request the kernel cannot start.
fn check_request(entry: u64, sp: u64, fork_flags: u32) -> Result<(), AbiErrno> {
    if entry == 0 || sp == 0 || fork_flags != 0 {
        return Err(AbiErrno::EINVAL);
    }
    // The kernel's THREAD_CREATE contract requires a 16-byte aligned stack top.
    if sp & 0xf != 0 {
        return Err(AbiErrno::EINVAL);
    }
    Ok(())
}

fn handle_from(value: usize) -> Result<Handle, AbiErrno> {
    u32::try_from(value)
        .map(Handle)
        .map_err(|_| AbiErrno::EOVERFLOW)
}

/// Start the child thread on the cloned resume frame and return its pid.
///
/// The child process handle is closed by the caller whether or not the thread starts; the
/// kernel keeps the child alive through its own self handle and the scheduler's reference.
fn start_child(child: Handle, entry: u64, sp: u64, tp: u64) -> Result<c_int, AbiErrno> {
    // SAFETY: PROC_HANDLE_INFO takes one live process handle owned by the caller.
    let pid = raven_libc_sys::errno::ret_i32(unsafe {
        raven_libc_sys::arch::syscall1(PROC_HANDLE_INFO, child.0 as usize)
    })?;
    let extended = ThreadCreateEx {
        tp: tp as usize,
        tid_out: 0,
        clear_tid_addr: 0,
    };
    // SAFETY: THREAD_CREATE_EX reads the 24-byte `extended` payload for this call and starts the
    // child on the caller-provided entry and stack in the child's address space.
    let thread = handle_from(raven_libc_sys::errno::ret(unsafe {
        raven_libc_sys::arch::syscall6(
            Syscall::THREAD_CREATE as usize,
            child.0 as usize,
            entry as usize,
            sp as usize,
            0,
            THREAD_CREATE_EX,
            &extended as *const ThreadCreateEx as usize,
        )
    })?)?;
    // The thread runs independently once the kernel has enqueued it.
    let _ = raven_libc_sys::sys::handle_close(thread);
    Ok(pid)
}

/// Fork with an explicit resume frame; used by the naked entry below.
fn fork_at(entry: u64, sp: u64) -> Result<c_int, AbiErrno> {
    check_request(entry, sp, 0)?;
    // SAFETY: PROC_FORK takes the caller's self-process handle and an explicit zero flags word;
    // `syscall1` would leave the second argument register unspecified and the kernel rejects a
    // non-zero flag word with EINVAL. It returns a new handle owned by the caller.
    let child = handle_from(raven_libc_sys::errno::ret(unsafe {
        raven_libc_sys::arch::syscall2(PROC_FORK, Handle::SELF_PROC.0 as usize, 0)
    })?)?;
    let result = start_child(
        child,
        entry,
        sp,
        raven_libc_sys::tp::thread_pointer() as u64,
    );
    let _ = raven_libc_sys::sys::handle_close(child);
    result
}

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
extern "C" fn raven_fork_resume(sp: usize) -> c_int {
    let entry = raven_fork_child as *const () as u64;
    match fork_at(entry, sp as u64) {
        Ok(pid) => pid,
        Err(error) => {
            crate::ffi::errno::set_raw(i32::from(error.0));
            -1
        }
    }
}

/// Child-side fork hook: drop the inherited cached event queue.
///
/// One event queue allows one waiter. The child must create its own queue rather than share the
/// parent's cached handle; the next `poll` creates it lazily.
extern "C" fn raven_fork_child_reset() {
    // SIG_SETUP is per kernel thread: the child's thread inherits the parent's TCB but not its
    // kernel entry. Clear the stale registration and, when the parent had one, register the
    // child's entry immediately so an inherited handler works before the next signal call.
    let _ = raven_libc_signal::raw::reset_after_fork(raven_libc_sys::tp::thread_pointer() as usize);
    let platform = crate::ffi::platform::global();
    platform.reset_event_queue();
    // The child keeps its inherited socket descriptors alive after the parent closes its copies.
    platform.fork_child_inherit();
    // The socket records cache the kernel handle; point them at the child's fresh handles so a
    // later `Call::Event`/`CALL` does not use the parent's released handle (K409 EBADF).
    crate::socket::refresh_after_fork();
}

/// `fork`.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn fork() -> c_int {
    core::arch::naked_asm!(
        // SysV: the caller's return address is at the entry SP; the six callee-saved
        // registers plus one pad keep the resume SP 16-byte aligned.
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "sub rsp, 8",
        "mov rdi, rsp",
        "call {resume}",
        "add rsp, 8",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
        resume = sym raven_fork_resume,
    );
}

/// The child's first instruction: return zero from the `fork` call site.
#[cfg(target_arch = "x86_64")]
#[unsafe(naked)]
extern "C" fn raven_fork_child() {
    core::arch::naked_asm!(
        "call {reset}",
        "xor eax, eax",
        "add rsp, 8",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "ret",
        reset = sym raven_fork_child_reset,
    );
}

/// `fork`.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn fork() -> c_int {
    core::arch::naked_asm!(
        // x19-x28, x29 (fp) and x30 (lr) are callee-saved; each pair keeps the SP aligned.
        "stp x29, x30, [sp, #-16]!",
        "stp x19, x20, [sp, #-16]!",
        "stp x21, x22, [sp, #-16]!",
        "stp x23, x24, [sp, #-16]!",
        "stp x25, x26, [sp, #-16]!",
        "stp x27, x28, [sp, #-16]!",
        "mov x0, sp",
        "bl {resume}",
        "ldp x27, x28, [sp], #16",
        "ldp x25, x26, [sp], #16",
        "ldp x23, x24, [sp], #16",
        "ldp x21, x22, [sp], #16",
        "ldp x19, x20, [sp], #16",
        "ldp x29, x30, [sp], #16",
        "ret",
        resume = sym raven_fork_resume,
    );
}

/// The child's first instruction: return zero from the `fork` call site.
#[cfg(target_arch = "aarch64")]
#[unsafe(naked)]
extern "C" fn raven_fork_child() {
    core::arch::naked_asm!(
        "bl {reset}",
        "mov x0, #0",
        "ldp x27, x28, [sp], #16",
        "ldp x25, x26, [sp], #16",
        "ldp x23, x24, [sp], #16",
        "ldp x21, x22, [sp], #16",
        "ldp x19, x20, [sp], #16",
        "ldp x29, x30, [sp], #16",
        "ret",
        reset = sym raven_fork_child_reset,
    );
}

/// Other architectures have no resume-frame thunk yet.
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
#[unsafe(no_mangle)]
pub extern "C" fn fork() -> c_int {
    crate::ffi::errno::set_raw(i32::from(AbiErrno::ENOSYS.0));
    -1
}

#[cfg(test)]
#[path = "fork_ut.rs"]
mod tests;
