// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::file`]: flag values, the index state machine and invariants.
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
use crate::fake_ut::leaked_state;

#[test]
fn flag_values_are_the_donor_values() {
    assert_eq!(UNGET, 8);
    assert_eq!(F_PERM, 1);
    assert_eq!(F_NORD, 4);
    assert_eq!(F_NOWR, 8);
    assert_eq!(F_EOF, 16);
    assert_eq!(F_ERR, 32);
    assert_eq!(F_SVB, 64);
    assert_eq!(F_APP, 128);
    assert_eq!(F_BYCALLER, 256);
    assert_eq!(F_LAZYBUF, 512);
    assert_eq!(MAYBE_WAITERS, 0x4000_0000);
}

#[test]
fn empty_state_is_closed() {
    let state = leaked_state();
    let file = File::new(state);
    let st = file.st();
    assert_eq!(st.flags, 0);
    assert_eq!(st.fd, -1);
    assert_eq!(st.buf_size, 0);
    assert_eq!(st.lbf, -1);
    assert_eq!(st.mode, 0);
    assert_eq!(st.lock.load(core::sync::atomic::Ordering::Relaxed), 0);
    assert!(matches!(st.backend, crate::backend::Backend::Closed));
    assert!(st.getln_buf.is_none());
    file.check_invariants();
}

#[test]
fn orientation_only_moves_from_zero() {
    let state = leaked_state();
    let file = File::new(state);
    file.st_mut().orient_byte();
    assert_eq!(file.st().mode, -1);
    file.st_mut().orient_byte();
    assert_eq!(file.st().mode, -1);
    file.st_mut().mode = 1;
    file.st_mut().orient_byte();
    assert_eq!(file.st().mode, 1);
    file.st_mut().mode = 0;
    file.st_mut().mbmode = raven_libc_ctype::MbMode::Byte;
    assert_eq!(file.st().mbmode.mb_cur_max(), 1);
    assert_eq!(raven_libc_ctype::MbMode::Utf8.mb_cur_max(), 4);
}

#[test]
fn unread_window_is_signed() {
    let state = leaked_state();
    let file = File::new(state);
    {
        let st = file.st_mut();
        st.rpos = 10;
        st.rend = 4;
    }
    assert_eq!(file.st().unread(), -6);
    {
        let st = file.st_mut();
        st.rpos = 2;
        st.rend = 9;
    }
    assert_eq!(file.st().unread(), 7);
}

#[test]
fn pending_write_and_clear() {
    let state = leaked_state();
    let file = File::new(state);
    {
        let st = file.st_mut();
        st.wbase = UNGET;
        st.wpos = UNGET;
        assert!(!st.has_pending_write());
        st.wpos = UNGET + 3;
        assert!(st.has_pending_write());
        st.clear_write();
        assert!(!st.has_pending_write());
        st.wend = UNGET + 10;
        st.wpos = UNGET + 2;
        st.wbase = UNGET + 1;
        st.clear_write();
        assert_eq!(st.wend, 0);
        st.rpos = 3;
        st.rend = 4;
        st.clear_read();
        assert_eq!(st.rpos, 0);
        assert_eq!(st.rend, 0);
    }
}

#[test]
fn accessors_reflect_state() {
    let state = leaked_state();
    let file = File::new(state);
    assert_eq!(file.fd(), -1);
    assert!(!file.is_perm());
    assert_eq!(file.flags(), 0);
    assert!(core::ptr::eq(file.state(), state));
    file.st_mut().fd = 7;
    file.st_mut().flags = F_PERM;
    assert_eq!(file.fd(), 7);
    assert!(file.is_perm());
    assert_eq!(file.flags(), F_PERM);
}

#[test]
fn invariants_accept_a_consistent_read_state() {
    let state = leaked_state();
    let file = File::new(state);
    {
        let st = file.st_mut();
        st.backend = crate::backend::Backend::Fd;
        st.fd = 0;
        st.buf_size = 16;
        st.buffer = Buffer::Heap(alloc::vec![0; UNGET + 16]);
        st.rpos = UNGET + 2;
        st.rend = UNGET + 10;
    }
    file.check_invariants();
}
