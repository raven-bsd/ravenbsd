// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use crate::hardened::Hardened;

#[test]
fn hash_is_stable_and_page_based() {
    assert_eq!(
        Hardened::<Pages, Locks, Random, Host>::hash(0x2000),
        Hardened::<Pages, Locks, Random, Host>::hash(0x2000)
    );
    assert_ne!(
        Hardened::<Pages, Locks, Random, Host>::hash(0x2000),
        Hardened::<Pages, Locks, Random, Host>::hash(0x9000)
    );
}

struct Pages;

impl crate::provider::PageProvider for Pages {
    fn page_size(&self) -> usize {
        4096
    }
    fn map_pages(&self, _len: usize, _align: usize, _prot: u8) -> Option<usize> {
        None
    }
    fn map_exact(&self, _hint: usize, _len: usize) -> Option<usize> {
        None
    }
    fn unmap(&self, _addr: usize, _len: usize) -> bool {
        false
    }
    fn protect(&self, _addr: usize, _len: usize, _prot: u8) -> bool {
        false
    }
}

struct Locks;

impl crate::provider::Lock for Locks {
    fn lock(&self, _index: usize) {}
    fn unlock(&self, _index: usize) {}
    fn reset(&self, _index: usize) {}
}

struct Random;

impl crate::provider::Entropy for Random {
    fn fill(&self, buf: &mut [u8]) -> bool {
        buf.fill(1);
        true
    }
}

struct Host;

impl crate::provider::Platform for Host {
    fn thread_id(&self) -> usize {
        1
    }
    fn set_errno(&self, _e: i32) {}
    fn write_stderr(&self, _b: &[u8]) {}
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"pages"
    }
    fn process_id(&self) -> u32 {
        1
    }
}
