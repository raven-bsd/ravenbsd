// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the RACK misc allocator/free-list internals.

use super::*;

#[test]
fn allocation_reuses_the_hot_free_list() {
    let mut rack = RackMisc::new();
    let ids: alloc::vec::Vec<u64> = (0..5).map(|_| rack.rack_alloc().expect("alloc")).collect();
    for id in &ids {
        assert!(rack.rack_free(*id));
    }
    assert_eq!(rack.free_count, 5);
    let reused = rack.rack_alloc().expect("reuse");
    assert!(ids.contains(&reused));
}

#[test]
fn a_full_map_limit_reports_once() {
    let mut rack = RackMisc::new();
    rack.map_entries_limit = 1;
    let first = rack.rack_alloc_full_limit().expect("first");
    assert!(rack.rack_alloc_full_limit().is_none());
    assert!(rack.alloc_limit_reported);
    let _ = rack.rack_free(first);
}

#[test]
fn split_limit_marks_the_limit_type() {
    let mut rack = RackMisc::new();
    rack.split_limit = 1;
    let first = rack.rack_alloc_limit(1).expect("first");
    assert_eq!(rack.sendmaps.get(&first).expect("map").limit_type, 1);
    assert!(rack.rack_alloc_limit(1).is_none());
    assert_eq!(rack.split_allocs, 1);
}

#[test]
fn ctor_zeroes_the_zone_memory() {
    let mut memory = [0xaa; 16];
    assert_eq!(RackMisc::rack_ctor(&mut memory), 0);
    assert!(memory.iter().all(|byte| *byte == 0));
    RackMisc::rack_dtor(&mut memory);
}
