// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::lock`]: recursion, trylock, the fast path (trap S5), BYCALLER and
//! contention on real host threads.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::bool_assert_comparison,
    clippy::undocumented_unsafe_blocks,
    clippy::uninlined_format_args
)]

use super::*;
use crate::fake_ut::{FakeEnv, leaked_file};
use crate::file::F_BYCALLER;

#[test]
fn recursive_lock_returns_false() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    assert!(lockfile(file));
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        state.env().tid()
    );
    assert!(!lockfile(file));
    unlockfile(file);
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
    // A fresh acquisition after the release works.
    assert!(lockfile(file));
    unlockfile(file);
}

#[test]
fn explicit_trylock_nesting_and_funlock() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    assert_eq!(ftrylockfile(file), 0);
    assert_eq!(file.st().lockcount, 1);
    assert_eq!(ftrylockfile(file), 0);
    assert_eq!(file.st().lockcount, 2);
    funlockfile(file);
    assert_eq!(file.st().lockcount, 1);
    funlockfile(file);
    assert_eq!(file.st().lockcount, 0);
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
    // Unlocking an unlocked stream is a no-op.
    funlockfile(file);
    assert_eq!(file.st().lockcount, 0);
}

#[test]
fn flockfile_is_explicit_and_recursive() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    flockfile(file);
    assert_eq!(file.st().lockcount, 1);
    flockfile(file);
    assert_eq!(file.st().lockcount, 2);
    funlockfile(file);
    funlockfile(file);
    assert_eq!(file.st().lockcount, 0);
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn fast_path_guard_does_not_release_an_unowned_lock() {
    let env = FakeEnv::new();
    env.set_threaded(false);
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    {
        let _guard = FileGuard::new(file);
        assert_eq!(
            file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
            0
        );
        state.env().set_threaded(true);
    }
    // The guard was created in fast mode and must not unlock anything (trap S5).
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
    assert!(lockfile(file));
    unlockfile(file);
}

#[test]
fn bycaller_guard_is_a_noop() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    file.st_mut().flags |= F_BYCALLER;
    {
        let _guard = FileGuard::new(file);
        assert_eq!(
            file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
            0
        );
    }
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn eight_threads_contend_on_one_file() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = leaked_file(state);
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(std::thread::spawn(move || {
            for _ in 0..2_000 {
                flockfile(file);
                funlockfile(file);
            }
        }));
    }
    for handle in handles {
        handle.join().expect("thread");
    }
    assert_eq!(file.st().lockcount, 0);
    assert_eq!(
        file.st().lock.load(core::sync::atomic::Ordering::Relaxed),
        0
    );
}
