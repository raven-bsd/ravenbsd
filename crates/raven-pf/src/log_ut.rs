// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the bounded packet log.

use super::*;
use crate::address::Ipv4Addr;
use crate::ast::{Direction, tcp_flag};

fn entry(time: u64) -> LogEntry {
    let packet = Packet::tcp(
        Direction::In,
        "eth0",
        Ipv4Addr::new(10, 0, 0, 1),
        1234,
        Ipv4Addr::new(192, 0, 2, 1),
        80,
        tcp_flag::SYN,
        60,
    );
    LogEntry::from_packet(&packet, Action::Pass, Some(0), None, time)
}

#[test]
fn records_are_kept_in_time_order() {
    let mut log = PacketLog::new();
    for time in 0..3 {
        log.push(entry(time));
    }
    assert_eq!(log.len(), 3);
    assert_eq!(log.entries()[0].time, 0);
    assert_eq!(log.entries()[2].time, 2);
    assert_eq!(log.entries()[1].interface, "eth0");
}

#[test]
fn the_log_is_bounded_and_drops_the_oldest_record() {
    let mut log = PacketLog::new();
    for time in 0..=(MAX_LOG_ENTRIES as u64) {
        log.push(entry(time));
    }
    assert_eq!(log.len(), MAX_LOG_ENTRIES);
    assert_eq!(log.entries()[0].time, 1);
    assert_eq!(
        log.entries()[MAX_LOG_ENTRIES - 1].time,
        MAX_LOG_ENTRIES as u64
    );
}

#[test]
fn drain_empties_the_log() {
    let mut log = PacketLog::new();
    log.push(entry(1));
    let drained = log.drain();
    assert_eq!(drained.len(), 1);
    assert!(log.is_empty());
}
