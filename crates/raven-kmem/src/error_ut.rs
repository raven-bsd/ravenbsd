// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the error mapping.

use super::*;

#[test]
fn arena_errors_map_to_kmem_errors() {
    assert_eq!(
        KmemError::from(raven_vmem::VmemError::Exhausted),
        KmemError::Exhausted
    );
    assert_eq!(
        KmemError::from(raven_vmem::VmemError::NoTags),
        KmemError::Exhausted
    );
    assert_eq!(
        KmemError::from(raven_vmem::VmemError::Overflow),
        KmemError::Exhausted
    );
    assert_eq!(
        KmemError::from(raven_vmem::VmemError::ZeroLength),
        KmemError::ZeroSize
    );
    assert_eq!(
        KmemError::from(raven_vmem::VmemError::Alignment),
        KmemError::Alignment
    );
}
