// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pure trace id, span and correlation model.

use alloc::collections::BTreeSet;

use super::*;

#[test]
fn ids_are_nonzero_and_unique() {
    let mut ids = IdGenerator::new(0);
    let mut traces = BTreeSet::new();
    let mut spans = BTreeSet::new();
    let mut index = 0u64;
    while index < 10_000 {
        let trace = ids.next_trace();
        let span = ids.next_span();
        assert_ne!(trace.raw(), 0);
        assert_ne!(span.raw(), 0);
        assert!(traces.insert(trace.raw()));
        assert!(spans.insert(span.raw()));
        index = index.saturating_add(1);
    }
    assert!(trace_id_rejects_zero());
    assert!(span_id_rejects_zero());
}

fn trace_id_rejects_zero() -> bool {
    TraceId::new(0).is_none() && TraceId::new(1).is_some()
}

fn span_id_rejects_zero() -> bool {
    SpanId::new(0).is_none() && SpanId::new(1).is_some()
}

#[test]
fn sampler_is_deterministic_and_bounded() {
    assert!(Sampler::always().sample(TraceId::new(1).unwrap()));
    assert!(!Sampler::never().sample(TraceId::new(1).unwrap()));
    assert_eq!(Sampler::new(1001).rate_per_mille(), 1000);
    assert_eq!(Sampler::new(0).rate_per_mille(), 0);

    let sampler = Sampler::new(500);
    let mut ids = IdGenerator::new(0x1234);
    let mut sampled = 0usize;
    let mut index = 0usize;
    while index < 10_000 {
        let trace = ids.next_trace();
        let decision = sampler.sample(trace);
        assert_eq!(decision, sampler.sample(trace));
        if decision {
            sampled = sampled.saturating_add(1);
        }
        index = index.saturating_add(1);
    }
    assert!((4_500..=5_500).contains(&sampled), "sampled={sampled}");
}

#[test]
fn disabled_tracer_is_the_no_allocation_fast_path() {
    let mut tracer = Tracer::new(false, 8);
    let mut ids = IdGenerator::new(7);
    assert!(
        tracer
            .start_root(1, 10, &mut ids, Sampler::always())
            .is_none()
    );
    assert!(tracer.is_empty());
    assert_eq!(tracer.len(), 0);
    assert_eq!(tracer.dropped(), 0);
}

#[test]
fn root_and_child_record_the_parent_chain() {
    let mut ids = IdGenerator::new(7);
    let mut tracer = Tracer::new(true, 8);
    let root = tracer
        .start_root(1, 10, &mut ids, Sampler::always())
        .expect("root");
    let child = tracer
        .start_child(root, 2, 20, &mut ids)
        .expect("child result")
        .expect("sampled child");
    assert_eq!(tracer.len(), 2);
    assert!(tracer.records()[0].is_open());
    assert_eq!(tracer.records()[0].context().parent(), None);
    assert_eq!(tracer.records()[1].context().parent(), Some(root.span()));
    assert_eq!(
        tracer.records()[0].context().trace(),
        tracer.records()[1].context().trace()
    );
    assert!(tracer.records()[0].context().sampled());
    assert!(tracer.records()[1].context().sampled());

    tracer.close(root, 30, 0).expect("close root");
    tracer.close(child, 40, 7).expect("close child");
    assert_eq!(tracer.records()[0].end_ns(), Some(30));
    assert_eq!(tracer.records()[1].status(), 7);
    assert_eq!(tracer.close(root, 50, 0), Err(TraceError::AlreadyClosed));
    assert_eq!(
        tracer.start_child(root, 3, 60, &mut ids),
        Err(TraceError::ParentClosed)
    );
}

#[test]
fn unsampled_root_records_nothing() {
    let mut ids = IdGenerator::new(9);
    let mut tracer = Tracer::new(true, 8);
    assert!(
        tracer
            .start_root(1, 10, &mut ids, Sampler::never())
            .is_none()
    );
    assert!(tracer.is_empty());
}

#[test]
fn a_full_store_drops_and_counts_new_spans() {
    let mut ids = IdGenerator::new(11);
    let mut tracer = Tracer::new(true, 1);
    assert!(
        tracer
            .start_root(1, 10, &mut ids, Sampler::always())
            .is_some()
    );
    assert!(
        tracer
            .start_root(2, 20, &mut ids, Sampler::always())
            .is_none()
    );
    assert_eq!(tracer.len(), 1);
    assert_eq!(tracer.dropped(), 1);
}

#[test]
fn zero_capacity_still_stores_one_record() {
    let mut ids = IdGenerator::new(12);
    let mut tracer = Tracer::new(true, 0);
    assert!(
        tracer
            .start_root(1, 10, &mut ids, Sampler::always())
            .is_some()
    );
    assert_eq!(tracer.capacity(), 1);
}

#[test]
fn a_handle_from_another_tracer_is_rejected() {
    let mut ids = IdGenerator::new(13);
    let mut first = Tracer::new(true, 4);
    let mut second = Tracer::new(true, 4);
    let handle = first
        .start_root(1, 10, &mut ids, Sampler::always())
        .expect("first");
    assert_eq!(second.close(handle, 20, 0), Err(TraceError::UnknownSpan));
}

#[test]
fn client_server_correlation_shares_the_trace_and_links_the_parent() {
    let mut ids = IdGenerator::new(14);
    let trace = ids.next_trace();
    let client = SpanContext::root(trace, ids.next_span(), true);
    let call = CallContext::new(client, &mut ids);
    assert_eq!(call.client(), client);
    assert_eq!(call.server().trace(), client.trace());
    assert_eq!(call.server().parent(), Some(client.span()));
    assert_eq!(call.reply(), client);
    assert!(call.server().sampled());

    let unsampled = SpanContext::root(trace, ids.next_span(), false);
    let call = CallContext::new(unsampled, &mut ids);
    assert!(!call.server().sampled());
    assert!(!call.reply().sampled());
}

#[test]
fn exhaustive_small_state_span_lifetime() {
    // Exhaustive 4^5 operation sequences over one tracer: root, child, close, close-child.
    let mut sequence = 0u32;
    while sequence < 1024 {
        let mut tracer = Tracer::new(true, 4);
        let mut ids = IdGenerator::new(u64::from(sequence));
        let mut root: Option<SpanHandle> = None;
        let mut child: Option<SpanHandle> = None;
        let mut root_closed = false;
        let mut op = sequence;
        let mut step = 0usize;
        while step < 5 {
            match op & 3 {
                0 => {
                    if root.is_none() {
                        root = tracer.start_root(1, 10, &mut ids, Sampler::always());
                    }
                }
                1 => {
                    if let Some(handle) = root {
                        let result = tracer.start_child(handle, 2, 20, &mut ids);
                        if root_closed {
                            assert_eq!(result, Err(TraceError::ParentClosed));
                        } else {
                            let started = result.expect("open root child");
                            child = child.or(started);
                        }
                    }
                }
                2 => {
                    if let Some(handle) = root {
                        let result = tracer.close(handle, 30, 0);
                        if root_closed {
                            assert_eq!(result, Err(TraceError::AlreadyClosed));
                        } else {
                            result.expect("first root close");
                            root_closed = true;
                        }
                    }
                }
                _ => {
                    if let Some(handle) = child {
                        let first = tracer.close(handle, 40, 1);
                        let second = tracer.close(handle, 50, 1);
                        assert!(first.is_ok() || first == Err(TraceError::AlreadyClosed));
                        assert_eq!(second, Err(TraceError::AlreadyClosed));
                    }
                }
            }
            op >>= 2;
            step = step.saturating_add(1);
        }
        assert!(tracer.len() <= 4);
        assert_eq!(tracer.records().len(), tracer.len());
        sequence = sequence.saturating_add(1);
    }
}
