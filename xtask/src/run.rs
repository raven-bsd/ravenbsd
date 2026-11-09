// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! QEMU runner stub (ticket K05).

use std::path::Path;

use anyhow::{Result, bail};

use crate::build::Arch;

/// Build and run the image under QEMU. Replaced by K05.
pub(crate) fn run(
    _root: &Path,
    _arch: Arch,
    _profile: &str,
    _no_build: bool,
    _extra: &[String],
) -> Result<u8> {
    bail!("not implemented (ticket K05)")
}
