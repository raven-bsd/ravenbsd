// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD build orchestration.
//!
//! The top-level `Makefile` forwards to this crate. It owns the per-component flag
//! table, the reproducible build environment, the stable-Rust guard and the local
//! `check` gate. Image assembly, initfs packing and QEMU running are ticket K05 and
//! live here as stubs.

#![forbid(unsafe_code)]

mod addons;
mod brush;
mod build;
mod check;
mod cli;
mod docs;
mod fat;
mod gpt;
mod guard;
mod image;
mod initfs;
mod libc_abi;
mod libc_audit;
mod libc_census;
mod libc_cmd;
mod libc_headers;
mod libc_link;
mod libc_lint;
mod libc_lint_abi;
mod libc_lint_data;
mod libc_lint_scan;
mod libc_lint_size;
#[cfg(test)]
mod libc_lint_testutil;
mod libc_oracle;
mod libc_pack;
mod libc_pack_crt;
mod libc_pack_hostrun;
mod libc_pack_probe;
mod libc_pack_stubs;
mod libc_pack_tools;
mod libc_size;
mod libc_stubs;
mod libc_world;
mod pi;
mod posix_check;
mod posix_gaps;
mod process;
mod profiles;
mod qtest;
mod repo;
mod rootfs;
mod run;
mod update;
mod uutils;

pub use rootfs::{build_root_disk, build_ufs_root, inject_initfs_file, stage_rootfs_spec};

/// Dispatch `cargo xtask` arguments, with the K27 `update` planner and the brush component
/// beside the K05 commands.
pub fn run_cli(args: Vec<String>) -> std::process::ExitCode {
    if args.first().map(String::as_str) == Some("libc-audit") {
        return libc_audit::run_cli(&args[1..]);
    }
    if args.first().map(String::as_str) == Some("update") {
        return update::run_cli(&args[1..]);
    }
    if let Some(result) = brush::try_run(&args) {
        return match result {
            Ok(code) => std::process::ExitCode::from(code),
            Err(error) => {
                eprintln!("error: {error:#}");
                std::process::ExitCode::from(2)
            }
        };
    }
    cli::run_cli(brush::rewrite_qtest(args))
}
