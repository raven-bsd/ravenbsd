// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::glue_io`]: the buffered and unbuffered sinks (A-stdio-12) and the
//! scan source.
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
use crate::fake_ut::{Op, leaked_state, open_file};
use raven_libc_fmt::Sink;

#[test]
fn buffered_sink_writes_through_fwritex() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut sink = FileSink::new(file);
    assert!(sink.write(b"hello ").is_ok());
    assert!(sink.write(b"world").is_ok());
    assert!(sink.finish().is_ok());
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), b"hello world".to_vec());
}

#[test]
fn unbuffered_sink_collects_and_writes_once() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    state.env().clear_trace();
    let mut sink = FileSink::new(file);
    assert!(sink.write(b"one ").is_ok());
    assert!(sink.write(b"two").is_ok());
    assert!(state.env().trace().is_empty(), "no write before finish");
    assert!(sink.finish().is_ok());
    let trace = state.env().trace();
    assert_eq!(trace.len(), 1);
    assert!(matches!(trace[0], Op::Write2 { fd, len: 7, ret: 7 } if fd == file.fd()));
    assert_eq!(state.env().data(file.fd()), b"one two".to_vec());
}

#[test]
fn unbuffered_sink_flushes_when_the_buffer_is_full() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IONBF, 0).expect("setvbuf");
    state.env().clear_trace();
    let mut sink = FileSink::new(file);
    let chunk = std::vec![b'x'; UNBUF_PRINT_BUF + 1];
    assert!(sink.write(&chunk).is_ok());
    assert_eq!(state.env().trace().len(), 1);
    assert!(sink.write(b"tail").is_ok());
    assert!(sink.finish().is_ok());
    assert_eq!(state.env().trace().len(), 2);
}

#[test]
fn wide_sink_encodes_utf8() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"", 1024);
    let file = open_file(state, b"/f", b"w");
    let mut sink = FileSink::new(file);
    assert!(Sink::<u32>::write(&mut sink, &[u32::from(b'a'), 0x20ac]).is_ok());
    assert!(sink.finish().is_ok());
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(state.env().contents(b"/f"), "a€".as_bytes().to_vec());
}

#[test]
fn source_reads_and_pushes_back() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"abc", 1024);
    let file = open_file(state, b"/f", b"r");
    let mut source = FileSource::new(file);
    assert_eq!(ScanSource::<u8>::getc(&mut source), Some(b'a'));
    assert_eq!(source.consumed(), 1);
    ScanSource::<u8>::ungetc(&mut source, b'a');
    assert_eq!(source.consumed(), 0);
    assert_eq!(ScanSource::<u8>::getc(&mut source), Some(b'a'));
    assert_eq!(ScanSource::<u8>::getc(&mut source), Some(b'b'));
    assert_eq!(ScanSource::<u8>::getc(&mut source), Some(b'c'));
    assert_eq!(ScanSource::<u8>::getc(&mut source), None);
    assert_eq!(source.consumed(), 3);
    assert!(core::ptr::eq(source.file(), file));
}

#[test]
fn wide_source_reads_wide_units() {
    let state = leaked_state();
    state.env().add_file(b"/f", "é!".as_bytes(), 1024);
    let file = open_file(state, b"/f", b"r");
    let mut source = FileSource::new(file);
    assert_eq!(ScanSource::<u32>::getc(&mut source), Some(0xe9));
    assert_eq!(ScanSource::<u32>::getc(&mut source), Some(u32::from(b'!')));
    ScanSource::<u32>::ungetc(&mut source, 0xe9);
    assert_eq!(ScanSource::<u32>::getc(&mut source), Some(0xe9));
}
