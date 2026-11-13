// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OpenBSD `regress/lib/libc/stdio/test___f*.c` expectations.
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

use raven_libc_stdio::ext::{
    __fbufsize, __flbf, __fpending, __fpurge, __freadable, __freadahead, __freading, __freadptr,
    __freadptrinc, __fseterr, __fsetlocking, __fwritable, __fwriting, FSETLOCKING_BYCALLER,
    FSETLOCKING_INTERNAL, FSETLOCKING_QUERY,
};
use raven_libc_stdio::{Fail, fclose, fflush, fgetc, fwrite};

#[test]
fn openbsd_fbufsize_and_flbf() {
    let (state, env) = support::state();
    env.add(b"/f", b"abc");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(__fbufsize(file), 0);
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    assert!(__fbufsize(file) >= 512);
    assert!(!__flbf(file));
    let _ = fclose(file);
}

#[test]
fn openbsd_fpending_and_fwriting() {
    let (state, env) = support::state();
    env.add(b"/f", b"");
    let file = support::open(state, b"/f", b"w");
    assert!(__fwritable(file));
    assert!(!__freadable(file));
    assert_eq!(fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(__fpending(file), 3);
    assert!(__fwriting(file));
    assert!(!__freading(file));
    let _ = fflush(state, Some(file));
    assert_eq!(__fpending(file), 0);
    let _ = fclose(file);
}

#[test]
fn openbsd_freadahead_and_freadptr() {
    let (state, env) = support::state();
    env.add(b"/f", b"abcdef");
    let file = support::open(state, b"/f", b"r");
    assert!(__freading(file));
    assert_eq!(__freadahead(file), 0);
    assert_eq!(__freadptr(file), None);
    assert_eq!(fgetc(file), Ok(i32::from(b'a')));
    let before = __freadahead(file);
    assert!(before > 0);
    let (offset, len) = __freadptr(file).expect("ptr");
    assert!(offset >= 8);
    assert!(len > 0);
    __freadptrinc(file, 1);
    assert_eq!(__freadahead(file), before - 1);
    let _ = fclose(file);
}

#[test]
fn openbsd_fsetlocking_states() {
    let (state, env) = support::state();
    env.add(b"/f", b"");
    let file = support::open(state, b"/f", b"w");
    assert_eq!(__fsetlocking(file, FSETLOCKING_QUERY), FSETLOCKING_INTERNAL);
    assert_eq!(
        __fsetlocking(file, FSETLOCKING_BYCALLER),
        FSETLOCKING_INTERNAL
    );
    assert_eq!(__fsetlocking(file, FSETLOCKING_QUERY), FSETLOCKING_BYCALLER);
    assert_eq!(
        __fsetlocking(file, FSETLOCKING_INTERNAL),
        FSETLOCKING_BYCALLER
    );
    assert_eq!(__fsetlocking(file, 99), FSETLOCKING_INTERNAL);
    let _ = fclose(file);
}

#[test]
fn openbsd_fpurge_drops_the_buffer() {
    let (state, env) = support::state();
    env.add(b"/f", b"");
    let file = support::open(state, b"/f", b"w");
    assert_eq!(fwrite(file, b"abc", 1, 3), Ok(3));
    assert_eq!(__fpurge(file), Ok(()));
    assert_eq!(__fpending(file), 0);
    let _ = fflush(state, Some(file));
    assert!(env.bytes(b"/f").is_empty());
    let _ = fclose(file);
}

#[test]
fn openbsd_fseterr_sets_the_indicator() {
    let (state, env) = support::state();
    env.add(b"/f", b"");
    let file = support::open(state, b"/f", b"w");
    assert!(!raven_libc_stdio::ferror(file));
    __fseterr(file);
    assert!(raven_libc_stdio::ferror(file));
    raven_libc_stdio::clearerr(file);
    assert!(!raven_libc_stdio::ferror(file));
    let _ = fclose(file);
}

#[test]
fn openbsd_status_functions_are_consistent_after_a_read_error() {
    let (state, env) = support::state();
    env.add(b"/f", b"x");
    let file = support::open(state, b"/f", b"r");
    assert_eq!(fgetc(file), Ok(i32::from(b'x')));
    assert_eq!(fgetc(file), Err(Fail::Eof));
    assert!(raven_libc_stdio::feof(file));
    assert!(!raven_libc_stdio::ferror(file));
    let _ = fclose(file);
}
