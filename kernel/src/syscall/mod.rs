// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! System call dispatch.

pub mod basic;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;

use crate::arch::x86_64::trapframe::TrapFrame;

/// Rust side of the `syscall` entry stub.
///
/// The assembly stub saves a TrapFrame-compatible register image, passes its address here, and
/// loads the return value from [`TrapFrame::rax`] after this function returns.
#[unsafe(no_mangle)]
pub extern "C" fn syscall_dispatch(frame: &mut TrapFrame) {
    let number = usize::try_from(frame.rax).unwrap_or(usize::MAX);
    let result = match Syscall::from_usize(number) {
        Some(call) => {
            basic::dispatch(call, frame).map_or_else(|error| error.to_ret(), |value| value as isize)
        }
        None => Errno::ENOSYS.to_ret(),
    };
    frame.rax = result as u64;
}
