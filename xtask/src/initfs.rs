// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Initial filesystem packing stub (ticket K05).

use std::path::Path;

use anyhow::{Result, bail};

use crate::build::Arch;

/// Pack the profile's programs into `initfs.img`. Replaced by K05.
pub(crate) fn run(_root: &Path, _arch: Arch, _profile: &str) -> Result<u8> {
    bail!("not implemented (ticket K05)")
}
