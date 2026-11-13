// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parse_keeps_the_build_string() {
    let options = parse(&TestPlatform::new(), b"CJ");
    assert!(options.chunk_canaries);
    assert_eq!(options.def_malloc_junk, 2);
}

#[test]
fn pool_and_cache_counts_clamp() {
    let options = Options {
        malloc_mutexes: 100,
        def_maxcache: 1000,
        ..Options::default()
    };
    assert_eq!(pool_count(&options), 32);
    assert_eq!(cache_count(&options), 256);
}

struct TestPlatform {
    errno: core::cell::Cell<i32>,
}

impl TestPlatform {
    fn new() -> Self {
        Self {
            errno: core::cell::Cell::new(0),
        }
    }
}

impl crate::provider::Platform for TestPlatform {
    fn thread_id(&self) -> usize {
        1
    }
    fn set_errno(&self, e: i32) {
        self.errno.set(e);
    }
    fn write_stderr(&self, _b: &[u8]) {}
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"opts"
    }
    fn process_id(&self) -> u32 {
        1
    }
}
