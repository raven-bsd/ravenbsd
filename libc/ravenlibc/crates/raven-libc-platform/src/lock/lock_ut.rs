// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Lock module tests.

use crate::lock::{Lock, NoLock, guard};

#[test]
fn no_lock_is_transparent() {
    let lock = NoLock;
    let _guard = guard(&lock);
    let _ = lock;
}

#[test]
fn guard_releases_on_drop() {
    struct Counting(std::sync::atomic::AtomicUsize);
    impl Lock for Counting {
        fn acquire(&self) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        fn release(&self) {
            self.0.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let lock = Counting(std::sync::atomic::AtomicUsize::new(0));
    {
        let _guard = guard(&lock);
        assert_eq!(lock.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
    assert_eq!(lock.0.load(std::sync::atomic::Ordering::SeqCst), 0);
}
