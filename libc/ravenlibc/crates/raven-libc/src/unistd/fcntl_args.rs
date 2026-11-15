// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `fcntl(2)` variadic-argument classifier.
//!
//! The C prototype is variadic, but only some commands consume the third
//! argument. Reading a missing argument is undefined, so the trampoline must
//! know the command set before it touches the `va_list`. The table follows the
//! Linux command numbers generated in `raven-libc-abi`.

use raven_libc_abi::fcntl::{
    F_ADD_SEALS, F_CANCELLK, F_DUPFD, F_DUPFD_CLOEXEC, F_GET_FILE_RW_HINT, F_GET_RW_HINT, F_GETLK,
    F_GETOWN_EX, F_GETOWNER_UIDS, F_NOTIFY, F_OFD_GETLK, F_OFD_SETLK, F_OFD_SETLKW,
    F_SET_FILE_RW_HINT, F_SET_RW_HINT, F_SETFD, F_SETFL, F_SETLEASE, F_SETLK, F_SETLKW, F_SETOWN,
    F_SETOWN_EX, F_SETPIPE_SZ, F_SETSIG,
};

/// What the variadic third argument contains for a command.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Argument {
    /// The command takes no third argument.
    None,
    /// An integer (`int`, `long` or a bit mask).
    Scalar,
    /// A pointer to a caller-owned object (`struct flock`, owner record, ...).
    Pointer,
}

/// Classify one `fcntl` command.
#[must_use]
pub(crate) fn argument(command: i32) -> Argument {
    match command {
        F_DUPFD | F_SETFD | F_SETFL | F_SETOWN | F_SETSIG | F_SETOWN_EX | F_DUPFD_CLOEXEC
        | F_SETPIPE_SZ | F_ADD_SEALS | F_SETLEASE | F_NOTIFY => Argument::Scalar,
        F_GETLK | F_SETLK | F_SETLKW | F_OFD_GETLK | F_OFD_SETLK | F_OFD_SETLKW | F_GETOWN_EX
        | F_GETOWNER_UIDS | F_GET_RW_HINT | F_SET_RW_HINT | F_GET_FILE_RW_HINT
        | F_SET_FILE_RW_HINT | F_CANCELLK => Argument::Pointer,
        _ => Argument::None,
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "fcntl_args_ut.rs"]
mod tests;
