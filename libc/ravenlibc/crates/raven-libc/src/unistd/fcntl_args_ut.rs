// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn no_argument_commands_are_empty() {
    use raven_libc_abi::fcntl::{F_GET_SEALS, F_GETFD, F_GETFL, F_GETLEASE, F_GETPIPE_SZ};
    for command in [F_GETFD, F_GETFL, F_GETLEASE, F_GETPIPE_SZ, F_GET_SEALS] {
        assert!(argument(command) == Argument::None, "command {command}");
    }
}

#[test]
fn flag_and_dup_commands_read_a_scalar() {
    use raven_libc_abi::fcntl::{F_DUPFD, F_DUPFD_CLOEXEC, F_SETFD, F_SETFL};
    for command in [F_DUPFD, F_DUPFD_CLOEXEC, F_SETFD, F_SETFL] {
        assert!(argument(command) == Argument::Scalar, "command {command}");
    }
}

#[test]
fn lock_commands_read_a_pointer() {
    use raven_libc_abi::fcntl::{F_GETLK, F_OFD_GETLK, F_OFD_SETLK, F_SETLK, F_SETLKW};
    for command in [F_GETLK, F_SETLK, F_SETLKW, F_OFD_GETLK, F_OFD_SETLK] {
        assert!(argument(command) == Argument::Pointer, "command {command}");
    }
}

#[test]
fn an_unknown_command_reads_nothing() {
    assert!(argument(-1) == Argument::None);
    assert!(argument(9999) == Argument::None);
}
