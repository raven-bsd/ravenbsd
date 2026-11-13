// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! ARMv8-A routines imported from the Arm optimized routines (Arm Limited, MIT).
//!
//! The assembled symbols are `__raven_str_*`; the C-ABI shell aliases them to the C names
//! (RD-new 1). They are compiled only for a NEON-capable aarch64 target; the soft-float target
//! uses the generic Rust path.

#[cfg(all(target_arch = "aarch64", target_feature = "neon"))]
mod asm;
