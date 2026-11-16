// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host stress and fault-injection harness for the GENET model.
//!
//! The harness is a test-only workspace package. It drives the public `raven-genet` state
//! machines through long deterministic loops; no MMIO, DMA or board access exists here.

#![forbid(unsafe_code)]

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
