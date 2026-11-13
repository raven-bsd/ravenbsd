// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Portable arithmetic helpers and the exact-class predicates built on the kernels.

pub use self::ldexp::{ldexp, ldexpf};
pub use self::modf::modf;
pub use self::modff::modff;

mod ldexp;
mod modf;
mod modff;
