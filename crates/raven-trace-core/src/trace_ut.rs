// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the span registry: the operation contract and the exhaustive
//! small-state exploration of span lifetime.

use crate::{Error, Limits, SpanId, Trace, TraceContext, TraceId};

fn ctx(trace: u64, span: u64, sampled: bool) -> TraceContext {
    TraceContext::new(TraceId::from_raw(trace), SpanId::from_raw(span), sampled)
}

#[test]
fn adopt_creates_root_span_from_a_fresh_context() {
    let mut trace = Trace::default();
    let id = trace.adopt(ctx(1, 0, false)).unwrap();
    assert_eq!(trace.trace_id(id), Some(TraceId::from_raw(1)));
    assert_eq!(trace.parent(id), None);
    assert_eq!(trace.remote_parent(id), None);
    assert!(!trace.is_sampled(id));
    assert!(trace.is_active(id));
}

#[test]
fn adopt_correlates_a_remote_parent() {
    let mut trace = Trace::default();
    let id = trace.adopt(ctx(7, 100, true)).unwrap();
    assert_eq!(trace.trace_id(id), Some(TraceId::from_raw(7)));
    assert_eq!(trace.parent(id), None);
    assert_eq!(trace.remote_parent(id), Some(SpanId::from_raw(100)));
    assert!(trace.is_sampled(id));
}

#[test]
fn adopt_rejects_a_disabled_context() {
    let mut trace = Trace::default();
    assert_eq!(trace.adopt(TraceContext::DISABLED), Err(Error::Disabled));
    assert!(trace.is_empty());
}

#[test]
fn open_child_inherits_trace_and_sampling_and_sets_parent() {
    let mut trace = Trace::default();
    let root = trace.adopt(ctx(9, 0, true)).unwrap();
    let child = trace.open_child(root).unwrap();

    assert_eq!(trace.parent(child), Some(root));
    assert_eq!(trace.remote_parent(child), None);
    assert_eq!(trace.trace_id(child), Some(TraceId::from_raw(9)));
    assert!(trace.is_sampled(child));
}

#[test]
fn open_child_requires_an_active_parent() {
    let mut trace = Trace::default();
    assert_eq!(
        trace.open_child(SpanId::from_raw(99)),
        Err(Error::NoSuchSpan)
    );

    let root = trace.adopt(ctx(1, 0, false)).unwrap();
    trace.close(root).unwrap();
    assert_eq!(trace.open_child(root), Err(Error::ParentInactive));
}

#[test]
fn close_severs_the_parent_link() {
    let mut trace = Trace::default();
    let root = trace.adopt(ctx(1, 0, false)).unwrap();
    let child = trace.open_child(root).unwrap();

    trace.close(child).unwrap();
    assert!(!trace.is_active(child));
    assert_eq!(trace.parent(child), None, "no parent after close");
    assert_eq!(trace.remote_parent(child), None);
    // The parent itself is unaffected by the child closing.
    assert!(trace.is_active(root));
}

#[test]
fn close_is_terminal_and_record_on_closed_span_errors() {
    let mut trace = Trace::default();
    let span = trace.adopt(ctx(1, 0, false)).unwrap();
    trace.close(span).unwrap();
    assert_eq!(trace.close(span), Err(Error::SpanClosed));
    assert_eq!(trace.record(span, 1), Err(Error::SpanClosed));
    assert_eq!(trace.record(SpanId::from_raw(5), 1), Err(Error::NoSuchSpan));
}

#[test]
fn record_is_bounded_and_drops_the_oldest() {
    let mut trace = Trace::with_capacity(8, 2);
    let span = trace.adopt(ctx(1, 0, false)).unwrap();

    trace.record(span, 10).unwrap();
    trace.record(span, 20).unwrap();
    assert_eq!(trace.records_len(span), Some(2));
    assert_eq!(trace.dropped(span), Some(0));
    assert_eq!(trace.record_at(span, 0), Some(10));
    assert_eq!(trace.record_at(span, 1), Some(20));

    trace.record(span, 30).unwrap();
    assert_eq!(trace.records_len(span), Some(2));
    assert_eq!(trace.dropped(span), Some(1));
    assert_eq!(trace.record_at(span, 0), Some(20));
    assert_eq!(trace.record_at(span, 1), Some(30));
}

#[test]
fn registry_is_bounded() {
    let mut trace = Trace::with_capacity(2, 2);
    let a = trace.adopt(ctx(1, 0, false)).unwrap();
    let b = trace.adopt(ctx(2, 0, false)).unwrap();
    assert_eq!(trace.adopt(ctx(3, 0, false)), Err(Error::TooManySpans));
    assert_eq!(trace.len(), 2);
    assert!(trace.contains(a));
    assert!(trace.contains(b));
}

#[test]
fn limits_clamp_to_supported_range() {
    assert_eq!(Trace::new(Limits::new(0, 0)).limits(), Limits::new(1, 1));
    assert_eq!(
        Trace::new(Limits::new(usize::MAX, usize::MAX)).limits(),
        Limits::new(4096, 4096)
    );
    assert_eq!(Trace::new(Limits::new(3, 5)).limits(), Limits::new(3, 5));
}

// --- exhaustive small-state exploration -------------------------------------

const MAX_DEPTH: usize = 8;
const MAX_STATES: usize = 200_000;

fn contexts() -> [TraceContext; 4] {
    [
        TraceContext::DISABLED,
        ctx(1, 0, false),
        ctx(1, 100, true),
        ctx(2, 0, true),
    ]
}

const CANDIDATES: [SpanId; 4] = [
    SpanId::from_raw(1),
    SpanId::from_raw(2),
    SpanId::from_raw(3),
    SpanId::from_raw(4),
];

const EVENTS: [u64; 2] = [0, 1];

#[derive(Clone, Copy, Debug)]
enum Action {
    Adopt(TraceContext),
    Child(SpanId),
    Close(SpanId),
    Record(SpanId, u64),
}

#[derive(Clone, Debug)]
struct Node {
    trace: Trace,
    depth: usize,
}

fn actions() -> impl Iterator<Item = Action> {
    let adopt = contexts().into_iter().map(Action::Adopt);
    let child = CANDIDATES.iter().copied().map(Action::Child);
    let close = CANDIDATES.iter().copied().map(Action::Close);
    let record = CANDIDATES
        .iter()
        .copied()
        .flat_map(|s| EVENTS.iter().copied().map(move |e| Action::Record(s, e)));
    adopt.chain(child).chain(close).chain(record)
}

fn apply(node: &Node, action: Action) -> (Node, Result<Option<SpanId>, Error>) {
    let mut next = node.clone();
    next.depth += 1;
    let result = match action {
        Action::Adopt(ctx) => next.trace.adopt(ctx).map(Some),
        Action::Child(parent) => next.trace.open_child(parent).map(Some),
        Action::Close(span) => next.trace.close(span).map(|_| None),
        Action::Record(span, event) => next.trace.record(span, event).map(|_| None),
    };
    (next, result)
}

fn check_invariants(trace: &Trace) {
    let limits = trace.limits();
    assert!(trace.len() <= limits.max_spans);
    for span in trace.spans() {
        assert!(!trace.trace_id(span).unwrap().is_zero());
        assert!(trace.records_len(span).unwrap() <= limits.max_records);
        if !trace.is_active(span) {
            assert_eq!(trace.parent(span), None, "closed span still has a parent");
            assert_eq!(
                trace.remote_parent(span),
                None,
                "closed span still has a remote parent"
            );
        }
    }
}

#[test]
fn exhaustive_span_lifetime_preserves_no_parent_after_close() {
    use std::collections::{HashSet, VecDeque};

    let root = Node {
        trace: Trace::new(Limits::new(3, 2)),
        depth: 0,
    };

    let mut seen: HashSet<Trace> = HashSet::new();
    let mut queue: VecDeque<Node> = VecDeque::new();
    seen.insert(root.trace.clone());
    queue.push_back(root);

    let mut states = 0usize;
    let mut transitions = 0usize;
    let mut max_depth = 0usize;

    while let Some(node) = queue.pop_front() {
        states += 1;
        max_depth = max_depth.max(node.depth);
        check_invariants(&node.trace);

        for action in actions() {
            transitions += 1;
            let before = node.trace.clone();
            let (next, result) = apply(&node, action);

            match action {
                Action::Adopt(ctx) => match result {
                    Ok(Some(id)) => {
                        assert!(!ctx.trace_id.is_zero());
                        assert!(before.len() < before.limits().max_spans);
                        assert_eq!(next.trace.len(), before.len() + 1);
                        assert_eq!(next.trace.trace_id(id), Some(ctx.trace_id));
                        assert_eq!(next.trace.parent(id), None);
                        assert_eq!(
                            next.trace.remote_parent(id),
                            if ctx.span_id.is_none() {
                                None
                            } else {
                                Some(ctx.span_id)
                            }
                        );
                        assert_eq!(next.trace.is_sampled(id), ctx.sampled);
                        assert!(next.trace.is_active(id));
                    }
                    Ok(None) => unreachable!("adopt never returns Ok(None)"),
                    Err(error) => {
                        if ctx.trace_id.is_zero() {
                            assert_eq!(error, Error::Disabled);
                        } else {
                            assert_eq!(error, Error::TooManySpans);
                            assert_eq!(before.len(), before.limits().max_spans);
                        }
                        assert_eq!(next.trace, before);
                    }
                },
                Action::Child(parent) => match result {
                    Ok(Some(id)) => {
                        assert!(before.is_active(parent));
                        assert!(before.len() < before.limits().max_spans);
                        assert_eq!(next.trace.parent(id), Some(parent));
                        assert_eq!(next.trace.remote_parent(id), None);
                        assert_eq!(next.trace.trace_id(id), before.trace_id(parent));
                        assert_eq!(next.trace.is_sampled(id), before.is_sampled(parent));
                        assert!(next.trace.is_active(id));
                    }
                    Ok(None) => unreachable!("open_child never returns Ok(None)"),
                    Err(error) => {
                        if !before.contains(parent) {
                            assert_eq!(error, Error::NoSuchSpan);
                        } else if !before.is_active(parent) {
                            assert_eq!(error, Error::ParentInactive);
                        } else {
                            assert_eq!(error, Error::TooManySpans);
                            assert_eq!(before.len(), before.limits().max_spans);
                        }
                        assert_eq!(next.trace, before);
                    }
                },
                Action::Close(span) => match result {
                    Ok(None) => {
                        assert!(before.is_active(span));
                        assert!(!next.trace.is_active(span));
                        assert_eq!(next.trace.parent(span), None);
                        assert_eq!(next.trace.remote_parent(span), None);
                    }
                    Ok(Some(_)) => unreachable!("close never returns Ok(Some)"),
                    Err(error) => {
                        if !before.contains(span) {
                            assert_eq!(error, Error::NoSuchSpan);
                        } else {
                            assert_eq!(error, Error::SpanClosed);
                        }
                        assert_eq!(next.trace, before);
                    }
                },
                Action::Record(span, event) => match result {
                    Ok(None) => {
                        assert!(before.is_active(span));
                        let max_records = before.limits().max_records;
                        let was_full = before.records_len(span) == Some(max_records);
                        assert_eq!(
                            next.trace.records_len(span),
                            Some(if was_full {
                                max_records
                            } else {
                                before.records_len(span).unwrap() + 1
                            })
                        );
                        assert_eq!(
                            next.trace.dropped(span),
                            if was_full {
                                Some(before.dropped(span).unwrap() + 1)
                            } else {
                                before.dropped(span)
                            }
                        );
                        let last = next.trace.records_len(span).unwrap() - 1;
                        assert_eq!(next.trace.record_at(span, last), Some(event));
                    }
                    Ok(Some(_)) => unreachable!("record never returns Ok(Some)"),
                    Err(error) => {
                        if !before.contains(span) {
                            assert_eq!(error, Error::NoSuchSpan);
                        } else {
                            assert_eq!(error, Error::SpanClosed);
                        }
                        assert_eq!(next.trace, before);
                    }
                },
            }

            check_invariants(&next.trace);

            if next.depth < MAX_DEPTH && seen.insert(next.trace.clone()) {
                assert!(
                    states < MAX_STATES,
                    "state space did not saturate within bound"
                );
                queue.push_back(next);
            }
        }
    }

    println!(
        "exhaustive span model: {states} states, {transitions} transitions, max depth {max_depth}"
    );
    assert!(states > 1);
    assert!(max_depth <= MAX_DEPTH);
}
