// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::buf`]: the lazy buffer sizes, the tty rule, the inverted stdout
//! probe (trap S11) and the flush-before-read trace.
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
use crate::fake_ut::{FakeEnv, Op, leaked_state, leaked_streams};
use crate::file::{F_LAZYBUF, F_SVB};

fn file_with(state: &'static crate::state::State<FakeEnv>, fd: i32) -> &'static File<FakeEnv> {
    let file = &*std::boxed::Box::leak(std::boxed::Box::new(File::new(state)));
    {
        let st = file.st_mut();
        st.fd = fd;
        st.flags = F_LAZYBUF;
        st.buffer = crate::file::Buffer::Heap(alloc::vec![0; 8]);
        st.buf_size = 0;
    }
    file
}

#[test]
fn blksize_is_clamped() {
    for (blksize, expected) in [(0u32, BUFSIZ), (100, 512), (4096, 4096), (1 << 20, 65536)] {
        let env = FakeEnv::new();
        let fd = env.push_fd(b"", crate::fake_ut::Kind::Regular(blksize));
        let state: &'static crate::state::State<FakeEnv> =
            std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
        let file = file_with(state, fd);
        crate::mode::towrite(file).expect("towrite");
        assert_eq!(file.st().buf_size, expected);
        assert_eq!(file.st().flags & F_LAZYBUF, 0);
        assert_eq!(file.st().wend, UNGET + expected);
    }
}

#[test]
fn failed_fstat_uses_bufsiz() {
    let env = FakeEnv::new();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = file_with(state, 99);
    crate::mode::toread(file).expect("toread");
    assert_eq!(file.st().buf_size, BUFSIZ);
}

#[test]
fn tty_at_allocation_sets_line_buffering() {
    let env = FakeEnv::new();
    env.install_std_tty();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = file_with(state, 1);
    crate::mode::towrite(file).expect("towrite");
    assert_eq!(file.st().lbf, i32::from(b'\n'));
}

#[test]
fn setvbuf_disables_the_policy() {
    let env = FakeEnv::new();
    env.install_std_tty();
    let state: &'static crate::state::State<FakeEnv> =
        std::boxed::Box::leak(std::boxed::Box::new(crate::state::State::new(env)));
    let file = file_with(state, 1);
    let mut buffer = std::vec![0u8; 64];
    crate::setvbuf::setvbuf(
        file,
        Some((buffer.as_mut_ptr(), buffer.len())),
        crate::setvbuf::IOFBF,
        64,
    )
    .expect("setvbuf");
    assert_ne!(file.st().flags & F_SVB, 0);
    crate::mode::towrite(file).expect("towrite");
    assert_eq!(file.st().buf_size, 64 - UNGET);
    assert_eq!(file.st().lbf, -1);
}

#[test]
fn stdout_probe_turns_line_buffering_off_on_a_file() {
    let state = leaked_state();
    let env = state.env();
    env.install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    assert_eq!(stdout.st().lbf, i32::from(b'\n'));
    assert!(stdout.st().probe);
    crate::char_io::putc(stdout, i32::from(b'x')).expect("putc");
    crate::flush::fflush(state, Some(stdout)).expect("fflush");
    assert_eq!(stdout.st().lbf, -1);
    assert!(!stdout.st().probe);
}

#[test]
fn stdout_probe_keeps_line_buffering_on_a_tty() {
    let state = leaked_state();
    let env = state.env();
    env.install_std_tty();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    crate::char_io::putc(stdout, i32::from(b'x')).expect("putc");
    crate::flush::fflush(state, Some(stdout)).expect("fflush");
    assert_eq!(stdout.st().lbf, i32::from(b'\n'));
}

#[test]
fn stdout_probe_is_skipped_after_setvbuf() {
    let state = leaked_state();
    let env = state.env();
    env.install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    let mut buffer = std::vec![0u8; 64];
    crate::setvbuf::setvbuf(
        stdout,
        Some((buffer.as_mut_ptr(), buffer.len())),
        crate::setvbuf::IONBF,
        64,
    )
    .expect("setvbuf");
    assert_ne!(stdout.st().flags & F_SVB, 0);
    assert!(stdout.st().probe);
    crate::char_io::putc(stdout, i32::from(b'x')).expect("putc");
    assert!(!stdout.st().probe);
    assert_eq!(stdout.st().lbf, -1);
}

#[test]
fn prompt_is_flushed_before_the_read() {
    let state = leaked_state();
    let env = state.env();
    env.install_std_tty();
    env.inner.lock().unwrap().files.get_mut(&0).unwrap().data = b"answer".to_vec();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    let stdin = streams.stdin();
    crate::line_io::fputs(stdout, b"p? ").expect("fputs");
    env.clear_trace();
    let first = crate::char_io::getc(stdin).expect("getc");
    assert_eq!(first, i32::from(b'a'));
    let trace = env.trace();
    let write_at = trace
        .iter()
        .position(|op| matches!(op, Op::Write { fd: 1, .. } | Op::Write2 { fd: 1, .. }));
    let read_at = trace
        .iter()
        .position(|op| matches!(op, Op::Read { fd: 0, .. }));
    assert!(
        write_at.is_some(),
        "stdout was not flushed before the read: {trace:?}"
    );
    assert!(read_at.is_some());
    assert!(write_at < read_at, "write must precede read: {trace:?}");
}
