// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven-libc-tls`: the thread control block (TCB) and the ELF TLS block layout.
//!
//! This is layer L0 of ravenlibc. It owns the constants (`GAP_ABOVE_TP`, `DTP_OFFSET`,
//! `TLS_ABOVE_TP`, `MIN_TLS_ALIGN`), the `Pthread` structure whose first fields are an ABI that
//! compilers read, the `PT_TLS` size/offset arithmetic and its copy placement, the dynamic thread
//! vector (`dtv`) and the `tp`-relative `errno` offset.
//!
//! The crate is pure arithmetic and atomics: [`#![deny(unsafe_code)]`] covers everything except
//! [`raw`], the one audited module that writes the initial TLS image and reads a `Pthread` through
//! a thread pointer. It has no `alloc`, no `std` outside tests and no dependency other than
//! `raven-libc-abi`.
//!
//! The loader obligations that C62 must satisfy are in `README.md`.

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]
#![cfg_attr(
    test,
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::indexing_slicing,
        clippy::arithmetic_side_effects
    )
)]

pub mod consts;
pub mod dtv;
pub mod errno;
pub mod init;
pub mod layout;
#[allow(unsafe_code)] // the one audited module: initial-image writes and `tp` conversion
pub mod raw;
pub mod tcb;

pub use consts::{
    Abi, DTP_OFFSET, GAP_ABOVE_TP, MIN_TLS_ALIGN, TARGET_ABI, TLS_ABOVE_TP, TP_OFFSET,
};
pub use dtv::Dtv;
pub use layout::{
    LayoutError, ModuleOffsets, StaticLayout, TlsModuleIn, compute_static_layout, module_offsets,
};
pub use tcb::Pthread;
