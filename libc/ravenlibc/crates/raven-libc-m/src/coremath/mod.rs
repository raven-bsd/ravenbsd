// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CORE-MATH kernels (`cbrt`, `hypot`): bit-level algorithms with a proven error bound.

pub use self::cbrt::{cbrt, cbrt_round};
pub use self::cbrtf::cbrtf;
pub use self::hypot::hypot;
pub use self::hypotf::hypotf;

mod cbrt;
mod cbrtf;
mod hypot;
mod hypotf;
