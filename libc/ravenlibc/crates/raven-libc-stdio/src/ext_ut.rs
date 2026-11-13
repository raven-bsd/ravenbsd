// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::ext`], ported from the OpenBSD `test___f*.c` programs.
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
use crate::fake_ut::{FakeEnv, leaked_state, leaked_streams, open_file};
use crate::file::{F_BYCALLER, F_NORD, F_NOWR};

fn file(
    state: &'static crate::state::State<FakeEnv>,
    data: &[u8],
    mode: &[u8],
) -> &'static File<FakeEnv> {
    state.env().add_file(b"/f", data, 1024);
    open_file(state, b"/f", mode)
}

#[test]
fn fsetlocking_toggles_the_bycaller_flag() {
    let state = leaked_state();
    let file = file(state, b"", b"w");
    assert_eq!(__fsetlocking(file, FSETLOCKING_QUERY), FSETLOCKING_INTERNAL);
    assert_eq!(
        __fsetlocking(file, FSETLOCKING_BYCALLER),
        FSETLOCKING_INTERNAL
    );
    assert_ne!(file.flags() & F_BYCALLER, 0);
    assert_eq!(__fsetlocking(file, FSETLOCKING_QUERY), FSETLOCKING_BYCALLER);
    assert_eq!(
        __fsetlocking(file, FSETLOCKING_INTERNAL),
        FSETLOCKING_BYCALLER
    );
    assert_eq!(file.flags() & F_BYCALLER, 0);
    assert_eq!(__fsetlocking(file, 42), FSETLOCKING_INTERNAL);
    assert_eq!(file.flags() & F_BYCALLER, 0);
}

#[test]
fn fwriting_and_freading_follow_the_flags() {
    let state = leaked_state();
    let read = file(state, b"a", b"r");
    assert!(__freadable(read));
    assert!(!__fwritable(read));
    assert!(__freading(read));
    assert!(!__fwriting(read));
    let write = file(state, b"", b"w");
    assert!(!__freadable(write));
    assert!(__fwritable(write));
    assert!(__fwriting(write));
    assert!(!__freading(write));
    assert_ne!(read.flags() & F_NOWR, 0);
    assert_ne!(write.flags() & F_NORD, 0);
}

#[test]
fn buffer_queries() {
    let state = leaked_state();
    let read = file(state, b"abcdef", b"r");
    assert_eq!(__fbufsize(read), 0);
    assert_eq!(__freadahead(read), 0);
    assert_eq!(__freadptr(read), None);
    assert_eq!(crate::char_io::getc(read), Ok(i32::from(b'a')));
    assert!(__freadahead(read) > 0);
    let (offset, len) = __freadptr(read).expect("readptr");
    assert!(offset >= crate::file::UNGET);
    assert!(len > 0);
    let before = __freadahead(read);
    __freadptrinc(read, 2);
    assert_eq!(__freadahead(read), before - 2);
    assert!(!__flbf(read));
    let write = file(state, b"", b"w");
    assert_eq!(crate::block_io::fwrite(write, b"abc", 1, 3), Ok(3));
    assert_eq!(__fpending(write), 3);
    assert!(__fwriting(write));
}

#[test]
fn flbf_tracks_line_buffering() {
    let state = leaked_state();
    let file = file(state, b"", b"w");
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IOLBF, 0).expect("setvbuf");
    assert!(__flbf(file));
    crate::setvbuf::setvbuf(file, None, crate::setvbuf::IOFBF, 0).expect("setvbuf");
    assert!(!__flbf(file));
}

#[test]
fn fpurge_and_fseterr() {
    let state = leaked_state();
    let file = file(state, b"", b"w");
    assert_eq!(crate::block_io::fwrite(file, b"abc", 1, 3), Ok(3));
    assert!(__fpending(file) > 0);
    assert_eq!(__fpurge(file), Ok(()));
    assert_eq!(__fpending(file), 0);
    assert!(!__fseterr_checked(file));
    __fseterr(file);
    assert!(__fseterr_checked(file));
}

fn __fseterr_checked<E: Env>(file: &File<E>) -> bool {
    crate::status::ferror(file)
}

#[test]
fn flushlbf_flushes_only_line_buffered_writers() {
    let state = leaked_state();
    state.env().install_std_file();
    let streams = leaked_streams(state);
    let stdout = streams.stdout();
    assert_eq!(crate::block_io::fwrite(stdout, b"line", 1, 4), Ok(4));
    let file = file(state, b"", b"w");
    assert_eq!(crate::block_io::fwrite(file, b"full", 1, 4), Ok(4));
    assert_eq!(_flushlbf(state), Ok(()));
    assert_eq!(state.env().data(1), b"line".to_vec());
    assert!(state.env().contents(b"/f").is_empty());
    crate::flush::fflush(state, Some(file)).expect("fflush");
}
