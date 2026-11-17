// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `abi-diff` CLI: generate the native ABI table or diff two tables.

fn main() -> std::process::ExitCode {
    abi_diff::run()
}
