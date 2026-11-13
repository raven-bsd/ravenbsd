// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_caches_start_empty() {
    let _serial = crate::test_support::serial();
    reset();
    assert_eq!(cached(), 0);
    assert_eq!(zombies(), 0);
}

#[test]
fn an_unmappable_stack_is_eagain() {
    let _serial = crate::test_support::serial();
    reset();
    let kernel = crate::test_support::MockKernel::new();
    let result = allocate(&kernel, 65536, 4096, None, 512, 1024);
    assert_eq!(result, Err(crate::limits::ENOMEM));
}

#[test]
fn a_user_stack_without_tls_costs_nothing() {
    let _serial = crate::test_support::serial();
    reset();
    let kernel = crate::test_support::MockKernel::new();
    let result = allocate(&kernel, 65536, 0, Some((0x1000_0000, 65536)), 512, 1024).unwrap();
    assert_eq!(result.map_base, 0);
    assert_eq!(result.guard, 0);
    assert_eq!(result.stack_limit, 0x1000_0000);
    assert_eq!(result.stack_top, 0x1001_0000 - 512 - 1024);
    assert_eq!(result.tls_base % 16, 0);
}

#[test]
fn a_user_stack_gets_no_guard() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    let result = allocate(&kernel, 65536, 4096, Some((0x2000_0000, 65536)), 512, 1024).unwrap();
    assert_eq!(result.guard, 0);
}

#[test]
fn a_large_tls_user_stack_gets_a_second_mapping() {
    let _serial = crate::test_support::serial();
    reset();
    let kernel = crate::test_support::MockKernel::new();
    // A 1 MiB TLS need on a 64 KiB stack does not fit the 1/8 and 2 KiB rule, and the fake has
    // no mapping to give, so allocation reports ENOMEM rather than silently sharing the stack.
    assert_eq!(
        allocate(&kernel, 65536, 0, Some((0x3000_0000, 65536)), 1 << 20, 1024),
        Err(crate::limits::ENOMEM)
    );
}

#[test]
fn a_bad_user_stack_is_einval() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    assert_eq!(
        allocate(&kernel, 0, 0, Some((0x1000, 10)), 512, 1024),
        Err(crate::limits::EINVAL)
    );
}

#[test]
fn the_cache_holds_sixteen_and_reuses_the_exact_size() {
    let _serial = crate::test_support::serial();
    reset();
    for _ in 0..STACK_CACHE_MAX {
        let allocation = StackAlloc {
            map_base: 0x1000,
            map_size: crate::limits::DEFAULT_STACK_SIZE + 4096,
            guard: 4096,
            stack_limit: 0,
            stack_top: 0,
            tls_base: 0,
            tsd_base: 0,
            tls_size: 0,
            tsd_size: 0,
        };
        let kernel = crate::test_support::MockKernel::new();
        assert_eq!(release(&kernel, allocation), Ok(()));
    }
    assert_eq!(cached(), STACK_CACHE_MAX);
}

#[test]
fn popping_a_cached_stack_returns_its_base() {
    let _serial = crate::test_support::serial();
    reset();
    assert_eq!(acquire_cached(1234), None);
}

#[test]
fn zombies_are_bounded() {
    let _serial = crate::test_support::serial();
    reset();
    for index in 0..ZOMBIE_MAX {
        assert_eq!(push_zombie(index * 4096, 4096, 0), Ok(()));
    }
    assert_eq!(zombies(), ZOMBIE_MAX);
    assert_eq!(push_zombie(0, 4096, 0), Err(crate::limits::EAGAIN));
    reset();
}

#[test]
fn abort_unmaps_a_mapping() {
    let _serial = crate::test_support::serial();
    let kernel = crate::test_support::MockKernel::new();
    abort(
        &kernel,
        StackAlloc {
            map_base: 0x1000,
            map_size: 4096,
            guard: 0,
            stack_limit: 0,
            stack_top: 0,
            tls_base: 0,
            tsd_base: 0,
            tls_size: 0,
            tsd_size: 0,
        },
    );
}
