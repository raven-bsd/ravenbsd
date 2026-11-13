// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

struct Pages;

impl PageProvider for Pages {
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

impl Lock for Locks {
    fn lock(&self, _index: usize) {}
    fn unlock(&self, _index: usize) {}
    fn reset(&self, _index: usize) {}
}

struct Random;

impl Entropy for Random {
    fn fill(&self, buf: &mut [u8]) -> bool {
        buf.fill(1);
        true
    }
}

struct Host;

impl Platform for Host {
    fn thread_id(&self) -> usize {
        1
    }
    fn set_errno(&self, _e: i32) {}
    fn write_stderr(&self, _b: &[u8]) {}
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"test"
    }
    fn process_id(&self) -> u32 {
        1
    }
}

#[test]
fn traits_have_the_documented_defaults() {
    assert!(Pages.seal(0, 4096));
    assert_eq!(Pages.page_size(), 4096);
    Locks.lock(0);
    Locks.unlock(0);
    Locks.reset(0);
    let mut bytes = [0u8; 4];
    assert!(Random.fill(&mut bytes));
    assert_eq!(bytes, [1; 4]);
    assert!(Host.is_secure_exec());
    assert_eq!(Host.env_malloc_options(), None);
    assert_eq!(Host.env_malloc_debug(), None);
    assert_eq!(Host.get_errno(), 0);
    assert_eq!(Host.mallinfo(), Mallinfo::default());
}

#[test]
fn lock_indices_are_not_bounded_by_the_trait() {
    Locks.lock(31);
    Locks.unlock(31);
    Locks.reset(31);
}

#[test]
fn entropy_can_fill_an_empty_slice() {
    let mut empty = [];
    assert!(Random.fill(&mut empty));
}

#[test]
fn pages_seal_is_a_no_op_success() {
    assert!(Pages.seal(0x1000, 0x1000));
}

#[test]
fn platform_defaults_are_conservative() {
    assert!(Host.is_secure_exec());
    assert_eq!(Host.env_malloc_options(), None);
    assert_eq!(Host.env_malloc_debug(), None);
}
