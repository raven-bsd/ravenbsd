// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD build orchestration.
//!
//! The top-level `Makefile` forwards to this crate. It owns the per-component flag
//! table, the reproducible build environment, the stable-Rust guard and the local
//! `check` gate. Image assembly, initfs packing and QEMU running are ticket K05 and
//! live here as stubs.

#![forbid(unsafe_code)]

mod build;
mod check;
mod cli;
mod docs;
mod fat;
mod gpt;
mod guard;
mod image;
mod initfs;
mod pi;
mod process;
mod profiles;
mod qtest;
mod repo;
mod rootfs;
mod run;

pub use rootfs::{build_root_disk, build_ufs_root, inject_initfs_file, stage_rootfs_spec};

pub use cli::run_cli;
