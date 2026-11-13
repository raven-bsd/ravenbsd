// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Configuration module aggregate tests.

use crate::conf::{SysconfInputs, confstr, fpathconf_value, pathconf_value, sysconf};
use raven_libc_abi::sysconf as ids;

#[test]
fn sysconf_defaults_are_the_documented_ones() {
    let inputs = SysconfInputs::default();
    assert_eq!(sysconf(ids::_SC_ARG_MAX, &inputs), Ok(131072));
    assert_eq!(sysconf(ids::_SC_PAGE_SIZE, &inputs), Ok(4096));
}

#[test]
fn pathconf_and_fpathconf_agree_on_constants() {
    let fs = raven_libc_abi::structs::Statvfs::ZERO;
    assert_eq!(
        pathconf_value(ids::_PC_NAME_MAX, None),
        fpathconf_value(ids::_PC_NAME_MAX, &fs)
    );
}

#[test]
fn confstr_path_is_stable() {
    assert_eq!(confstr(ids::_CS_PATH, None), Ok(14));
}
