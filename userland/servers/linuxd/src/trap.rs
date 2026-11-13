// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The trap codec: kernel `LinuxSyscallV1` in, dispatcher, `LinuxReplyV1` out.
//!
//! The kernel delivers one 58-byte request in a scheme `CALL` packet's inline data and expects a
//! 9-byte reply in the response inline data. The packet carries no sender identity, so a server
//! with more than one Linux process cannot attribute a trap; the skeleton refuses the second
//! process instead of guessing (`trap::SINGLE_PROCESS` in the report).

use raven_linux_dispatch::{Dispatcher, SyscallArgs};
use raven_proto_linux::{
    ARCH_X86_64, LinuxReplyV1, LinuxSyscallV1, ProtoError, REPLY_LEN, SYSCALL_LEN,
};

use crate::table::LinuxProcess;

/// What the server does after dispatching one trap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapAction {
    /// Send this value back to the process (`rax`).
    Reply(isize),
    /// The process asked to exit; reap it with this status.
    Exit(i32),
}

/// A malformed request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrapError {
    /// The wire message is malformed.
    Proto(ProtoError),
    /// The syscall number does not fit `usize`.
    NumberTooLarge,
}

/// Decode one request from the packet's inline data.
pub fn decode(request: &[u8]) -> Result<LinuxSyscallV1, TrapError> {
    let call = LinuxSyscallV1::decode(request).map_err(TrapError::Proto)?;
    if call.arch != ARCH_X86_64 {
        return Err(TrapError::Proto(ProtoError::BadArch));
    }
    Ok(call)
}

/// Number of user bytes the trap loop must copy in before dispatch.
///
/// Only `write` reads a user buffer in the skeleton (`buf`, `count`). The bound protects the server
/// from a hostile or corrupt Linux process.
pub fn copy_in_len(call: &LinuxSyscallV1) -> Option<usize> {
    if call.nr != 1 {
        return None;
    }
    let count = usize::try_from(call.args[2]).ok()?;
    Some(count.min(crate::table::USER_BUFFER_MAX))
}

/// Dispatch one decoded call and decide the server's next step.
pub fn dispatch(
    process: &mut LinuxProcess,
    dispatcher: &mut Dispatcher,
    call: &LinuxSyscallV1,
) -> TrapAction {
    let Ok(nr) = usize::try_from(call.nr) else {
        return TrapAction::Reply(-(raven_abi::errno::Errno::ENOSYS.0 as isize));
    };
    let args = SyscallArgs {
        nr,
        a: [
            call.args[0] as usize,
            call.args[1] as usize,
            call.args[2] as usize,
            call.args[3] as usize,
            call.args[4] as usize,
            call.args[5] as usize,
        ],
    };
    let value = dispatcher.dispatch(process, &args);
    match process.exit {
        Some(code) => TrapAction::Exit(code),
        None => TrapAction::Reply(value),
    }
}

/// Encode the reply; the caller puts it in the response inline data.
pub fn encode_reply(value: i64, out: &mut [u8]) -> usize {
    LinuxReplyV1::new(value).encode(out).unwrap_or(0)
}

/// Length of the encoded request and reply, for buffer sizing.
pub const fn lengths() -> (usize, usize) {
    (SYSCALL_LEN, REPLY_LEN)
}

#[cfg(test)]
#[path = "trap_ut.rs"]
mod tests;
