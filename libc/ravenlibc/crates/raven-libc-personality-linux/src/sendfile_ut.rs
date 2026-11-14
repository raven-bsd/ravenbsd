// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `sendfile`/`copy_file_range` bounce loop (ticket C66 5.H family `sendfile`).

use super::*;
use crate::testutil::{FakeFile, FakeFiles};

fn files_with(data: &[u8]) -> (FakeFiles, i32, i32) {
    let mut files = FakeFiles::new();
    files.insert(4, data);
    files.insert(5, b"");
    (files, 4, 5)
}

#[test]
fn empty_file_copies_nothing() {
    let (mut files, input, output) = files_with(b"");
    assert_eq!(sendfile(&mut files, output, input, None, 10), Ok(0));
}

#[test]
fn one_byte() {
    let (mut files, input, output) = files_with(b"x");
    assert_eq!(sendfile(&mut files, output, input, None, 10), Ok(1));
    assert_eq!(files.data(output), Some(&b"x"[..]));
}

#[test]
fn sixty_five_thousand_five_hundred_thirty_five_bytes() {
    let data = crate::testutil::repeated(b'a', 65_535);
    let (mut files, input, output) = files_with(&data);
    assert_eq!(
        sendfile(&mut files, output, input, None, 65_535),
        Ok(65_535)
    );
    assert_eq!(files.data(output), Some(data.as_slice()));
}

#[test]
fn exact_bounce_boundary() {
    let data = crate::testutil::repeated(b'b', BOUNCE);
    let (mut files, input, output) = files_with(&data);
    assert_eq!(
        sendfile(&mut files, output, input, None, BOUNCE),
        Ok(BOUNCE)
    );
    assert_eq!(files.data(output).map(<[u8]>::len), Some(BOUNCE));
}

#[test]
fn one_byte_over_the_bounce_boundary() {
    let data = crate::testutil::repeated(b'c', BOUNCE + 1);
    let (mut files, input, output) = files_with(&data);
    assert_eq!(
        sendfile(&mut files, output, input, None, BOUNCE + 1),
        Ok(BOUNCE + 1)
    );
    assert_eq!(files.data(output).map(<[u8]>::len), Some(BOUNCE + 1));
}

#[test]
fn three_bounce_buffers() {
    let len = BOUNCE * 3;
    let data = crate::testutil::repeated(b'd', len);
    let (mut files, input, output) = files_with(&data);
    assert_eq!(sendfile(&mut files, output, input, None, len), Ok(len));
    assert_eq!(files.data(output).map(<[u8]>::len), Some(len));
}

#[test]
fn count_is_clamped_to_the_linux_limit() {
    let data = crate::testutil::repeated(b'e', 16);
    let (mut files, input, output) = files_with(&data);
    assert_eq!(
        sendfile(&mut files, output, input, None, usize::MAX),
        Ok(16)
    );
    assert_eq!(SEND_COUNT_MAX, 0x7fff_f000);
}

#[test]
fn offset_form_leaves_the_descriptor_position() {
    let data = crate::testutil::repeated(b'f', 10);
    let mut files = FakeFiles::new();
    files.insert(4, &data);
    files.insert(5, b"");
    let mut offset = 3i64;
    assert_eq!(sendfile(&mut files, 5, 4, Some(&mut offset), 4), Ok(4));
    assert_eq!(offset, 7);
    assert_eq!(files.position(4), Some(0));
    assert_eq!(files.data(5), Some(&data[3..7]));
}

#[test]
fn offset_past_end_copies_nothing() {
    let (mut files, input, output) = files_with(b"abc");
    let mut offset = 10i64;
    assert_eq!(
        sendfile(&mut files, output, input, Some(&mut offset), 4),
        Ok(0)
    );
    assert_eq!(offset, 10);
}

#[test]
fn offset_on_a_non_seekable_input_is_espipe() {
    let mut files = FakeFiles::new();
    files.insert_file(
        4,
        FakeFile {
            data: b"abc".to_vec(),
            pos: 0,
            directory: false,
            read_only: false,
            seekable: false,
        },
    );
    files.insert(5, b"");
    let mut offset = 0i64;
    assert_eq!(
        sendfile(&mut files, 5, 4, Some(&mut offset), 3),
        Err(Errno::ESPIPE)
    );
}

#[test]
fn negative_offset_is_einval() {
    let (mut files, input, output) = files_with(b"abc");
    let mut offset = -1i64;
    assert_eq!(
        sendfile(&mut files, output, input, Some(&mut offset), 3),
        Err(Errno::EINVAL)
    );
}

#[test]
fn directory_input_is_eisdir() {
    let mut files = FakeFiles::new();
    files.insert_file(
        4,
        FakeFile {
            data: Vec::new(),
            pos: 0,
            directory: true,
            read_only: false,
            seekable: true,
        },
    );
    files.insert(5, b"");
    assert_eq!(sendfile(&mut files, 5, 4, None, 3), Err(Errno::EISDIR));
}

#[test]
fn read_only_output_is_ebadf() {
    let mut files = FakeFiles::new();
    files.insert(4, b"abc");
    files.insert_file(
        5,
        FakeFile {
            data: Vec::new(),
            pos: 0,
            directory: false,
            read_only: true,
            seekable: true,
        },
    );
    assert_eq!(sendfile(&mut files, 5, 4, None, 3), Err(Errno::EBADF));
}

#[test]
fn write_error_after_partial_returns_the_count() {
    let data = crate::testutil::repeated(b'g', BOUNCE + 10);
    let (mut files, input, output) = files_with(&data);
    files.write_fail_after = Some(1);
    assert_eq!(
        sendfile(&mut files, output, input, None, BOUNCE + 10),
        Ok(BOUNCE)
    );
    assert_eq!(files.data(output).map(<[u8]>::len), Some(BOUNCE));
}

#[test]
fn write_error_at_the_start_is_the_error() {
    let (mut files, input, output) = files_with(b"abc");
    files.write_error = Some(Errno::EIO);
    assert_eq!(
        sendfile(&mut files, output, input, None, 3),
        Err(Errno::EIO)
    );
}

#[test]
fn eagain_at_the_start_is_eagain() {
    let (mut files, input, output) = files_with(b"abc");
    files.write_eagain = true;
    assert_eq!(
        sendfile(&mut files, output, input, None, 3),
        Err(Errno::EAGAIN)
    );
}

#[test]
fn short_read_ends_the_copy() {
    let (mut files, input, output) = files_with(b"abc");
    assert_eq!(sendfile(&mut files, output, input, None, 100), Ok(3));
}

#[test]
fn copy_file_range_flags_nonzero_is_einval() {
    let (mut files, input, output) = files_with(b"abc");
    assert_eq!(
        copy_file_range(&mut files, input, None, output, None, 3, 1),
        Err(Errno::EINVAL)
    );
}

#[test]
fn copy_file_range_zero_length_is_zero() {
    let (mut files, input, output) = files_with(b"abc");
    assert_eq!(
        copy_file_range(&mut files, input, None, output, None, 0, 0),
        Ok(0)
    );
}

#[test]
fn copy_file_range_same_fd_is_einval() {
    let (mut files, input, _) = files_with(b"abc");
    assert_eq!(
        copy_file_range(&mut files, input, None, input, None, 3, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn copy_file_range_basic_copy() {
    let (mut files, input, output) = files_with(b"hello");
    assert_eq!(
        copy_file_range(&mut files, input, None, output, None, 5, 0),
        Ok(5)
    );
    assert_eq!(files.data(output), Some(&b"hello"[..]));
}

#[test]
fn copy_file_range_offsets_advance_without_moving_positions() {
    let (mut files, input, output) = files_with(b"0123456789");
    let mut off_in = 2i64;
    let mut off_out = 0i64;
    assert_eq!(
        copy_file_range(
            &mut files,
            input,
            Some(&mut off_in),
            output,
            Some(&mut off_out),
            4,
            0
        ),
        Ok(4)
    );
    assert_eq!(off_in, 6);
    assert_eq!(off_out, 4);
    assert_eq!(files.position(input), Some(0));
    assert_eq!(files.data(output), Some(&b"2345"[..]));
}

#[test]
fn copy_file_range_directory_is_eisdir() {
    let mut files = FakeFiles::new();
    files.insert_file(
        4,
        FakeFile {
            data: Vec::new(),
            pos: 0,
            directory: true,
            read_only: false,
            seekable: true,
        },
    );
    files.insert(5, b"");
    assert_eq!(
        copy_file_range(&mut files, 4, None, 5, None, 1, 0),
        Err(Errno::EISDIR)
    );
}

#[test]
fn copy_file_range_negative_offset_is_einval() {
    let (mut files, input, output) = files_with(b"abc");
    let mut off = -1i64;
    assert_eq!(
        copy_file_range(&mut files, input, Some(&mut off), output, None, 3, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn overlap_helper() {
    assert!(ranges_overlap(0, 5, 10));
    assert!(!ranges_overlap(0, 10, 5));
    assert!(!ranges_overlap(0, 20, 5));
    assert!(ranges_overlap(0, 0, 1));
}
