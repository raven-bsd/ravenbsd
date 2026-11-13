// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `srand(1)`/`rand()` against the host glibc when the host is glibc (host-only, test code).

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod glibc {
    use raven_libc_stdlib::rand::DefaultRandom;

    unsafe extern "C" {
        fn srand(seed: u32);
        fn rand() -> i32;
    }

    #[test]
    fn first_hundred_values_match_host_glibc() {
        let mut generator = DefaultRandom::new();
        generator.srand(1);
        // SAFETY: srand/rand are the host C library's process-global generator; this test owns
        // the process seed for its duration and the binary does not otherwise depend on it.
        unsafe {
            srand(1);
            for index in 0..100 {
                let expected = rand() as u32;
                assert_eq!(generator.rand(), expected, "index {index}");
            }
        }
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
#[test]
fn host_is_not_glibc() {
    println!("skipped: the host libc is not glibc, so the rand() sequence has no reference");
}
