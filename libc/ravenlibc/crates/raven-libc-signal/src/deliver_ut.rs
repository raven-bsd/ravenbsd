// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::defaults::DefaultAction;
use crate::kernel::{FaultKind, FaultRecord};

#[test]
fn lowest_is_the_smallest_signal_number() {
    assert_eq!(lowest(0), 0);
    assert_eq!(lowest(1), 1);
    assert_eq!(lowest(0b10), 2);
    assert_eq!(lowest(1 << 34), 35);
    assert_eq!(lowest(crate::critical::signal_bit(64)), 64);
}

#[test]
fn deliverable_masks_internal_and_blocked_bits() {
    let pending = u64::MAX;
    assert_eq!(deliverable(pending, 0), crate::sigset::APP_MASK);
    assert_eq!(deliverable(0, 0), 0);
}

#[test]
fn decide_covers_ignore_default_and_handler() {
    assert_eq!(decide(0, 0, false, false), Decision::Nothing);
    assert_eq!(decide(1, 2, false, false), Decision::Ignore);
    assert_eq!(
        decide(0, 15, false, false),
        Decision::Default(DefaultAction::Terminate)
    );
    assert_eq!(
        decide(0, 9, false, false),
        Decision::Default(DefaultAction::Terminate)
    );
    assert_eq!(decide(0x1234, 10, false, false), Decision::Handler(0x1234));
    assert_eq!(decide(0x1234, 10, true, false), Decision::Defer);
}

#[test]
fn a_fault_with_default_terminates_even_for_an_ignored_default() {
    assert_eq!(
        decide(0, 11, false, true),
        Decision::Default(DefaultAction::Core)
    );
    assert_eq!(
        decide(0, 8, false, true),
        Decision::Default(DefaultAction::Core)
    );
}

#[test]
fn fault_mapping_matches_ticket_4_6_2() {
    let cases = [
        (FaultKind::PageNomap, 11, 1),
        (FaultKind::PagePerm, 11, 2),
        (FaultKind::Divide, 8, 1),
        (FaultKind::Illegal, 4, 1),
        (FaultKind::Misalign, 7, 1),
        (FaultKind::Breakpoint, 5, 1),
    ];
    for (kind, signal, code) in cases {
        let (got_signal, got_code) = fault_to_signal(kind);
        assert_eq!((got_signal, got_code), (signal, code));
    }
}

#[test]
fn fault_record_builds_the_siginfo() {
    let record = FaultRecord {
        kind: FaultKind::PageNomap as u32,
        addr: 0xdead_0000,
        pc: 0x1000,
    };
    let (signal, info) = fault_signal(&record).unwrap();
    assert_eq!(signal, 11);
    assert_eq!(crate::frame::si_code(&info), 1);
    assert_eq!(crate::frame::si_addr(&info), 0xdead_0000);
}

#[test]
fn divide_and_illegal_report_the_pc() {
    for kind in [FaultKind::Divide, FaultKind::Illegal] {
        let record = FaultRecord {
            kind: kind as u32,
            addr: 0x1,
            pc: 0x2000,
        };
        let (_, info) = fault_signal(&record).unwrap();
        assert_eq!(crate::frame::si_addr(&info), 0x2000);
    }
}

#[test]
fn application_signal_range_and_duplicate_rule() {
    assert!(application_signal(1));
    assert!(application_signal(64));
    assert!(!application_signal(0));
    assert!(!application_signal(32));
    assert!(!application_signal(33));
    assert!(!application_signal(34));
    assert!(drop_duplicate(5, crate::critical::signal_bit(5)));
    assert!(!drop_duplicate(5, 0));
    assert!(!drop_duplicate(36, crate::critical::signal_bit(36)));
}

#[test]
fn info_signal_reads_si_signo() {
    let mut info = raven_libc_abi::structs::SiginfoT::ZERO;
    info.si_signo = 17;
    assert_eq!(info_signal(&info), 17);
}

#[test]
fn fault_ignores_the_mask() {
    assert!(fault_ignores_mask(11));
}
