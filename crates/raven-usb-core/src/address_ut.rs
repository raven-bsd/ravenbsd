// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the USB address pool.

use super::*;

#[test]
fn the_pool_assigns_the_lowest_free_address() {
    let mut pool = AddressPool::new();
    assert_eq!(pool.assign(), Ok(1));
    assert_eq!(pool.assign(), Ok(2));
    assert!(pool.is_assigned(1));
    assert!(!pool.is_assigned(3));
    assert_eq!(pool.assigned(), 2);
    assert_eq!(pool.release(1), Ok(()));
    assert_eq!(pool.assign(), Ok(1));
}

#[test]
fn reserved_and_released_addresses_are_checked() {
    let mut pool = AddressPool::new();
    assert_eq!(pool.reserve(7), Ok(()));
    assert!(pool.is_assigned(7));
    assert_eq!(pool.reserve(7), Err(AddressError::AlreadyAssigned(7)));
    assert_eq!(pool.release(7), Ok(()));
    assert_eq!(pool.release(7), Err(AddressError::NotAssigned(7)));
    assert_eq!(pool.release(DEFAULT_ADDRESS), Err(AddressError::Invalid(0)));
    assert_eq!(pool.release(128), Err(AddressError::Invalid(128)));
    assert_eq!(pool.reserve(0), Err(AddressError::Invalid(0)));
    assert!(!pool.is_assigned(0));
    assert!(!pool.is_assigned(200));
}

#[test]
fn the_pool_exhausts_after_127_addresses() {
    let mut pool = AddressPool::new();
    for address in 1..=MAX_ADDRESS {
        assert_eq!(pool.assign(), Ok(address));
    }
    assert_eq!(pool.assign(), Err(AddressError::Exhausted));
    assert_eq!(pool.assigned(), usize::from(MAX_ADDRESS));
    assert_eq!(pool.release(MAX_ADDRESS), Ok(()));
    assert_eq!(pool.assign(), Ok(MAX_ADDRESS));
}
