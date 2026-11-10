// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Architecture-specific loader code selected by `cfg(target_arch)`.

#[cfg(target_arch = "x86_64")]
pub mod x86_64;

#[cfg(any(target_arch = "aarch64", test))]
pub mod aarch64;
