// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable-Rust x86-64 SysV variadic prototype (ticket K03).
//!
//! Stable Rust cannot define a C-variadic function, but it can call one and it can provide the
//! symbol in `global_asm!`. [`my_snprintf`] is such a symbol: the assembly trampoline saves the
//! SysV argument registers, builds a `va_list`, and calls a fixed-signature Rust implementation.
//! [`VaReader`] walks the register save area and the overflow area.
//!
//! This crate is a host-target spike, not shipped code. It is `no_std` outside tests and has no
//! dependencies. Every `unsafe` operation has a `// SAFETY:` comment.

#![cfg_attr(not(test), no_std)]
#![cfg(target_arch = "x86_64")]

mod asm;
mod snprintf;
mod va;

pub use asm::my_snprintf;
pub use va::{VaList, VaReader};
