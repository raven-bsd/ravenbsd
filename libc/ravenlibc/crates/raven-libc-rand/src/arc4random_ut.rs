// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::cell::Cell;

use raven_libc_malloc::Entropy;

struct FakeEntropy {
    calls: Cell<usize>,
    bytes: Cell<usize>,
    state: Cell<u64>,
    fail: Cell<bool>,
}

impl FakeEntropy {
    fn new(seed: u64) -> Self {
        Self {
            calls: Cell::new(0),
            bytes: Cell::new(0),
            state: Cell::new(seed | 1),
            fail: Cell::new(false),
        }
    }

    fn fail_after(&self) {
        self.fail.set(true);
    }
}

impl Entropy for FakeEntropy {
    fn fill(&self, buf: &mut [u8]) -> bool {
        if self.fail.get() {
            return false;
        }
        self.calls.set(self.calls.get() + 1);
        self.bytes.set(self.bytes.get() + buf.len());
        let mut value = self.state.get();
        for byte in buf {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            *byte = value as u8;
        }
        self.state.set(value);
        true
    }
}

struct Fatal;

impl RandPlatform for Fatal {
    fn no_entropy(&self) -> ! {
        panic!("no entropy marker")
    }
}

fn fake_fork<R: ForkEvents + Clone>(parent: &mut R) -> R {
    parent.prefork();
    let mut child = parent.clone();
    parent.postfork_parent();
    child.postfork_child();
    child
}

#[test]
fn a_new_generator_is_wiped() {
    assert!(Arc4Random::new().is_wiped());
}

#[test]
fn first_draw_stirs_once_with_forty_bytes() {
    let entropy = FakeEntropy::new(1);
    let mut random = Arc4Random::new();
    let mut out = [0u8; 1];
    random.random_buf(&entropy, &Fatal, &mut out);
    assert_eq!(entropy.calls.get(), 1);
    assert_eq!(entropy.bytes.get(), KEYSZ + IVSZ);
}

#[test]
fn zero_length_draw_does_not_stir() {
    let entropy = FakeEntropy::new(1);
    let mut random = Arc4Random::new();
    random.random_buf(&entropy, &Fatal, &mut []);
    assert_eq!(entropy.calls.get(), 0);
    assert!(random.is_wiped());
}

#[test]
fn same_entropy_is_deterministic() {
    let first_entropy = FakeEntropy::new(42);
    let second_entropy = FakeEntropy::new(42);
    let mut first = Arc4Random::new();
    let mut second = Arc4Random::new();
    let mut left = [0u8; 4097];
    let mut right = [0u8; 4097];
    first.random_buf(&first_entropy, &Fatal, &mut left);
    second.random_buf(&second_entropy, &Fatal, &mut right);
    assert_eq!(left, right);
}

#[test]
fn different_entropy_differs() {
    let first_entropy = FakeEntropy::new(1);
    let second_entropy = FakeEntropy::new(2);
    let mut first = Arc4Random::new();
    let mut second = Arc4Random::new();
    let mut left = [0u8; 64];
    let mut right = [0u8; 64];
    first.random_buf(&first_entropy, &Fatal, &mut left);
    second.random_buf(&second_entropy, &Fatal, &mut right);
    assert_ne!(left, right);
}

#[test]
fn buffer_lengths_work() {
    for length in [0usize, 1, RSBUFSZ, RSBUFSZ + 1, 4097] {
        let entropy = FakeEntropy::new(length as u64 + 1);
        let mut random = Arc4Random::new();
        let mut out = std::vec![0u8; length];
        random.random_buf(&entropy, &Fatal, &mut out);
        if length != 0 {
            assert!(out.iter().any(|byte| *byte != 0), "length {length}");
        }
    }
}

#[test]
fn stir_forces_a_new_fill() {
    let entropy = FakeEntropy::new(9);
    let mut random = Arc4Random::new();
    let mut out = [0u8; 32];
    random.random_buf(&entropy, &Fatal, &mut out);
    assert_eq!(entropy.calls.get(), 1);
    random.stir();
    random.random_buf(&entropy, &Fatal, &mut out);
    assert_eq!(entropy.calls.get(), 2);
}

#[test]
fn a_large_request_stirs_once() {
    let entropy = FakeEntropy::new(11);
    let mut random = Arc4Random::new();
    let mut out = std::vec![0u8; 4 * 1024 * 1024];
    random.random_buf(&entropy, &Fatal, &mut out);
    assert_eq!(entropy.calls.get(), 1);
}

#[test]
fn handed_out_bytes_are_zeroed_in_the_buffer() {
    let entropy = FakeEntropy::new(3);
    let mut random = Arc4Random::new();
    let _ = random.random_u32(&entropy, &Fatal);
    assert_eq!(&random.buffer()[..4], &[0; 4]);
}

#[test]
fn random_u32_advances_the_countdown() {
    let entropy = FakeEntropy::new(5);
    let mut random = Arc4Random::new();
    let _ = random.random_u32(&entropy, &Fatal);
    let before = random.count();
    let _ = random.random_u32(&entropy, &Fatal);
    assert!(random.count() < before);
}

#[test]
fn fork_child_is_wiped_and_parent_is_not() {
    let entropy = FakeEntropy::new(7);
    let mut parent = Arc4Random::new();
    let mut out = [0u8; 64];
    parent.random_buf(&entropy, &Fatal, &mut out);
    let child = fake_fork(&mut parent);
    assert!(child.is_wiped());
    assert!(!parent.is_wiped());
}

#[test]
fn child_reseeds_once_and_streams_differ() {
    let entropy = FakeEntropy::new(13);
    let mut parent = Arc4Random::new();
    let mut parent_out = [0u8; 1024];
    parent.random_buf(&entropy, &Fatal, &mut parent_out);
    let calls_before = entropy.calls.get();
    let mut child = fake_fork(&mut parent);
    let mut child_out = [0u8; 1024];
    child.random_buf(&entropy, &Fatal, &mut child_out);
    assert_eq!(entropy.calls.get(), calls_before + 1);
    assert_ne!(child_out, parent_out);
}

#[test]
fn parent_continuation_and_child_stream_differ() {
    let entropy = FakeEntropy::new(17);
    let mut parent = Arc4Random::new();
    let mut first = [0u8; 256];
    parent.random_buf(&entropy, &Fatal, &mut first);
    let mut child = fake_fork(&mut parent);
    let mut parent_next = [0u8; 256];
    let mut child_next = [0u8; 256];
    parent.random_buf(&entropy, &Fatal, &mut parent_next);
    child.random_buf(&entropy, &Fatal, &mut child_next);
    assert_ne!(parent_next, child_next);
}

#[test]
fn a_child_can_fork_again() {
    let entropy = FakeEntropy::new(19);
    let mut parent = Arc4Random::new();
    let mut out = [0u8; 64];
    parent.random_buf(&entropy, &Fatal, &mut out);
    let mut child = fake_fork(&mut parent);
    let grandchild = fake_fork(&mut child);
    assert!(grandchild.is_wiped());
    assert!(child.is_wiped());
}

#[test]
fn uniform_zero_and_one_do_not_stir() {
    let entropy = FakeEntropy::new(23);
    let mut random = Arc4Random::new();
    assert_eq!(random.uniform(&entropy, &Fatal, 0), 0);
    assert_eq!(random.uniform(&entropy, &Fatal, 1), 0);
    assert_eq!(entropy.calls.get(), 0);
}

#[test]
fn uniform_values_stay_in_range() {
    let entropy = FakeEntropy::new(29);
    let mut random = Arc4Random::new();
    for bound in [2u32, 3, 5, 10, 1000, 1 << 31, u32::MAX] {
        let value = random.uniform(&entropy, &Fatal, bound);
        assert!(value < bound, "bound {bound} value {value}");
    }
}

#[test]
fn first_entropy_failure_is_fatal() {
    let entropy = FakeEntropy::new(31);
    entropy.fail_after();
    let mut random = Arc4Random::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut out = [0u8; 4];
        random.random_buf(&entropy, &Fatal, &mut out);
    }));
    assert!(result.is_err());
}

#[test]
fn later_stir_failure_is_fatal() {
    let entropy = FakeEntropy::new(37);
    let mut random = Arc4Random::new();
    let mut out = [0u8; 4];
    random.random_buf(&entropy, &Fatal, &mut out);
    entropy.fail_after();
    random.stir();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        random.random_buf(&entropy, &Fatal, &mut out);
    }));
    assert!(result.is_err());
}

#[test]
fn repeated_draws_are_not_constant() {
    let entropy = FakeEntropy::new(41);
    let mut random = Arc4Random::new();
    let first = random.random_u32(&entropy, &Fatal);
    let second = random.random_u32(&entropy, &Fatal);
    assert_ne!(first, second);
}

#[test]
fn child_wipes_the_key_words() {
    let entropy = FakeEntropy::new(43);
    let mut parent = Arc4Random::new();
    let mut out = [0u8; 64];
    parent.random_buf(&entropy, &Fatal, &mut out);
    let child = fake_fork(&mut parent);
    assert_eq!(child.key_words(), [0; 16]);
}

#[test]
fn parent_hook_is_a_no_op() {
    let entropy = FakeEntropy::new(47);
    let mut random = Arc4Random::new();
    let mut before = [0u8; 32];
    random.random_buf(&entropy, &Fatal, &mut before);
    random.postfork_parent();
    let mut after = [0u8; 32];
    random.random_buf(&entropy, &Fatal, &mut after);
    assert_ne!(before, after);
}

#[test]
fn wiped_generator_can_draw_again() {
    let entropy = FakeEntropy::new(53);
    let mut random = Arc4Random::new();
    let mut out = [0u8; 32];
    random.random_buf(&entropy, &Fatal, &mut out);
    random.postfork_child();
    assert!(random.is_wiped());
    random.random_buf(&entropy, &Fatal, &mut out);
    assert!(!random.is_wiped());
}

#[test]
fn random_u32_is_not_always_zero() {
    let entropy = FakeEntropy::new(59);
    let mut random = Arc4Random::new();
    let mut any = false;
    for _ in 0..16 {
        any |= random.random_u32(&entropy, &Fatal) != 0;
    }
    assert!(any);
}

#[test]
fn fork_hook_wipes_counters() {
    let entropy = FakeEntropy::new(61);
    let mut parent = Arc4Random::new();
    let mut out = [0u8; 32];
    parent.random_buf(&entropy, &Fatal, &mut out);
    assert!(parent.count() > 0);
    let child = fake_fork(&mut parent);
    assert_eq!(child.count(), 0);
}
