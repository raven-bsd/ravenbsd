// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions for the synthetic firmware fixture and the crate constants.

use super::*;
use crate::sim::SimSeam;
use crate::test_fixture;

#[test]
fn the_fixture_is_a_recorded_synthetic_image() {
    let image = test_fixture::image();
    assert!(image.is_recorded());
    assert_eq!(image.path, "synthetic:rp1-fw-fixture");
    assert_eq!(image.size, Some(64));
    assert!(image.licence.starts_with("synthetic"));
    let blob = test_fixture::blob();
    assert_eq!(blob.len(), 64);
    assert_eq!(blob.first(), Some(&0));
    assert_eq!(blob.last(), Some(&63));
}

#[test]
fn the_real_firmware_fact_is_not_invented() {
    assert!(REAL_FIRMWARE_PATH_UNVERIFIED.starts_with("UNVERIFIED"));
    let redistributed = BLOB_REDISTRIBUTED;
    assert!(!redistributed);
}

#[test]
fn the_fixture_runs_through_the_loader() {
    let blob = test_fixture::blob();
    let image = test_fixture::image();
    let plan = Plan::new(64, 16).expect("plan");
    let mut loader = Loader::new(image, &blob, plan, Limits::DEFAULT).expect("loader");
    let mut seam = SimSeam::new(&blob).expect("seam");
    let outcome = loader.run(&mut seam).expect("run");
    assert_eq!(outcome.chunks, 4);
    assert!(seam.started());
    assert!(seam.verify());
}
