// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the DragonFly IPIQ FIFO port.

extern crate std;

use super::*;
use std::vec::Vec;

fn message(value: u32) -> Message<u32> {
    Message {
        func: value,
        arg1: value as usize,
        arg2: value as i32,
    }
}

#[test]
fn fifo_order_and_xindex() {
    let queue: IpIQ<u32> = IpIQ::new();
    for value in 0..10u32 {
        assert_eq!(queue.push(message(value)), Ok(value + 1));
    }
    let mut seen = Vec::new();
    assert_eq!(queue.process(0, |item| seen.push(item.func)), 0);
    assert_eq!(seen, (0..10u32).collect::<Vec<_>>());
    assert_eq!(queue.xindex(), 10);
    assert_eq!(queue.pending(), 0);
}

#[test]
fn process_limit_leaves_the_rest_pending() {
    let queue: IpIQ<u32> = IpIQ::new();
    for value in 0..10u32 {
        let _ = queue.push(message(value));
    }
    let mut seen = Vec::new();
    assert_eq!(queue.process(5, |item| seen.push(item.func)), 5);
    assert_eq!(seen, (0..5u32).collect::<Vec<_>>());
    assert_eq!(queue.xindex(), 5);
    assert_eq!(queue.process(0, |item| seen.push(item.func)), 0);
    assert_eq!(seen, (0..10u32).collect::<Vec<_>>());
}

#[test]
fn the_ring_wraps_without_losing_a_message() {
    let queue: IpIQ<u32> = IpIQ::new();
    for value in 0..10_000u32 {
        while queue.push(message(value)).is_err() {
            let _ = queue.process(1, |_| {});
        }
        if value % 7 == 0 {
            let _ = queue.process(3, |_| {});
        }
    }
    let _ = queue.process(0, |_| {});
    assert_eq!(queue.xindex(), 10_000);
    assert_eq!(queue.pending(), 0);
}

#[test]
fn one_slot_is_reserved() {
    let queue: IpIQ<u32> = IpIQ::new();
    for value in 0..(MAXCPUFIFO as u32 - 1) {
        assert!(queue.push(message(value)).is_ok());
    }
    assert!(queue.push(message(0xffff_ffff)).is_err());
    let _ = queue.process(1, |_| {});
    assert!(queue.push(message(0xffff_ffff)).is_ok());
}

#[test]
fn empty_fifo_is_quiet() {
    let queue: IpIQ<u32> = IpIQ::new();
    assert_eq!(queue.pending(), 0);
    assert_eq!(queue.process(0, |_| panic!("empty")), 0);
    assert_eq!(queue.pop(), None);
    assert_eq!(queue.xindex(), 0);
}
