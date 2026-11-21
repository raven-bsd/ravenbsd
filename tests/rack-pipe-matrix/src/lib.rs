// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only matrix over the K333 `rack_pipe` public surface.
//!
//! The integration tests call every public `RackPipe` path, regenerate the K319/K324 model
//! traces, and check the K333 rows in the K315 donor-function matrix.

#![forbid(unsafe_code)]

/// The K333 donor group.
pub const GROUP: &str = "K333";
/// The donor file used by the matrix.
pub const DONOR: &str = "FreeBSD sys/netinet/tcp_stacks/rack.c @ 133344405f";
