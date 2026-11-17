// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `<fenv.h>` C exports: each body is one `raven-libc-m` call.

/// The C `fexcept_t`: 16-bit on x86-64, 32-bit on the other targets.
#[cfg(target_arch = "x86_64")]
pub(crate) type FexceptC = u16;
/// The C `fexcept_t`: 16-bit on x86-64, 32-bit on the other targets.
#[cfg(not(target_arch = "x86_64"))]
pub(crate) type FexceptC = u32;

mod feclearexcept;
mod fegetround;
mod fesetexcept;
mod fetestexcept;
