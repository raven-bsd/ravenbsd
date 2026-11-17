// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the endpoint pair.

use crate::Endpoint;

#[test]
fn peer_is_symmetric() {
    assert_eq!(Endpoint::A.peer(), Endpoint::B);
    assert_eq!(Endpoint::B.peer(), Endpoint::A);
    assert_eq!(Endpoint::A.peer().peer(), Endpoint::A);
}

#[test]
fn all_contains_each_endpoint_once() {
    assert_eq!(Endpoint::ALL.len(), 2);
    assert_eq!(Endpoint::ALL, [Endpoint::A, Endpoint::B]);
}
