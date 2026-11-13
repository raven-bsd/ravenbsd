// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getentropy` and seed tests.

use raven_abi::errno::Errno;

use crate::backend::FakeSys;
use crate::platform::Platform;
use crate::random::{GETENTROPY_MAX, entropy_weak, getentropy, getrandom, read_rand, seed};

#[test]
fn two_hundred_fifty_six_is_accepted() {
    let fake = FakeSys::new();
    fake.rand(&[0x5a; 256]);
    let platform = Platform::new(fake);
    let mut buffer = [0u8; 256];
    getentropy(&platform, &mut buffer).expect("256");
    assert!(buffer.iter().all(|&byte| byte == 0x5a));
}

#[test]
fn two_hundred_fifty_seven_is_eio() {
    let fake = FakeSys::new();
    fake.rand(&[0x5a; 512]);
    let platform = Platform::new(fake);
    let mut buffer = [0u8; 257];
    assert_eq!(getentropy(&platform, &mut buffer), Err(Errno::EIO));
    assert_eq!(GETENTROPY_MAX, 256);
}

#[test]
fn zero_length_is_a_success() {
    let platform = Platform::new(FakeSys::new());
    let mut buffer = [0u8; 0];
    assert_eq!(getentropy(&platform, &mut buffer), Ok(()));
}

#[test]
fn short_source_is_eio() {
    let fake = FakeSys::new();
    fake.rand(&[1, 2, 3, 4]);
    let platform = Platform::new(fake);
    let mut buffer = [0u8; 8];
    assert_eq!(getentropy(&platform, &mut buffer), Err(Errno::EIO));
}

#[test]
fn missing_rand_scheme_is_enoent() {
    let platform = Platform::new(FakeSys::new());
    let mut buffer = [0u8; 8];
    assert_eq!(getentropy(&platform, &mut buffer), Err(Errno::ENOENT));
}

#[test]
fn seed_is_32_bytes_and_clock_mixed() {
    let fake = FakeSys::new();
    fake.rand(&[0u8; 64]);
    let platform = Platform::new(fake);
    let first = seed(&platform).expect("seed");
    assert_eq!(first.len(), 32);
    platform.backend().advance_ns(1_000_000_000);
    let second = seed(&platform).expect("seed");
    assert_ne!(first, second);
}

#[test]
fn getrandom_is_enosys_until_pg_52() {
    let fake = FakeSys::new();
    fake.rand(&[0u8; 32]);
    let platform = Platform::new(fake);
    let mut buffer = [0u8; 8];
    assert_eq!(getrandom(&platform, &mut buffer, 0), Err(Errno::ENOSYS));
}

#[test]
fn entropy_is_weak_until_pg_52() {
    assert!(entropy_weak());
}

#[test]
fn read_rand_returns_the_bytes() {
    let fake = FakeSys::new();
    fake.rand(b"abcdefgh");
    let platform = Platform::new(fake);
    assert_eq!(read_rand(&platform, 8), Ok(b"abcdefgh".to_vec()));
}

#[test]
fn platform_wrapper_matches_the_free_function() {
    let fake = FakeSys::new();
    fake.rand(&[9; 16]);
    let platform = Platform::new(fake);
    let mut buffer = [0u8; 16];
    platform.getentropy(&mut buffer).expect("getentropy");
    assert_eq!(buffer, [9; 16]);
}
