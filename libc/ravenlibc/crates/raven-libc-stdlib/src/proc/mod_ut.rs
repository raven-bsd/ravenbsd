// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The proc group surface.

use super::*;

#[test]
fn system_is_reexported() {
    struct Backend;
    impl SystemBackend for Backend {
        fn block(&mut self, _: &[Sig]) {}
        fn spawn_sh(&mut self, _: &[u8]) -> Result<i32, raven_libc_abi::errno::Errno> {
            Ok(1)
        }
        fn ignore(&mut self, _: &[Sig]) {}
        fn unblock_and_restore(&mut self) {}
        fn wait(&mut self, _: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
            Ok(0)
        }
    }
    assert_eq!(system(&mut Backend, None), 1);
}

#[test]
fn signals_are_distinct() {
    assert_ne!(Sig::Chld, Sig::Int);
    assert_ne!(Sig::Int, Sig::Quit);
}

#[test]
fn system_with_spawn_failure_returns_minus_one() {
    struct Backend;
    impl SystemBackend for Backend {
        fn block(&mut self, _: &[Sig]) {}
        fn spawn_sh(&mut self, _: &[u8]) -> Result<i32, raven_libc_abi::errno::Errno> {
            Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::ENOSYS as u16,
            ))
        }
        fn ignore(&mut self, _: &[Sig]) {}
        fn unblock_and_restore(&mut self) {}
        fn wait(&mut self, _: i32) -> Result<i32, raven_libc_abi::errno::Errno> {
            Ok(0)
        }
    }
    assert_eq!(system(&mut Backend, Some(b"x")), -1);
}
