// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Cyclic deadline accuracy and ordering scenarios for `scripts/lock-stress.sh`.
//!
//! Samples are written to `LOCK_STRESS_SAMPLES` when the harness sets it. The
//! deadline test busy-waits to a monotonic deadline and reports the scheduler
//! delay as jitter.

use std::fs::OpenOptions;
use std::hint::spin_loop;
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

use raven_cyclic::{Backend, CyclicSpec, Cyclics, Level};

const ROUNDS: usize = 128;
const JITTER_SAMPLES: usize = 256;
const INTERVAL_NS: u64 = 100_000;
static SAMPLE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Default)]
struct TestBackend {
    programmed: Option<u64>,
}

impl Backend for TestBackend {
    fn program(&mut self, deadline_ns: Option<u64>) {
        self.programmed = deadline_ns;
    }

    fn now_ns(&self) -> u64 {
        0
    }
}

fn record(label: &str, nanos: u128) {
    let Ok(path) = std::env::var("LOCK_STRESS_SAMPLES") else {
        return;
    };
    let _guard = SAMPLE_LOCK.lock().expect("sample lock");
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{label}\t{nanos}");
    }
}

fn spec(when: u64, interval: u64) -> CyclicSpec {
    CyclicSpec {
        when,
        interval,
        level: Level::Normal,
        slack_ns: 0,
    }
}

#[test]
fn stress_cyclic_deadline_jitter() {
    let start = Instant::now();
    for index in 0..JITTER_SAMPLES {
        let expected = u64::try_from(index + 1).expect("sample") * INTERVAL_NS;
        let mut cyclics = Cyclics::new(TestBackend::default());
        let _id = cyclics.add(spec(expected, 0));
        while u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX) < expected {
            spin_loop();
        }
        let now = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
        let mut fired = 0usize;
        cyclics.on_interrupt(now, |_| fired += 1);
        assert_eq!(fired, 1, "lost cyclic at sample {index}");
        record("cyclic_deadline_jitter", u128::from(now - expected));
        assert_eq!(cyclics.next_deadline(), None);
    }
}

#[test]
fn stress_cyclic_order() {
    for _ in 0..ROUNDS {
        let base = 1_000_000u64;
        let mut cyclics = Cyclics::new(TestBackend::default());
        let _first = cyclics.add(spec(base + 300, 0));
        let _second = cyclics.add(spec(base + 100, 0));
        let _third = cyclics.add(spec(base + 200, 0));
        let start = Instant::now();
        let mut fired = Vec::new();
        cyclics.on_interrupt(base + 300, |id| fired.push(id));
        record("cyclic_order_fire", start.elapsed().as_nanos());
        assert_eq!(fired, vec![1, 2, 0]);
    }
}

#[test]
fn stress_cyclic_cancel() {
    for _ in 0..ROUNDS {
        let base = 2_000_000u64;
        let mut cyclics = Cyclics::new(TestBackend::default());
        let id = cyclics.add(spec(base + 10, 0));
        let start = Instant::now();
        assert!(cyclics.cancel(id));
        assert!(!cyclics.cancel(id));
        record("cyclic_cancel", start.elapsed().as_nanos());
        let mut fired = 0usize;
        cyclics.on_interrupt(base + 10, |_| fired += 1);
        assert_eq!(fired, 0, "cancelled cyclic fired");
    }
}

#[test]
fn stress_cyclic_reprogram() {
    for _ in 0..ROUNDS {
        let base = 3_000_000u64;
        let mut cyclics = Cyclics::new(TestBackend::default());
        assert_eq!(cyclics.backend().programmed, None);
        let later = cyclics.add(spec(base + 500, 0));
        assert_eq!(cyclics.backend().programmed, Some(base + 500));
        let earlier = cyclics.add(spec(base + 100, 0));
        assert_eq!(cyclics.backend().programmed, Some(base + 100));
        assert!(cyclics.cancel(earlier));
        assert_eq!(cyclics.backend().programmed, Some(base + 500));
        let _earliest = cyclics.add(spec(base + 50, 0));
        assert_eq!(cyclics.backend().programmed, Some(base + 50));
        let start = Instant::now();
        cyclics.on_interrupt(base + 50, |_| {});
        record("cyclic_reprogram", start.elapsed().as_nanos());
        assert_eq!(cyclics.backend().programmed, Some(base + 500));
        assert!(cyclics.cancel(later));
        assert_eq!(cyclics.backend().programmed, None);
    }
}

#[test]
fn stress_cyclic_coalesce() {
    for _ in 0..ROUNDS {
        let base = 4_000_000u64;
        let mut cyclics = Cyclics::new(TestBackend::default());
        for offset in [100, 200, 300] {
            let _ = cyclics.add(spec(base + offset, 0));
        }
        let start = Instant::now();
        let mut fired = Vec::new();
        cyclics.on_interrupt(base + 300, |id| fired.push(id));
        record("cyclic_coalesce", start.elapsed().as_nanos());
        assert_eq!(fired, vec![0, 1, 2]);
    }
}

#[test]
fn stress_cyclic_repeat() {
    for _ in 0..ROUNDS {
        let base = 5_000_000u64;
        let mut cyclics = Cyclics::new(TestBackend::default());
        let _id = cyclics.add(spec(base + INTERVAL_NS, INTERVAL_NS));
        let start = Instant::now();
        let mut fired = Vec::new();
        cyclics.on_interrupt(base + 3 * INTERVAL_NS, |id| fired.push(id));
        record("cyclic_repeat", start.elapsed().as_nanos());
        assert_eq!(fired, vec![0, 0, 0]);
        assert_eq!(cyclics.next_deadline(), Some(base + 4 * INTERVAL_NS));
    }
}
