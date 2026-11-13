// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for [`crate::traits`] and the fake environment's trait implementation.
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
use crate::fake_ut::{FakeEnv, Kind};

#[test]
fn stat_lite_is_plain_data() {
    let stat = StatLite {
        mode: 0o100644,
        blksize: 4096,
        is_char_dev: false,
    };
    assert_eq!(stat.mode, 0o100644);
    assert_eq!(stat.blksize, 4096);
    assert!(!stat.is_char_dev);
    let copy = stat;
    assert_eq!(copy, stat);
}

#[test]
fn fake_env_implements_the_seam() {
    let env = FakeEnv::new();
    env.add_file(b"/a", b"hello", 512);
    let fd = env.open(b"/a", 0, 0).expect("open");
    let mut buf = [0u8; 2];
    assert_eq!(env.read(fd, &mut buf), Ok(2));
    assert_eq!(&buf, b"he");
    assert!(!env.isatty(fd));
    assert_eq!(env.seek(fd, 0, 0), Ok(0));
    assert_eq!(env.write(fd, b"XY"), Ok(2));
    assert_eq!(env.close(fd), Ok(()));
    assert_eq!(env.contents(b"/a"), b"XYllo".to_vec());
}

#[test]
fn fake_kinds_report_the_right_stat() {
    let env = FakeEnv::new();
    env.install_std_tty();
    let tty = env.fstat(1).expect("fstat tty");
    assert!(tty.is_char_dev);
    let pipe = env.push_fd(b"", Kind::Pipe);
    let stat = env.fstat(pipe).expect("fstat pipe");
    assert!(!stat.is_char_dev);
    assert_eq!(env.seek(pipe, 0, 0).map_err(|e| e.0), Err(29));
}

#[test]
fn fake_pipe_has_no_seek() {
    let env = FakeEnv::new();
    let (read, write) = env.pipe2(0).expect("pipe2");
    assert_eq!(env.seek(read, 0, 1).map_err(|e| e.0), Err(29));
    assert_eq!(env.write(write, b"x"), Ok(1));
    assert_eq!(env.close(read), Ok(()));
    assert_eq!(env.close(write), Ok(()));
}
