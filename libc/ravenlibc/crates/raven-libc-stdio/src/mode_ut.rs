// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::mode`]: every read/write transition and the orientation rule.
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
use crate::fake_ut::{FakeEnv, Kind, leaked_state};

fn writable_file(state: &'static crate::state::State<FakeEnv>) -> &'static File<FakeEnv> {
    let file = &*std::boxed::Box::leak(std::boxed::Box::new(File::new(state)));
    let st = file.st_mut();
    st.fd = state.env().push_fd(b"", Kind::Regular(1024));
    st.buffer = crate::file::Buffer::Heap(alloc::vec![0; 8 + 16]);
    st.buf_size = 16;
    st.flags = crate::file::F_LAZYBUF;
    st.backend = crate::backend::Backend::Fd;
    file
}

#[test]
fn towrite_activates_the_write_window() {
    let state = leaked_state();
    let file = writable_file(state);
    towrite(file).expect("towrite");
    let st = file.st();
    assert_eq!(st.mode, -1);
    assert_eq!(st.wbase, crate::file::UNGET);
    assert_eq!(st.wpos, crate::file::UNGET);
    assert_eq!(st.wend, crate::file::UNGET + st.buf_size);
    assert_eq!(st.rpos, 0);
    assert_eq!(st.rend, 0);
}

#[test]
fn toread_flushes_and_arms_the_read_window() {
    let state = leaked_state();
    let file = writable_file(state);
    towrite(file).expect("towrite");
    {
        let st = file.st_mut();
        st.wpos = st.wbase + 2;
        st.buffer.bytes_mut()[crate::file::UNGET] = b'x';
        st.buffer.bytes_mut()[crate::file::UNGET + 1] = b'y';
    }
    let buffered = toread(file).expect("toread");
    assert!(!buffered);
    let st = file.st();
    assert_eq!(st.wpos, 0);
    assert_eq!(st.wbase, 0);
    assert_eq!(st.wend, 0);
    assert_eq!(st.rpos, crate::file::UNGET + st.buf_size);
    assert_eq!(st.rend, st.rpos);
    let trace = state.env().trace();
    assert!(
        trace
            .iter()
            .any(|op| matches!(op, crate::fake_ut::Op::Write2 { len: 2, .. }))
    );
}

#[test]
fn write_only_stream_rejects_reading() {
    let state = leaked_state();
    let file = writable_file(state);
    file.st_mut().flags |= crate::file::F_NORD;
    assert!(toread(file).is_err());
    assert_ne!(file.st().flags & crate::file::F_ERR, 0);
}

#[test]
fn read_only_stream_rejects_writing() {
    let state = leaked_state();
    let file = writable_file(state);
    file.st_mut().flags |= crate::file::F_NOWR;
    assert!(towrite(file).is_err());
    assert_ne!(file.st().flags & crate::file::F_ERR, 0);
}

#[test]
fn orientation_is_fixed_by_the_first_byte_operation() {
    let state = leaked_state();
    let file = writable_file(state);
    assert_eq!(file.st().mode, 0);
    towrite(file).expect("towrite");
    assert_eq!(file.st().mode, -1);
    file.st_mut().mode = 1;
    towrite(file).expect("towrite keeps wide");
    assert_eq!(file.st().mode, 1);
    assert!(require_byte(file).is_err());
    assert_ne!(file.st().flags & crate::file::F_ERR, 0);
    file.st_mut().mode = 0;
    assert!(require_byte(file).is_ok());
}

#[test]
fn force_byte_orientation_leaves_wide_alone() {
    let state = leaked_state();
    let file = writable_file(state);
    file.st_mut().mode = 1;
    force_byte_orientation(file);
    assert_eq!(file.st().mode, 1);
    file.st_mut().mode = 0;
    force_byte_orientation(file);
    assert_eq!(file.st().mode, -1);
}
