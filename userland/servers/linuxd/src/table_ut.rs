// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process-table and region tests.

use raven_abi::types::Handle;

use super::*;

fn process() -> LinuxProcess {
    LinuxProcess::new(7, 0x4000, 0x5000, 0x8000, 0x9000)
}

#[test]
fn a_new_process_starts_at_the_break_and_has_no_exit() {
    let process = process();
    assert_eq!(process.pid, 7);
    assert_eq!(process.brk, 0x4000);
    assert_eq!(process.brk_start, 0x4000);
    assert_eq!(process.exit, None);
    assert!(process.regions.is_empty());
}

#[test]
fn regions_contain_whole_ranges_only() {
    let region = Region {
        start: 0x1000,
        len: 0x2000,
        backing: Handle(3),
        backing_offset: 0x40,
    };
    assert!(region.contains(0x1000, 0));
    assert!(region.contains(0x1000, 0x2000));
    assert!(!region.contains(0x1000, 0x2001));
    assert!(!region.contains(0xfff, 1));
    assert!(!region.contains(u64::MAX, 1));
}

#[test]
fn the_table_finds_the_region_for_a_pointer() {
    let mut process = process();
    process.regions.push(Region {
        start: 0x4000,
        len: 0x1000,
        backing: Handle(9),
        backing_offset: 0,
    });
    assert_eq!(
        process.region_for(0x4000, 0x10).map(|r| r.backing),
        Some(Handle(9))
    );
    assert_eq!(
        process.region_for(0x4fff, 0x1).map(|r| r.backing),
        Some(Handle(9))
    );
    assert!(process.region_for(0x5000, 1).is_none());
    assert!(process.region_for(0x3fff, 1).is_none());
}

#[test]
fn output_is_drained_once() {
    let mut process = process();
    process.output.extend_from_slice(b"hello");
    assert_eq!(process.take_output(), b"hello");
    assert!(process.output.is_empty());
    assert!(process.take_output().is_empty());
}

#[test]
fn the_context_downcasts_to_itself() {
    let mut process = process();
    {
        let context: &mut dyn LinuxContext = &mut process;
        let concrete = context
            .as_any_mut()
            .downcast_mut::<LinuxProcess>()
            .expect("the dispatcher passes the linuxd context");
        concrete.exit = Some(3);
    }
    assert_eq!(process.exit, Some(3));
}
