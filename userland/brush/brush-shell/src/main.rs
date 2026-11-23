// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Main entry for the `brush` shell.

fn main() {
    std::env::set_var("RUST_BACKTRACE", "1");
    brush_shell::entry::run();
}
