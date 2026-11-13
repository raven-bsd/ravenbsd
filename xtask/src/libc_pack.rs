// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-pack` command (C34 skeleton).

use std::path::Path;

use anyhow::{Result, bail};

pub(crate) fn run(_root: &Path, _args: &[String]) -> Result<u8> {
    bail!("libc-pack: not implemented by C34 (ticket C60)")
}
