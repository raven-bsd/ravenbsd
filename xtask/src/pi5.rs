// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `xtask` entry point skeleton for the Pi 5 image layout (B54).
//!
//! B54 owns `xtask/src/pi5*` but not the `cli.rs`/`image.rs` dispatcher, so this module is not
//! wired into `cargo xtask image --platform pi5` yet. B59 owns the image ticket and can call
//! [`describe`] from the dispatcher. `raven-pi5`'s tests include this file with `#[path]` so the
//! skeleton stays compiled and checked until then.

use crate::image::{ImageLayout, layout};

/// Return the Pi 5 image layout descriptor.
#[must_use]
pub fn describe() -> ImageLayout {
    layout()
}
