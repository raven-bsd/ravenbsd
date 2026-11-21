// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only RACK sockopt/query conformance matrix.
//!
//! The matrix is in the integration tests; this crate is a test-only consumer of
//! the K334 `raven_tcpip::RackMisc` sockopt/query surface.

#![forbid(unsafe_code)]

/// The donor file used by the matrix.
pub const DONOR: &str = "FreeBSD sys/netinet/tcp_stacks/rack.c @ 133344405f";
