// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for trace and span identifiers.

use crate::{COUNTER_MASK, SpanId, TraceId, TraceIdGen};

#[test]
fn trace_id_packs_and_unpacks_fields() {
    let id = TraceId::new(0xABCD, 0x42, 0x1234_5678);
    assert_eq!(id.boot_nonce(), 0xABCD);
    assert_eq!(id.cpu(), 0x42);
    assert_eq!(id.counter(), 0x1234_5678);
    assert_eq!(
        id.raw(),
        (0xABCD_0000_0000_0000u64) | (0x42u64 << 40) | 0x1234_5678
    );
}

#[test]
fn trace_id_counter_is_truncated_to_40_bits() {
    let id = TraceId::new(0, 0, (1u64 << 40) | 7);
    assert_eq!(id.counter(), 7);
}

#[test]
fn trace_id_zero_and_default_are_not_traced() {
    assert!(TraceId::ZERO.is_zero());
    assert!(TraceId::default().is_zero());
    assert!(!TraceId::new(1, 0, 1).is_zero());
}

#[test]
fn generator_is_monotonic_and_never_emits_zero() {
    let mut generator = TraceIdGen::new(0x0001, 0x02);
    assert_eq!(generator.boot_nonce(), 0x0001);
    assert_eq!(generator.cpu(), 0x02);

    let first = generator.next_id();
    assert_eq!(first.counter(), 1);
    let second = generator.next_id();
    assert_eq!(second.counter(), 2);
    assert_eq!(second.raw(), first.raw() + 1);
    assert!(!first.is_zero());
    assert!(!second.is_zero());
}

#[test]
fn generator_counter_wraps_to_one_not_zero() {
    let mut generator = TraceIdGen::with_counter(0, 0, COUNTER_MASK);
    let id = generator.next_id();
    assert_eq!(id.counter(), 1);
    assert!(!id.is_zero());
}

#[test]
fn span_id_none_and_default() {
    assert!(SpanId::NONE.is_none());
    assert!(SpanId::default().is_none());
    assert!(!SpanId::from_raw(1).is_none());
    assert_eq!(SpanId::from_raw(99).raw(), 99);
}
