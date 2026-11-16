// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the interrupter.

use super::*;

#[test]
fn an_event_needs_the_enable_bit_to_interrupt() {
    let mut irq = Interrupter::new();
    irq.raise();
    assert!(irq.pending());
    assert!(!irq.enabled());
    assert!(!irq.active());
    irq.write_iman(IMAN_IE);
    assert!(irq.active());
    assert_eq!(irq.iman(), IMAN_IP | IMAN_IE);
}

#[test]
fn writing_one_to_the_pending_bit_clears_it() {
    let mut irq = Interrupter::new();
    irq.write_iman(IMAN_IE);
    irq.raise();
    assert!(irq.pending());
    irq.write_iman(IMAN_IP | IMAN_IE);
    assert!(!irq.pending());
    assert!(irq.enabled());
    assert!(!irq.active());
}

#[test]
fn an_event_sets_ehb_and_erdp_clears_it() {
    let mut irq = Interrupter::new();
    irq.raise();
    assert!(irq.ehb());
    assert_eq!(irq.erdp(), ERDP_EHB);
    irq.write_erdp(0x2000 | 0x7);
    assert!(!irq.ehb());
    assert_eq!(irq.dequeue_pointer(), 0x2000);
    assert_eq!(irq.erdp(), 0x2000);
}

#[test]
fn acknowledge_does_not_clear_ehb() {
    let mut irq = Interrupter::new();
    irq.raise();
    irq.acknowledge();
    assert!(!irq.pending());
    assert!(irq.ehb());
}

#[test]
fn the_iman_register_round_trips() {
    let mut irq = Interrupter::new();
    irq.write_iman(IMAN_IE);
    assert_eq!(irq.iman(), IMAN_IE);
    irq.write_iman(0);
    assert!(!irq.enabled());
    assert!(!irq.pending());
}
