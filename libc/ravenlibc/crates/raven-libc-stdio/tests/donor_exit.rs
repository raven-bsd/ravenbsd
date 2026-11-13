// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `donor-map/04` generator 9 (exit and flush behaviour) and layer G: the cleanup order,
//! flush-before-close, the reader reposition, and the fork hooks.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks,
    clippy::bool_assert_comparison
)]

mod support;

use raven_libc_stdio::{
    StdStreams, fclose, fflush, fgetc, fork_child, fork_parent, fork_prepare, fwrite, stdio_cleanup,
};

#[test]
fn cleanup_flushes_writers_and_does_not_close() {
    let (state, env) = support::state();
    env.install_std(true);
    let streams: &'static StdStreams<support::MemEnv> = Box::leak(Box::new(StdStreams::new(state)));
    env.add(b"/w", b"");
    let file = support::open(state, b"/w", b"w");
    assert_eq!(fwrite(file, b"abc", 1, 3), Ok(3));
    let fd = file.fd();
    stdio_cleanup(state, streams);
    assert_eq!(env.bytes(b"/w"), b"abc".to_vec());
    assert!(
        env.files.lock().unwrap().contains_key(&fd),
        "cleanup must not close"
    );
    assert!(!file.is_perm());
    let _ = fd;
}

#[test]
fn cleanup_repositions_a_reader() {
    let (state, env) = support::state();
    env.install_std(true);
    let streams: &'static StdStreams<support::MemEnv> = Box::leak(Box::new(StdStreams::new(state)));
    env.add(b"/r", b"abcdef");
    let file = support::open(state, b"/r", b"r");
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    let fd = file.fd();
    stdio_cleanup(state, streams);
    let files = env.files.lock().unwrap();
    assert_eq!(files.get(&fd).expect("fd").pos, 1);
}

#[test]
fn flush_none_writes_every_pending_writer() {
    let (state, env) = support::state();
    env.install_std(true);
    let streams: &'static StdStreams<support::MemEnv> = Box::leak(Box::new(StdStreams::new(state)));
    let stdout = streams.stdout();
    let _ = fwrite(stdout, b"prompt", 1, 6);
    env.add(b"/a", b"");
    env.add(b"/b", b"");
    let first = support::open(state, b"/a", b"w");
    let second = support::open(state, b"/b", b"w");
    let _ = fwrite(first, b"1", 1, 1);
    let _ = fwrite(second, b"2", 1, 1);
    assert_eq!(fflush(state, None), Ok(()));
    assert_eq!(env.bytes(b"/a"), b"1".to_vec());
    assert_eq!(env.bytes(b"/b"), b"2".to_vec());
    assert_eq!(env.data(1), b"prompt".to_vec());
}

#[test]
fn fork_hooks_leave_the_list_usable() {
    let (state, env) = support::state();
    env.install_std(true);
    let _streams: &'static StdStreams<support::MemEnv> =
        Box::leak(Box::new(StdStreams::new(state)));
    env.add(b"/w", b"");
    let file = support::open(state, b"/w", b"w");
    assert_eq!(fwrite(file, b"x", 1, 1), Ok(1));
    fork_prepare(state);
    fork_parent(state);
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(env.bytes(b"/w"), b"x".to_vec());
    fork_prepare(state);
    fork_child(state);
    assert_eq!(fflush(state, Some(file)), Ok(()));
    let _ = fclose(file);
}
