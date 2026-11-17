// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exhaustive small-state model check of the turnstile registry.
//!
//! Breadth-first search over every action sequence up to a fixed depth, using the *real*
//! transition functions and checking the real invariants after every step. The frontier and the
//! visited set are deterministic (`HashSet` with a canonical iteration order for the frontier via
//! `VecDeque`), so the state count and any counterexample trace are reproducible.

use super::*;
use alloc::vec;
use alloc::vec::Vec;
use std::collections::{HashSet, VecDeque};
use std::println;

/// A mutation the model can apply.
#[derive(Clone, Copy, Debug)]
enum Act {
    Acquire(ObjectId, ThreadId),
    Block(ObjectId, ThreadId),
    WakeOne(ObjectId),
    WakeAll(ObjectId),
    Cancel(ObjectId, ThreadId),
    Boost(ObjectId),
    Unboost(ObjectId),
}

/// A fixed scenario: registered threads (with base priorities) and the objects in play.
struct Scenario {
    threads: Vec<(ThreadId, Priority)>,
    objects: Vec<ObjectId>,
}

impl Scenario {
    fn init(&self) -> Turnstiles {
        let mut t = Turnstiles::new();
        for &(thread, priority) in &self.threads {
            t.register(thread, priority).expect("register");
        }
        t
    }

    fn actions(&self, s: &Turnstiles) -> Vec<Act> {
        let mut acts = Vec::new();
        for &object in &self.objects {
            for &(thread, _) in &self.threads {
                if s.owner(object).is_none() {
                    acts.push(Act::Acquire(object, thread));
                }
                if let Some(owner) = s.owner(object)
                    && owner != thread
                    && s.waiting_on(thread).is_none()
                {
                    acts.push(Act::Block(object, thread));
                }
                if s.is_waiting(object, thread) {
                    acts.push(Act::Cancel(object, thread));
                }
            }
            if !s.waiters(object).is_empty() {
                acts.push(Act::WakeOne(object));
                acts.push(Act::WakeAll(object));
            }
            if s.owner(object).is_some() {
                acts.push(Act::Boost(object));
                if s.waiters(object).is_empty() {
                    acts.push(Act::Unboost(object));
                }
            }
        }
        acts
    }

    fn step(&self, s: &Turnstiles, act: Act) -> Option<Turnstiles> {
        let mut next = s.clone();
        let accepted = match act {
            Act::Acquire(object, thread) => next.acquire(object, thread).is_ok(),
            Act::Block(object, thread) => next.block(object, thread).is_ok(),
            Act::WakeOne(object) => {
                let _ = next.wake_one(object);
                true
            }
            Act::WakeAll(object) => {
                let _ = next.wake_all(object);
                true
            }
            Act::Cancel(object, thread) => next.cancel(object, thread).removed,
            Act::Boost(object) => next.boost(object).is_ok(),
            Act::Unboost(object) => next.unboost(object).is_ok(),
        };
        accepted.then_some(next)
    }
}

fn explore(scenario: &Scenario, max_depth: u32) -> (u64, u32) {
    let init = scenario.init();
    init.check_invariants().expect("initial state");
    let mut visited = HashSet::new();
    visited.insert(init.clone());
    let mut frontier = VecDeque::new();
    frontier.push_back((init, Vec::new()));
    let mut states = 1u64;
    let mut reached = 0u32;
    while let Some((state, trace)) = frontier.pop_front() {
        let depth = trace.len() as u32;
        reached = reached.max(depth);
        if depth >= max_depth {
            continue;
        }
        for act in scenario.actions(&state) {
            let Some(next) = scenario.step(&state, act) else {
                continue;
            };
            if let Err(violation) = next.check_invariants() {
                let mut full = trace.clone();
                full.push(act);
                panic!(
                    "{violation:?} violated at depth {}; trace: {full:?}",
                    depth + 1
                );
            }
            #[cfg(feature = "lockstat")]
            for &object in &scenario.objects {
                let stats = next.stats(object);
                let waiters = next.waiter_count(object) as u64;
                if stats.timeouts != 0 {
                    panic!(
                        "lockstat: timeouts nonzero at depth {}; trace: {trace:?}",
                        depth + 1
                    );
                }
                if stats.max_queue_depth < waiters {
                    panic!(
                        "lockstat: max_queue_depth {} < current waiters {waiters} at depth {}",
                        stats.max_queue_depth,
                        depth + 1
                    );
                }
                if waiters > 0 && stats.max_inheritance_depth == 0 {
                    panic!("lockstat: max_inheritance_depth 0 with {waiters} waiters");
                }
                if stats.wait_entries < waiters {
                    panic!(
                        "lockstat: wait_entries {} < current waiters {waiters}",
                        stats.wait_entries
                    );
                }
            }
            if visited.insert(next.clone()) {
                let mut full = trace.clone();
                full.push(act);
                states += 1;
                frontier.push_back((next, full));
            }
        }
    }
    (states, reached)
}

#[test]
fn exhaustive_two_threads_two_objects_two_priorities() {
    let scenario = Scenario {
        threads: vec![(0, Priority::new(0)), (1, Priority::new(1))],
        objects: vec![0, 1],
    };
    let (states, depth) = explore(&scenario, 6);
    println!("2-thread model: states={states} max_depth={depth}");
    assert!(
        states >= 100,
        "expected a meaningful state space, got {states}"
    );
}

#[test]
fn exhaustive_three_threads_one_object_two_priorities() {
    let scenario = Scenario {
        threads: vec![
            (0, Priority::new(0)),
            (1, Priority::new(0)),
            (2, Priority::new(1)),
        ],
        objects: vec![0],
    };
    let (states, depth) = explore(&scenario, 7);
    println!("3-thread model: states={states} max_depth={depth}");
    assert!(
        states >= 50,
        "expected a meaningful state space, got {states}"
    );
}
