// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! System call dispatch shared by x86-64 and aarch64.
//!
//! The handler bodies speak one [`SyscallFrame`] with the historical x86-64 register names. Each
//! architecture marshals its entry frame into it: x86-64 takes `rax`/`rdi`/... straight from the
//! `syscall` stub, aarch64 maps the `svc #0` convention (`x8` number, `x0..x5` arguments) onto
//! `rax`/`rdi`/`rsi`/`rdx`/`r10`/`r8`/`r9`. The result travels back in the architecture's return
//! register.

pub mod basic;
/// Console input is a COM1/PL011 device path.
pub mod debug_read;
pub mod driver;
/// `DTRACE_*` provider control syscalls (S57).
pub mod dtrace;
/// K208 raw duplex endpoint pair/shutdown syscalls.
pub mod duplex;
pub mod event;
pub mod exec;
pub mod futex;
pub mod handle_xfer;
/// Scheme IPC (`0x60..0x69`) is shared by x86-64 and aarch64.
pub mod ipc;
/// Linux-personality trap routing to the per-process userspace handler (S41).
pub mod linux;
pub mod memory_ctl;
pub mod pipe;
pub mod priority;
pub mod process_ctl;
pub mod record_lock;
pub mod thread;
/// `TRACE_SET` (R-OBS-6): the calling thread's current trace id.
pub mod trace;
pub mod tty;

use core::sync::atomic::Ordering;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::Personality;

use crate::process;

/// Dispatch one syscall frame, accounting kernel time to the caller.
fn dispatch(frame: &SyscallFrame) -> Result<usize, Errno> {
    let start = crate::time::now_ns();
    let result = dispatch_inner(frame);
    if let Some(caller) = process::current() {
        let elapsed = crate::time::now_ns().saturating_sub(start);
        caller.system_ns.fetch_add(elapsed, Ordering::AcqRel);
    }
    result
}

/// Register view shared by every syscall handler.
///
/// The field names are the x86-64 ABI names because the handler bodies predate aarch64; the
/// aarch64 entry marshals `x8`/`x0..x5` into the same slots.
#[derive(Clone, Copy, Debug, Default)]
pub struct SyscallFrame {
    /// Syscall number.
    pub rax: u64,
    /// First argument.
    pub rdi: u64,
    /// Second argument.
    pub rsi: u64,
    /// Third argument.
    pub rdx: u64,
    /// Fourth argument.
    pub r10: u64,
    /// Fifth argument.
    pub r8: u64,
    /// Sixth argument.
    pub r9: u64,
}

/// Dispatch one syscall frame and return its raw result.
fn dispatch_inner(frame: &SyscallFrame) -> Result<usize, Errno> {
    // S41: a Linux-personality process never enters the native table. Route the
    // raw number to the process handler before interpreting any native meaning;
    // in particular Linux `write` (1) must not reach native `EXIT`.
    if let Some(caller) = process::current()
        && caller.personality.lock().personality == Personality::Linux
    {
        return match linux::trap(&caller, frame) {
            Ok(value) => Ok(value as usize),
            Err(error) => process::exit_process(error.to_ret() as i32),
        };
    }
    let number = usize::try_from(frame.rax).unwrap_or(usize::MAX);
    // S57: DTrace control calls are dispatched before the native table and are not traced
    // themselves; every other native call fires the syscall-entry probe first.
    if dtrace::is_control(number) {
        return dtrace::dispatch(number, frame);
    }
    dtrace::syscall_probe(frame);
    if number == process_ctl::PROC_FORK {
        return process_ctl::fork(frame);
    }
    if number == raven_abi::syscall::PROC_REBOOT {
        return process_ctl::reboot(frame);
    }
    if number == raven_abi::syscall::PROC_CHROOT {
        return process_ctl::chroot(frame);
    }
    if number == ipc::PREAD {
        return ipc::pread(frame);
    }
    if number == ipc::PWRITE {
        return ipc::pwrite(frame);
    }
    if number == ipc::SEEK {
        return ipc::seek(frame);
    }
    if number == raven_abi::syscall::MSYNC {
        return memory_ctl::msync(frame);
    }
    if number == exec::PROC_EXEC {
        return exec::exec(frame);
    }
    if number == duplex::DUPLEX_PAIR {
        return duplex::pair(frame);
    }
    if number == duplex::DUPLEX_SHUTDOWN {
        return duplex::shutdown(frame);
    }
    if number == duplex::DUPLEX_SENDMSG {
        return duplex::sendmsg(frame);
    }
    if number == duplex::DUPLEX_RECVMSG {
        return duplex::recvmsg(frame);
    }
    if number == duplex::DUPLEX_PEERCRED {
        return duplex::peercred(frame);
    }
    if number == pipe::PIPE {
        return pipe::pipe(frame);
    }
    if number == pipe::PIPE2 {
        return pipe::pipe2(frame);
    }
    if number == priority::GETPRIORITY {
        return priority::getpriority(frame);
    }
    if number == priority::SETPRIORITY {
        return priority::setpriority(frame);
    }
    if number == priority::NICE {
        return priority::nice(frame);
    }
    if number == priority::SCHED_GET_PRIORITY_MAX {
        return priority::sched_get_priority_max(frame);
    }
    if number == priority::SCHED_GET_PRIORITY_MIN {
        return priority::sched_get_priority_min(frame);
    }
    if number == priority::SCHED_SETSCHEDULER {
        return priority::sched_setscheduler(frame);
    }
    if number == priority::SCHED_GETSCHEDULER {
        return priority::sched_getscheduler(frame);
    }
    if number == tty::PROC_CTTY_SET {
        return tty::set_ctty(frame);
    }
    if number == tty::PROC_CTTY_GET {
        return tty::get_pgrp(frame);
    }
    if number == tty::PROC_CTTY_SET_FG {
        return tty::set_pgrp(frame);
    }
    if number == tty::PROC_CTTY_HANDLE {
        return tty::ctty_handle();
    }
    if number == process_ctl::PROC_SELF_INFO {
        return process_ctl::self_info(frame);
    }
    if number == process_ctl::PROC_SETPGID {
        return process_ctl::setpgid(frame);
    }
    if number == process_ctl::PROC_SETSID {
        return process_ctl::setsid();
    }
    if number == process_ctl::PROC_SETCRED {
        return process_ctl::setcred(frame);
    }
    if number == process_ctl::PROC_PID_INFO {
        return process_ctl::pid_info(frame);
    }
    if number == process_ctl::PROC_SET_COMMAND {
        return process_ctl::set_command(frame);
    }
    if number == process_ctl::PROC_SETCRED_EX {
        return process_ctl::setcred_ex(frame);
    }
    if number == process_ctl::PROC_HANDLE_INFO {
        return process_ctl::handle_info(frame);
    }
    if number == process_ctl::PROC_EXEC_CRED {
        return process_ctl::exec_cred(frame);
    }
    if number == process_ctl::PROC_CRED_INFO {
        return process_ctl::cred_info(frame);
    }
    if number == process_ctl::PROC_GETGROUPS {
        return process_ctl::getgroups(frame);
    }
    if number == process_ctl::PROC_WAIT_EX {
        return process_ctl::wait_ex(frame);
    }
    if number == process_ctl::PROC_GETPGID {
        return process_ctl::getpgid(frame);
    }
    if number == process_ctl::PROC_SIGNAL {
        return process_ctl::signal(frame);
    }
    if number == process_ctl::PROC_SIGRECV {
        return process_ctl::sigrecv(frame);
    }
    if number == process_ctl::PROC_SIGRECV_EX {
        return process_ctl::sigrecv_ex(frame);
    }
    if number == process_ctl::PROC_SIGQUEUE {
        return process_ctl::sigqueue(frame);
    }
    if number == process_ctl::PROC_TIMES {
        return process_ctl::times(frame);
    }
    if number == process_ctl::PROC_GETRUSAGE {
        return process_ctl::getrusage(frame);
    }
    if number == process_ctl::PROC_GETRLIMIT {
        return process_ctl::getrlimit(frame);
    }
    if number == process_ctl::PROC_SETRLIMIT {
        return process_ctl::setrlimit(frame);
    }
    if number == raven_abi::syscall::PROC_SET_PERSONALITY {
        return process_ctl::set_personality(frame);
    }
    if number == raven_abi::syscall::THREAD_SET_TLS {
        return thread::set_tls(frame);
    }
    if number == raven_abi::syscall::THREAD_KICK {
        return thread::kick(frame);
    }
    if number == raven_abi::syscall::THREAD_SELF {
        return thread::self_tid();
    }
    if number == raven_abi::syscall::ROBUST_LIST_SET {
        return thread::robust_list_set(frame);
    }
    if number == raven_abi::syscall::HANDLE_COPY {
        return handle_xfer::copy(frame);
    }
    if number == raven_abi::syscall::HANDLE_TRANSFER_FILE {
        return handle_xfer::transfer_to_file(frame);
    }
    if number == raven_abi::syscall::THREAD_EXIT_EX {
        thread::exit_ex(frame);
    }
    if number == raven_abi::signal::SIG_SETUP {
        return thread::sig_setup(frame);
    }
    if number == raven_abi::signal::SIGRESEND {
        return thread::sigresend(frame);
    }
    // C19: the kernel sigacts table (`sigaction`/`sigprocmask`/`sigaltstack`), raw constants
    // assigned by the lead in `raven_abi::syscall`.
    if number == raven_abi::syscall::SIGACTION {
        return thread::sigaction(frame);
    }
    if number == raven_abi::syscall::SIGPROCMASK {
        return thread::sigprocmask(frame);
    }
    if number == raven_abi::syscall::SIGALTSTACK {
        return thread::sigaltstack(frame);
    }
    // S72: wall-clock and entropy syscalls, dispatched by reviewer-assigned raw numbers that are
    // not yet `raven-abi` variants (see `basic::{GETRANDOM, CLOCK_SET}` for the required diff).
    if number == basic::GETRANDOM {
        return basic::getrandom(frame);
    }
    if number == basic::CLOCK_SET {
        return basic::clock_set(frame);
    }
    // Q3b: TRACE_SET (reviewer-assigned raw number 0x0B; not a Syscall variant).
    if number == raven_abi::syscall::TRACE_SET {
        return trace::trace_set(frame);
    }
    // P5c: RECORD_LOCK (reviewer-assigned raw number 0x0C; not a Syscall variant).
    if number == raven_abi::syscall::RECORD_LOCK {
        return record_lock::record_lock(frame);
    }
    match Syscall::from_usize(number) {
        // EXIT is process-wide: every thread of the process terminates.
        Some(Syscall::EXIT) => {
            if let Some(current) = process::current() {
                record_lock::release_process(current.id);
            }
            process::exit_process(frame.rdi as i32)
        }
        // C16: authorized termination (contract crate owns the number and wording).
        Some(Syscall::PROC_TERMINATE) => process_ctl::terminate(frame),
        Some(Syscall::PROC_NAMESPACE) => process_ctl::namespace(frame),
        Some(Syscall::FUTEX) => futex::dispatch(frame),
        Some(Syscall::DEBUG_READ) => debug_read::handle(frame),
        Some(Syscall::THREAD_CREATE) => thread::create(frame),
        Some(Syscall::THREAD_EXIT) => thread::exit(frame),
        Some(Syscall::PROC_CREATE) => process_ctl::create(frame),
        Some(Syscall::PROC_WAIT) => process_ctl::wait(frame),
        Some(Syscall::MEMOBJ_CREATE) => memory_ctl::memobj_create(frame),
        Some(Syscall::PAGER_SUPPLY) => memory_ctl::pager_supply(frame),
        Some(Syscall::MAP) => memory_ctl::map(frame),
        Some(Syscall::UNMAP) => memory_ctl::unmap(frame),
        Some(Syscall::PROTECT) => memory_ctl::protect(frame),
        Some(Syscall::HANDLE_TRANSFER) => handle_xfer::transfer(frame),
        Some(call @ (Syscall::MMIO_CREATE | Syscall::BOOT_BLOB)) => driver::dispatch(call, frame),
        Some(call @ (Syscall::DMA_ALLOC | Syscall::DMA_INFO)) => driver::dispatch(call, frame),
        Some(call @ (Syscall::IRQ_CREATE | Syscall::IRQ_WAIT | Syscall::IRQ_ACK)) => {
            driver::dispatch(call, frame)
        }
        Some(
            call @ (Syscall::EVQ_CREATE
            | Syscall::EVQ_CTL
            | Syscall::EVQ_WAIT
            | Syscall::TIMER_CREATE
            | Syscall::EVENT_CREATE
            | Syscall::EVENT_SIGNAL
            | Syscall::TIMER_SET),
        ) => event::dispatch(call, frame),
        Some(
            call @ (Syscall::SCHEME_REGISTER
            | Syscall::OPEN
            | Syscall::READ
            | Syscall::WRITE
            | Syscall::CLOSE
            | Syscall::DUP
            | Syscall::FSTAT
            | Syscall::CALL
            | Syscall::RECV
            | Syscall::SEND),
        ) => ipc::dispatch(call, frame),
        Some(call) => basic::dispatch(call, frame),
        None => Err(Errno::ENOSYS),
    }
}

/// Turn a dispatch result into the raw return value.
fn result_value(result: Result<usize, Errno>) -> u64 {
    match result {
        Ok(value) => value as isize as u64,
        Err(error) => error.to_ret() as u64,
    }
}

/// Install a successful exec's entry and stack on the saved user frame.
///
/// The old image is already gone; every general register is cleared so no old-pointer value
/// leaks into the new program. `_start` takes the C8 stack from `rsp`/`sp`, not from a GPR.
#[cfg(target_arch = "x86_64")]
fn apply_exec_context(
    frame: &mut crate::arch::x86_64::trapframe::TrapFrame,
    context: crate::process::exec::ExecContext,
) {
    frame.rip = context.entry;
    frame.rsp = context.stack;
    frame.rflags = 0x202;
    frame.rax = 0;
    frame.rbx = 0;
    frame.rcx = 0;
    frame.rdx = 0;
    frame.rsi = 0;
    frame.rdi = 0;
    frame.rbp = 0;
    frame.r8 = 0;
    frame.r9 = 0;
    frame.r10 = 0;
    frame.r11 = 0;
    frame.r12 = 0;
    frame.r13 = 0;
    frame.r14 = 0;
    frame.r15 = 0;
}

/// Install a successful exec's entry and stack on the saved user frame.
#[cfg(target_arch = "aarch64")]
fn apply_exec_context(
    frame: &mut crate::arch::aarch64::vectors::TrapFrame,
    context: crate::process::exec::ExecContext,
) {
    frame.x = [0; 31];
    frame.sp = context.stack;
    frame.elr = context.entry;
    frame.spsr = 0;
    frame.esr = 0;
    frame.far = 0;
}

/// Rust side of the x86-64 `syscall` entry stub.
///
/// The assembly stub saves a TrapFrame-compatible register image, passes its address here, and
/// loads the return value from [`crate::arch::x86_64::trapframe::TrapFrame::rax`] after this
/// function returns. `syscall_return` is the last preemption point before `sysretq`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(frame: &mut crate::arch::x86_64::trapframe::TrapFrame) {
    // B21a: the Linux trap carries the full user frame; a handler may resume the thread at a
    // different RIP/RSP. The compact `dispatch` path remains for native calls.
    #[cfg(target_arch = "x86_64")]
    if let Some(caller) = process::current()
        && caller.personality.lock().personality == Personality::Linux
    {
        match linux::trap_x86_64(&caller, frame) {
            Ok(value) => frame.rax = value as u64,
            Err(error) => process::exit_process(error.to_ret() as i32),
        }
        crate::arch::x86_64::syscall::deliver_signal(frame);
        crate::sched::syscall_return();
        return;
    }
    let registers = SyscallFrame {
        rax: frame.rax,
        rdi: frame.rdi,
        rsi: frame.rsi,
        rdx: frame.rdx,
        r10: frame.r10,
        r8: frame.r8,
        r9: frame.r9,
    };
    if usize::try_from(registers.rax).ok() == Some(raven_abi::signal::SIGRETURN) {
        match crate::arch::x86_64::syscall::sigreturn_frame(frame) {
            Ok(()) => crate::arch::x86_64::syscall::deliver_signal(frame),
            Err(error) => frame.rax = error.to_ret() as u64,
        }
    } else {
        frame.rax = result_value(dispatch(&registers));
        if let Some(context) = process::exec::take_pending() {
            apply_exec_context(frame, context);
        } else {
            crate::arch::x86_64::syscall::deliver_signal(frame);
        }
    }
    crate::sched::syscall_return();
}

/// Dispatch an aarch64 `svc #0` (ESR `EC=0x15`) and publish the result in `x0`.
///
/// For an SVC, `ELR_EL1` already holds the address of the instruction after the `svc`, so the
/// vector returns there directly; advancing it here would skip one user instruction. The caller
/// has already verified the exception class.
#[cfg(target_arch = "aarch64")]
pub fn svc_dispatch(frame: &mut crate::arch::aarch64::vectors::TrapFrame) {
    let read = |index: usize| frame.x.get(index).copied().unwrap_or(0);
    let registers = SyscallFrame {
        rax: read(8),
        rdi: read(0),
        rsi: read(1),
        rdx: read(2),
        r10: read(3),
        r8: read(4),
        r9: read(5),
    };
    if usize::try_from(registers.rax).ok() == Some(raven_abi::signal::SIGRETURN) {
        match crate::arch::aarch64::vectors::sigreturn_frame(frame) {
            Ok(()) => crate::arch::aarch64::vectors::deliver_signal(frame),
            Err(error) => {
                if let Some(slot) = frame.x.get_mut(0) {
                    *slot = error.to_ret() as u64;
                }
            }
        }
    } else {
        let value = result_value(dispatch(&registers));
        if let Some(slot) = frame.x.get_mut(0) {
            *slot = value;
        }
        if let Some(context) = process::exec::take_pending() {
            apply_exec_context(frame, context);
        } else {
            crate::arch::aarch64::vectors::deliver_signal(frame);
        }
    }
    crate::sched::syscall_return();
}
