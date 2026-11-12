// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the GPIO arbiter.

use super::*;

#[test]
fn one_owner_can_claim_and_release_a_pin() {
    let mut arbiter = GpioArbiter::new();
    assert!(!arbiter.is_claimed(17));
    arbiter.claim(17, 1).expect("claim");
    assert!(arbiter.is_claimed(17));
    assert_eq!(arbiter.release(17, 1), Ok(()));
    assert!(!arbiter.is_claimed(17));
    assert_eq!(arbiter.release(17, 1), Err(Errno::EINVAL));
}

#[test]
fn a_second_owner_gets_ebusy() {
    let mut arbiter = GpioArbiter::new();
    arbiter.claim(17, 1).expect("claim");
    assert_eq!(arbiter.claim(17, 2), Err(Errno::EBUSY));
    assert!(arbiter.is_claimed(17));
}

#[test]
fn only_the_owner_can_configure_or_release() {
    let mut arbiter = GpioArbiter::new();
    arbiter.claim(4, 10).expect("claim");
    let config = PinConfig {
        function: Function::Output,
        pull: Pull::Up,
        level: true,
    };
    assert_eq!(arbiter.configure(4, 11, config), Err(Errno::EPERM));
    assert_eq!(arbiter.configure(4, 10, config), Ok(()));
    assert_eq!(arbiter.config(4), Some(config));
    assert_eq!(arbiter.release(4, 11), Err(Errno::EPERM));
    assert_eq!(arbiter.release(4, 10), Ok(()));
}

#[test]
fn an_out_of_range_pin_is_rejected() {
    let mut arbiter = GpioArbiter::new();
    assert_eq!(arbiter.claim(58, 1), Err(Errno::EINVAL));
    assert_eq!(arbiter.release(255, 1), Err(Errno::EINVAL));
    assert_eq!(
        arbiter.configure(58, 1, PinConfig::default()),
        Err(Errno::EINVAL)
    );
}

#[test]
fn function_and_pull_encodings_match_the_bcm2711() {
    assert_eq!(Function::Input.bits(), 0);
    assert_eq!(Function::Output.bits(), 1);
    assert_eq!(Function::Alt0.bits(), 4);
    assert_eq!(Function::Alt5.bits(), 2);
    assert_eq!(Pull::None.bits(), 0);
    assert_eq!(Pull::Up.bits(), 1);
    assert_eq!(Pull::Down.bits(), 2);
}

#[test]
fn a_dead_owner_is_reclaimed_by_close() {
    let mut arbiter = GpioArbiter::new();
    arbiter.claim(27, 100).expect("claim");
    // The kernel sends CLOSE for the dead client's scheme file; platd passes that file id.
    assert_eq!(arbiter.release(27, 100), Ok(()));
    arbiter.claim(27, 101).expect("reclaim");
}
