// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the join word and stack layout.

use super::*;

#[test]
fn stack_layout_aligns_and_adds_a_guard() {
    assert_eq!(stack_layout(4096).unwrap(), (8192, 4096));
    assert_eq!(stack_layout(4097).unwrap(), (12288, 4096));
    assert_eq!(stack_layout(16 * 1024).unwrap(), (20 * 1024, 4096));
}

#[test]
fn stack_layout_rejects_tiny_stacks() {
    assert_eq!(stack_layout(0), Err(Errno::EINVAL));
    assert_eq!(stack_layout(PAGE - 1), Err(Errno::EINVAL));
}

#[test]
fn stack_layout_rejects_overflow() {
    assert_eq!(stack_layout(usize::MAX), Err(Errno::ENOMEM));
}

#[test]
fn done_word_starts_armed_and_the_kernel_clears_it() {
    let control = ThreadControl {
        function: |_| {},
        arg: 0,
        done: AtomicU32::new(1),
    };
    assert_eq!(control.done.load(Ordering::Acquire), 1);
    // THREAD_EXIT publishes zero from kernel mode; the joiner unblocks on this value.
    control.done.store(0, Ordering::Release);
    assert_eq!(control.done.load(Ordering::Acquire), 0);
}

#[test]
fn is_finished_follows_the_kernel_clear_word() {
    let control = Box::new(ThreadControl {
        function: |_| {},
        arg: 0,
        done: AtomicU32::new(1),
    });
    let thread = Thread {
        control: Some(control),
        handle: None,
        stack_base: 0,
        stack_total: 0,
    };
    assert!(!thread.is_finished());
    thread
        .control
        .as_ref()
        .unwrap()
        .done
        .store(0, Ordering::Release);
    assert!(thread.is_finished());
}
