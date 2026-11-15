// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the in-memory seam double.

use super::*;

#[test]
fn a_matching_image_verifies() {
    let expected = [1u8, 2, 3, 4];
    let mut seam = SimSeam::new(&expected).expect("seam");
    assert!(!seam.verify());
    assert_eq!(seam.write_chunk(0, &expected), Ok(()));
    assert_eq!(seam.written(), 4);
    assert!(seam.verify());
    assert_eq!(seam.image(), &expected);
    assert_eq!(seam.doorbell(0), Ok(()));
    assert_eq!(seam.doorbells(), 1);
    assert_eq!(seam.start(), Ok(()));
    assert!(seam.started());
    assert_eq!(seam.reset(), Ok(()));
    assert_eq!(seam.resets(), 1);
    assert_eq!(seam.written(), 0);
    assert!(!seam.started());
}

#[test]
fn an_oversized_image_is_rejected() {
    let big = [0u8; MAX_IMAGE + 1];
    assert!(SimSeam::new(&big).is_none());
}

#[test]
fn the_status_double_answers_in_order() {
    let expected = [0u8; 4];
    let mut seam = SimSeam::new(&expected).expect("seam");
    seam.set_busy_polls(2);
    assert_eq!(seam.status(), Ok(Status::Busy));
    assert_eq!(seam.status(), Ok(Status::Busy));
    assert_eq!(seam.status(), Ok(Status::Ready));

    seam.set_fault(Some(0x42));
    assert_eq!(seam.status(), Ok(Status::Fault(0x42)));

    seam.set_status_error(Some(SeamError::Timeout));
    assert_eq!(seam.status(), Err(SeamError::Timeout));
}

#[test]
fn failures_can_be_injected() {
    let expected = [0u8; 4];
    let mut seam = SimSeam::new(&expected).expect("seam");
    seam.set_fail_write(true);
    assert_eq!(seam.write_chunk(0, &expected), Err(SeamError::Bus));
    seam.set_fail_reset(true);
    assert_eq!(seam.reset(), Err(SeamError::Bus));
    seam.set_fail_boot(true);
    assert_eq!(seam.boot_ready(), Err(SeamError::Bus));
}

#[test]
fn a_write_outside_the_image_fails() {
    let expected = [0u8; 4];
    let mut seam = SimSeam::new(&expected).expect("seam");
    assert_eq!(seam.write_chunk(250, &[0u8; 16]), Err(SeamError::Bus));
}
