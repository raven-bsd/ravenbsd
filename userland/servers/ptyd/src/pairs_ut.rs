// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for named pty pairs.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tty_core::{Mode, Settings};

use super::*;

#[derive(Default)]
struct Sink;

impl SignalSink for Sink {
    fn signal(&mut self, _pgrp: i32, _signo: u32) -> Result<(), Errno> {
        Ok(())
    }
}

fn read_slave(pair: &mut Pty<Sink>) -> Vec<u8> {
    let mut buffer = [0u8; 64];
    let count = pair.read_slave(&mut buffer).expect("slave read");
    buffer[..count].to_vec()
}

#[test]
fn pty_inode_is_nonzero_and_role_distinct() {
    assert_eq!(inode(0, false), 1);
    assert_eq!(inode(0, true), 2);
    assert_eq!(inode(7, false), 15);
    assert_eq!(inode(7, true), 16);
    assert_ne!(inode(0, false), inode(0, true));
    assert_ne!(inode(1, false), inode(0, true));
    assert_ne!(inode(u64::MAX, false), 0);
}

#[test]
fn allocation_uses_the_lowest_free_unit_and_reuses_it() {
    let mut pairs = Pairs::<Sink>::new();
    assert_eq!(pairs.alloc(), 1);
    pairs.open(1);
    assert_eq!(pairs.alloc(), 2);
    pairs.open(2);
    assert_eq!(pairs.alloc(), 3);
    assert!(pairs.remove(1));
    assert_eq!(pairs.alloc(), 1);
}

#[test]
fn removal_releases_only_the_named_pair() {
    let mut pairs = Pairs::<Sink>::new();
    pairs.open(1);
    pairs.open(2);
    assert_eq!(pairs.len(), 2);
    assert!(pairs.remove(1));
    assert_eq!(pairs.len(), 1);
    assert!(pairs.get_mut(1).is_none());
    assert!(pairs.get_mut(2).is_some());
    assert!(!pairs.remove(1));
}

#[test]
fn pairs_have_independent_queues_and_settings() {
    let mut pairs = Pairs::<Sink>::new();
    assert!(pairs.is_empty());

    pairs.open(0).write_master(b"zero\r");
    pairs.open(1).set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });
    pairs.open(1).write_master(b"raw");
    assert_eq!(pairs.len(), 2);

    assert_eq!(read_slave(pairs.open(0)), b"zero\n");
    assert_eq!(read_slave(pairs.open(1)), b"raw");
    assert_eq!(pairs.open(0).settings().mode, Mode::Canonical);
    assert_eq!(pairs.open(1).settings().mode, Mode::Raw);
    assert!(pairs.get_mut(9).is_none());
}
