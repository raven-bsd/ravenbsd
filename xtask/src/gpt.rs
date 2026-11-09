// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GPT writer stub (ticket K05).

use anyhow::{Result, bail};

/// Plan the GPT image layout. Replaced by K05.
#[allow(dead_code)] // K05 replaces this module; the stub keeps the file layout K01 promised.
pub(crate) fn not_implemented() -> Result<u8> {
    bail!("not implemented (ticket K05)")
}
