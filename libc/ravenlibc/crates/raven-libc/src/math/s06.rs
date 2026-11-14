// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

/// `totalordermag`.
#[unsafe(no_mangle)]
pub extern "C" fn totalordermag(x: f64, y: f64) -> i32 {
    i32::from(raven_libc_m::totalordermag(x, y))
}

/// `totalordermagf`.
#[unsafe(no_mangle)]
pub extern "C" fn totalordermagf(x: f32, y: f32) -> i32 {
    i32::from(raven_libc_m::totalordermagf(x, y))
}

/// `issignaling`.
#[unsafe(no_mangle)]
pub extern "C" fn issignaling(x: f64) -> i32 {
    i32::from(raven_libc_m::issignaling(x))
}

/// `issignalingf`.
#[unsafe(no_mangle)]
pub extern "C" fn issignalingf(x: f32) -> i32 {
    i32::from(raven_libc_m::issignalingf(x))
}
