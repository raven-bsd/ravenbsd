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
/// Console input is a COM1 device path; aarch64 reports `ENOSYS`.
#[cfg(target_arch = "x86_64")]
pub mod debug_read;
pub mod driver;
pub mod event;
pub mod futex;
pub mod handle_xfer;
/// Scheme IPC (`0x60..0x69`) is shared by x86-64 and aarch64.
pub mod ipc;
pub mod memory_ctl;
pub mod process_ctl;
pub mod thread;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;

use crate::process;

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
fn dispatch(frame: &SyscallFrame) -> Result<usize, Errno> {
    let number = usize::try_from(frame.rax).unwrap_or(usize::MAX);
    match Syscall::from_usize(number) {
        // EXIT is process-wide: every thread of the process terminates.
        Some(Syscall::EXIT) => process::exit_process(frame.rdi as i32),
        // C16: authorized termination (contract crate owns the number and wording).
        Some(Syscall::PROC_TERMINATE) => process_ctl::terminate(frame),
        Some(Syscall::PROC_NAMESPACE) => process_ctl::namespace(frame),
        Some(Syscall::FUTEX) => futex::dispatch(frame),
        #[cfg(target_arch = "x86_64")]
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

/// Rust side of the x86-64 `syscall` entry stub.
///
/// The assembly stub saves a TrapFrame-compatible register image, passes its address here, and
/// loads the return value from [`crate::arch::x86_64::trapframe::TrapFrame::rax`] after this
/// function returns. `syscall_return` is the last preemption point before `sysretq`.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(frame: &mut crate::arch::x86_64::trapframe::TrapFrame) {
    let registers = SyscallFrame {
        rax: frame.rax,
        rdi: frame.rdi,
        rsi: frame.rsi,
        rdx: frame.rdx,
        r10: frame.r10,
        r8: frame.r8,
        r9: frame.r9,
    };
    frame.rax = result_value(dispatch(&registers));
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
    let value = result_value(dispatch(&registers));
    if let Some(slot) = frame.x.get_mut(0) {
        *slot = value;
    }
    crate::sched::syscall_return();
}
