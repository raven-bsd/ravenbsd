// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Size probe: the heap state must stay within the ticket budgets.

mod common;

use common::{SpinLock, TestEntropy, TestPlatform};
use raven_libc_malloc::{Hardened, Tiny};

type HardenedHeap = Hardened<common::VecProvider, SpinLock, TestEntropy, TestPlatform>;
type TinyHeap = Tiny<common::VecProvider, SpinLock, TestPlatform>;

#[test]
fn hardened_state_is_below_sixteen_kib() {
    assert!(core::mem::size_of::<HardenedHeap>() < 16 * 1024);
}

#[test]
fn tiny_state_is_below_256_bytes() {
    assert!(core::mem::size_of::<TinyHeap>() <= 256);
}
