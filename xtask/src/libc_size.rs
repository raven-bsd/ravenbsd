// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-size` command (C34 skeleton).

use std::path::Path;

use anyhow::{Result, bail};

pub(crate) fn run(_root: &Path, _args: &[String]) -> Result<u8> {
    bail!("libc-size: not implemented by C34 (ticket C69)")
}
