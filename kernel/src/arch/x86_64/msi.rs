// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MSI/MSI-X message encoding (kernel view).
//!
//! The pure crate `crates/raven-msi` is included by `#[path]` so the kernel and the host tests
//! share one implementation of the message layout.  Re-exported for the driver module and the
//! interrupt-vector pool.

#[path = "../../../../crates/raven-msi/src/lib.rs"]
pub mod imp;

pub use imp::{DELIVERY_FIXED, msi_address, msi_data, vector_in_pool};
