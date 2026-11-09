// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Image assembly stub (ticket K05).

use std::path::Path;

use anyhow::{Result, bail};

use crate::build::Arch;

/// Assemble a bootable image. Replaced by K05.
pub(crate) fn run(
    root: &Path,
    _arch: Arch,
    profile: &str,
    _cmdline: Option<&str>,
    _output: Option<&Path>,
) -> Result<u8> {
    let _profile = crate::profiles::Profile::load(root, profile)?;
    bail!("not implemented (ticket K05)")
}
