// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::misc`]: `remove`'s `EISDIR` retry, `rename` and `tmpfile`.
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
use crate::fake_ut::{FakeEnv, leaked_state};

#[test]
fn remove_unlinks_a_file() {
    let state = leaked_state();
    state.env().add_file(b"/f", b"x", 1024);
    assert_eq!(remove(state, b"/f"), Ok(()));
    assert_eq!(
        remove(state, b"/f"),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn remove_falls_back_to_rmdir_on_eisdir() {
    let state = leaked_state();
    state.env().add_dir(b"/d");
    assert_eq!(remove(state, b"/d"), Ok(()));
    assert_eq!(
        remove(state, b"/d"),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn rename_moves_the_contents() {
    let state = leaked_state();
    state.env().add_file(b"/a", b"data", 1024);
    assert_eq!(rename(state, b"/a", b"/b"), Ok(()));
    assert_eq!(state.env().contents(b"/b"), b"data".to_vec());
    assert!(state.env().contents(b"/a").is_empty());
    assert_eq!(
        rename(state, b"/missing", b"/c"),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn tmpfile_creates_an_anonymous_stream() {
    let state = leaked_state();
    let file = crate::raw::file_ref(tmpfile(state).expect("tmpfile"));
    assert!(!file.is_perm());
    assert_eq!(crate::block_io::fwrite(file, b"tmp", 1, 3), Ok(3));
    crate::flush::fflush(state, Some(file)).expect("fflush");
    assert_eq!(crate::seek::ftell(file), Ok(3));
    let pointer = (file as *const File<FakeEnv>).cast_mut();
    assert_eq!(crate::close::fclose(file), Ok(()));
    let _ = pointer;
}
