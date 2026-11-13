// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Multi-threaded stress and cross-thread free (doc 08 7.4).

mod common;

use std::sync::{Arc, Barrier, Mutex};

use common::{MmapProvider, SpinLock, TestEntropy, TestPlatform};
use raven_libc_malloc::{Hardened, Heap};

fn operations() -> usize {
    std::env::var("RL_THREAD_OPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(200_000)
}

fn stress(options: &[u8]) {
    let heap = Arc::new(Hardened::new(
        MmapProvider::new(),
        SpinLock::new(),
        TestEntropy::new(13),
        TestPlatform::new(1),
        options,
    ));
    let baseline = heap.info();
    let shared = Arc::new(Mutex::new(Vec::new()));
    let barrier = Arc::new(Barrier::new(8));
    let ops = operations();
    let mut handles = Vec::new();
    for id in 0..8usize {
        let heap = Arc::clone(&heap);
        let shared = Arc::clone(&shared);
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            heap.platform()
                .thread
                .store(id + 1, std::sync::atomic::Ordering::Relaxed);
            let mut local = Vec::new();
            let mut index = 0usize;
            while index < ops {
                let size = (index % 2048) + 1;
                let pointer = heap.malloc(size);
                if pointer != 0 {
                    local.push(pointer);
                }
                if local.len() >= 64 {
                    let pointer = local.remove(0);
                    heap.free(pointer);
                }
                index += 1;
            }
            barrier.wait();
            {
                let mut shared = shared.lock().expect("shared");
                shared.extend(local.drain(..));
            }
            barrier.wait();
            loop {
                let pointer = {
                    let mut shared = shared.lock().expect("shared");
                    shared.pop()
                };
                let Some(pointer) = pointer else {
                    break;
                };
                heap.free(pointer);
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    let after = heap.info();
    assert!(after.uordblks < baseline.uordblks.saturating_add(2 * 1024 * 1024));
}

#[test]
fn threads_default_options() {
    stress(b"");
}

#[test]
fn threads_more_pools() {
    stress(b"+");
}

#[test]
fn threads_fewer_pools() {
    stress(b"-");
}

#[test]
fn threads_security_set() {
    stress(b"S");
}
