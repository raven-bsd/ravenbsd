// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Release gate for the interim entropy source (H-5, RD-56).
//!
//! Exit 1 while `weak-entropy-interim` is in the default feature set; the PG-52 follow-up
//! removes the feature and the gate turns green.

fn main() {
    let weak = raven_libc_rand::entropy_is_weak();
    println!("entropy_is_weak={weak}");
    if weak {
        std::process::exit(1);
    }
}
