// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Server fixture: installs into the `fixture-test` profile.

fn main() {
    println!("{}", fixture_core::add(1, 1));
}
