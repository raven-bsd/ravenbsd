// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the procinfo page reader.

#![allow(clippy::indexing_slicing)] // fixed-size page buffers
#![allow(clippy::arithmetic_side_effects)] // page fill arithmetic

use core::cell::Cell;

use super::*;

struct FakePage {
    data: [u8; PAGE_LEN],
    seq: Cell<u32>,
    odd_once: Cell<bool>,
    change_once: Cell<bool>,
    change_every_read: Cell<bool>,
    fail_read: Cell<bool>,
}

impl FakePage {
    fn new() -> Self {
        Self {
            data: [0u8; PAGE_LEN],
            seq: Cell::new(0),
            odd_once: Cell::new(false),
            change_once: Cell::new(false),
            change_every_read: Cell::new(false),
            fail_read: Cell::new(false),
        }
    }

    fn put_u32(&mut self, offset: usize, value: u32) {
        self.data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_i32(&mut self, offset: usize, value: i32) {
        self.data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_u64(&mut self, offset: usize, value: u64) {
        self.data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }

    fn put_bytes(&mut self, offset: usize, value: &[u8]) {
        self.data[offset..offset + value.len()].copy_from_slice(value);
    }
}

impl SeqPage for FakePage {
    fn seq(&self) -> u32 {
        let current = self.seq.get();
        if self.odd_once.replace(false) {
            return current | 1;
        }
        current
    }

    fn read(&self, offset: usize, out: &mut [u8]) -> bool {
        if self.fail_read.get() {
            return false;
        }
        let end = match offset.checked_add(out.len()) {
            Some(end) => end,
            None => return false,
        };
        let Some(source) = self.data.get(offset..end) else {
            return false;
        };
        out.copy_from_slice(source);
        if self.change_every_read.get() || self.change_once.replace(false) {
            self.seq.set(self.seq.get().wrapping_add(2));
        }
        true
    }
}

fn valid_page() -> FakePage {
    let mut page = FakePage::new();
    page.put_u32(OFF_MAGIC, MAGIC);
    page.put_u32(OFF_VERSION, 1);
    page.put_i32(OFF_PID, 42);
    page.put_i32(OFF_PPID, 1);
    page.put_i32(OFF_PGID, 42);
    page.put_i32(OFF_SID, 42);
    page.put_u32(OFF_IDS, 1000);
    page.put_u32(OFF_IDS + 4, 1001);
    page.put_u32(OFF_IDS + 8, 1002);
    page.put_u32(OFF_IDS + 12, 2000);
    page.put_u32(OFF_IDS + 16, 2001);
    page.put_u32(OFF_IDS + 20, 2002);
    page.put_u32(OFF_UMASK, 0o022);
    page.put_i32(OFF_NICE, -5);
    page.put_u32(OFF_NGROUPS, 2);
    page.put_u32(OFF_FLAGS, 3);
    page.put_u32(OFF_GROUPS, 10);
    page.put_u32(OFF_GROUPS + 4, 20);
    page.put_u64(OFF_RLIMITS, 1024);
    page.put_u64(OFF_RLIMITS + 8, 32768);
    page.put_bytes(OFF_LOGIN, b"alice");
    page.put_bytes(OFF_TITLE, b"sh");
    page
}

#[test]
fn the_layout_offsets_are_fixed() {
    assert_eq!(OFF_MAGIC, 0);
    assert_eq!(OFF_SEQ, 8);
    assert_eq!(OFF_PID, 16);
    assert_eq!(OFF_UMASK, 56);
    assert_eq!(OFF_GROUPS, 72);
    assert_eq!(OFF_RLIMITS, 200);
    assert_eq!(OFF_LOGIN, 456);
    assert_eq!(OFF_TITLE, 489);
    assert_eq!(PAGE_LEN, 4096);
}

#[test]
fn a_valid_page_decodes() {
    let page = valid_page();
    let info = read_procinfo(&page).expect("procinfo");
    assert_eq!(info.version, 1);
    assert_eq!(info.pid, 42);
    assert_eq!(info.ppid, 1);
    assert_eq!(info.pgid, 42);
    assert_eq!(info.sid, 42);
    assert_eq!((info.ruid, info.euid, info.suid), (1000, 1001, 1002));
    assert_eq!((info.rgid, info.egid, info.sgid), (2000, 2001, 2002));
    assert_eq!(info.umask, 0o022);
    assert_eq!(info.nice, -5);
    assert_eq!(info.ngroups, 2);
    assert_eq!(info.flags, 3);
    assert_eq!(info.groups[0], 10);
    assert_eq!(info.groups[1], 20);
    assert!(!info.more_groups);
    assert_eq!(info.rlimits[0], [1024, 32768]);
    assert_eq!(&info.login[..5], b"alice");
    assert_eq!(&info.title[..2], b"sh");
}

#[test]
fn a_bad_magic_is_eproto() {
    let mut page = valid_page();
    page.put_u32(OFF_MAGIC, 0);
    assert_eq!(read_procinfo(&page), Err(EPROTO));
}

#[test]
fn an_odd_sequence_is_retried() {
    let page = valid_page();
    page.odd_once.set(true);
    assert!(read_procinfo(&page).is_ok());
}

#[test]
fn a_changed_sequence_is_retried_once() {
    let page = valid_page();
    page.change_once.set(true);
    assert!(read_procinfo(&page).is_ok());
}

#[test]
fn a_forever_changing_sequence_is_eagain() {
    let page = valid_page();
    page.change_every_read.set(true);
    assert_eq!(read_procinfo(&page), Err(EAGAIN));
}

#[test]
fn a_failed_read_is_eio() {
    let page = valid_page();
    page.fail_read.set(true);
    assert_eq!(read_procinfo(&page), Err(EIO));
}

#[test]
fn more_than_32_groups_is_capped() {
    let mut page = valid_page();
    page.put_u32(OFF_NGROUPS, 40);
    let info = read_procinfo(&page).expect("procinfo");
    assert_eq!(info.ngroups, 32);
    assert!(info.more_groups);
}

#[test]
fn the_sequence_value_is_reported() {
    let page = valid_page();
    page.seq.set(18);
    let info = read_procinfo(&page).expect("procinfo");
    assert_eq!(info.seq, 18);
}
