// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the optional prebuilt case image.

use super::*;

#[test]
fn a_case_can_select_a_prebuilt_image() {
    let text = "name = \"smoke\"\narch = \"x86_64\"\nprofile = \"base\"\n\
                timeout_secs = 10\nexpect_exit = \"success\"\n\
                image = \"build/x86_64/demo.qcow2\"\n";
    let case = Case::parse(text).expect("image case");
    assert_eq!(case.spec.image.as_deref(), Some("build/x86_64/demo.qcow2"));
}

#[test]
fn an_empty_case_image_is_rejected() {
    let text = "name = \"smoke\"\narch = \"x86_64\"\nprofile = \"base\"\n\
                timeout_secs = 10\nexpect_exit = \"success\"\nimage = \"  \"\n";
    let error = format!("{:#}", Case::parse(text).unwrap_err());
    assert!(error.contains("image must not be empty"), "{error}");
}
