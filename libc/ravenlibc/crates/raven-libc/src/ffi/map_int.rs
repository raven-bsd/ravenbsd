// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `strto*` status -> `errno` table (`donor-map/03`, C41 `IntStatus`; C60 4.3 item 14).

use raven_libc_stdlib::conv::IntStatus;

use super::errno;
use raven_libc_abi::errno::Errno;

/// Apply a `strto*` status: `Range` -> `ERANGE`, `InvalidBase` -> `EINVAL`, otherwise nothing.
pub(crate) fn apply(status: IntStatus) {
    match status {
        IntStatus::Range => errno::set(Errno(34)),
        IntStatus::InvalidBase => errno::set(Errno(22)),
        IntStatus::Ok | IntStatus::NoDigits => {}
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "map_ut.rs"]
mod tests;
