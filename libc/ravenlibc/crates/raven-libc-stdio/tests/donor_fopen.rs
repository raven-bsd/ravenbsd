// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD `t_fopen.c` and the FreeBSD `fopen_test.c` / `fdopen_test.c` / `freopen_test.c`
//! expectations.
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

use raven_libc_stdio::{Fail, SysFile, fclose, fflush, fgetc, freopen, fseek, fwrite};

#[test]
fn netbsd_fopen_read_missing_is_enoent() {
    let (state, _env) = support::state();
    assert_eq!(
        raven_libc_stdio::fopen(state, b"/missing", b"r").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::ENOENT))
    );
}

#[test]
fn netbsd_fopen_w_creates_and_truncates() {
    let (state, env) = support::state();
    env.add(b"/f", b"old contents");
    let file = support::open(state, b"/f", b"w");
    assert_eq!(fwrite(file, b"new", 1, 3), Ok(3));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(env.bytes(b"/f"), b"new".to_vec());
    let _ = fclose(file);
}

#[test]
fn netbsd_fopen_a_appends() {
    let (state, env) = support::state();
    env.add(b"/f", b"base");
    let file = support::open(state, b"/f", b"a");
    assert_eq!(fwrite(file, b"+", 1, 1), Ok(1));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(env.bytes(b"/f"), b"base+".to_vec());
    let _ = fclose(file);
}

#[test]
fn netbsd_fopen_r_plus_reads_and_writes() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r+");
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert_eq!(fseek(file, 1, 0), Ok(()));
    assert_eq!(fwrite(file, b"Z", 1, 1), Ok(1));
    assert_eq!(fflush(state, Some(file)), Ok(()));
    assert_eq!(env.bytes(b"/f"), b"aZcdef".to_vec());
    let _ = fclose(file);
}

#[test]
fn netbsd_fopen_mode_first_character_is_required() {
    let (state, _env) = support::state();
    assert_eq!(
        raven_libc_stdio::fopen(state, b"/f", b"q").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn freebsd_fdopen_rejects_an_incompatible_mode() {
    let (state, env) = support::state();
    let fd = env.push(b"", false, false);
    env.setfl(fd, 1).expect("setfl");
    assert_eq!(
        raven_libc_stdio::fdopen(state, fd, b"r").map(|_| ()),
        Err(Fail::from_int(raven_libc_abi::generated::errno::EINVAL))
    );
}

#[test]
fn freebsd_freopen_replaces_the_descriptor() {
    let (state, env) = support::state();
    env.add(b"/a", b"aaa");
    env.add(b"/b", b"bbb");
    let file = support::open(state, b"/a", b"r");
    assert_eq!(freopen(state, Some(b"/b"), b"r", file).map(|_| ()), Ok(()));
    assert_eq!(fgetc(file), Ok(i32::from(b'b')));
    let _ = fclose(file);
}

#[test]
fn freebsd_fopen_binary_mode_is_a_noop() {
    let (state, env) = support::state();
    env.add(b"/f", b"x");
    for mode in [b"rb".as_slice(), b"r+b", b"rb+"] {
        let file = support::open(state, b"/f", mode);
        assert_eq!(fgetc(file), Ok(i32::from(b'x')));
        let _ = fclose(file);
    }
}
