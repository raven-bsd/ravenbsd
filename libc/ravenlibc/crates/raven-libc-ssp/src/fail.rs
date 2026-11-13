// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2002 Hiroaki Etoh, Federico G. Schwindt, and Miodrag Vallat.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/libc_stack_protector.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! The stack-smashing failure paths.

/// The platform operations of the donor `__fail` sequence.
pub trait FailHooks {
    /// Block every signal except `SIGABRT`.
    fn block_signals_except_abort(&self);
    /// Log the message before the abort.
    fn log(&self, msg: &[u8]);
    /// Reset `SIGABRT` to `SIG_DFL`.
    fn reset_abort_to_default(&self);
    /// Raise `SIGABRT`.
    fn raise_abort(&self);
    /// `_exit(127)` after the raise returns.
    fn exit_now(&self, status: i32) -> !;
}

/// Message of `__stack_chk_fail`.
pub const MSG_STACK: &[u8] = b"stack overflow detected; terminated";
/// Message of `__chk_fail`.
pub const MSG_CHK: &[u8] = b"buffer overflow detected; terminated";

/// The `__fail` sequence: block, log, reset, raise, exit 127.
pub fn fail<H: FailHooks>(hooks: &H, message: &[u8]) -> ! {
    hooks.block_signals_except_abort();
    hooks.log(message);
    hooks.reset_abort_to_default();
    hooks.raise_abort();
    hooks.exit_now(127)
}

/// `__stack_chk_fail`.
pub fn stack_chk_fail<H: FailHooks>(hooks: &H) -> ! {
    fail(hooks, MSG_STACK)
}

/// `__chk_fail`.
pub fn chk_fail<H: FailHooks>(hooks: &H) -> ! {
    fail(hooks, MSG_CHK)
}

#[cfg(test)]
#[path = "fail_ut.rs"]
mod tests;
