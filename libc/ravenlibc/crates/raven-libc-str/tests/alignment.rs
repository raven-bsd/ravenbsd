// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Alignment sweeps and the guard-page proof of A-str-1 (ticket 7).
//!
//! The host allocation is three pages: two readable/writable and one `PROT_NONE` guard. Strings
//! are placed so their last byte is the last byte before the guard, so a word loop that reads past
//! the terminator would fault instead of silently passing.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::needless_range_loop,
    clippy::manual_c_str_literals
)]

use core::ffi::c_void;

use raven_libc_str::raw;

unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64)
    -> *mut c_void;
    fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
}

const PROT_NONE: i32 = 0;
const PROT_READ: i32 = 1;
const PROT_WRITE: i32 = 2;
const MAP_PRIVATE: i32 = 2;
const MAP_ANONYMOUS: i32 = 0x20;
const PAGE: usize = 4096;

struct Guard {
    base: *mut u8,
}

impl Guard {
    fn new() -> Self {
        // SAFETY: the mmap/mprotect calls use the Linux ABI constants above; the returned mapping
        // is owned by this value for the duration of the test.
        unsafe {
            let base = mmap(
                core::ptr::null_mut(),
                3 * PAGE,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            );
            assert_ne!(base as isize, -1, "mmap failed");
            let base = base.cast::<u8>();
            assert_eq!(
                mprotect(base.add(2 * PAGE).cast::<c_void>(), PAGE, PROT_NONE),
                0,
                "mprotect failed"
            );
            Self { base }
        }
    }

    /// First byte of the guard page.
    fn guard(&self) -> *mut u8 {
        // SAFETY: the mapping is three pages; the third page starts at 2 * PAGE.
        unsafe { self.base.add(2 * PAGE) }
    }

    /// Writable pointer such that the next `bytes` bytes end exactly at the guard.
    fn tail(&self, bytes: usize) -> *mut u8 {
        // SAFETY: `bytes <= 2 * PAGE`, so the result stays in the writable part.
        unsafe { self.guard().sub(bytes) }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        // The test process is short-lived; leaking three pages per test is bounded and simpler
        // than declaring munmap only for this purpose.
        let _ = self.base;
    }
}

#[test]
fn strlen_stops_at_the_guard_page() {
    let guard = Guard::new();
    for length in 0usize..=255 {
        let start = guard.tail(length + 1);
        // SAFETY: `length + 1` writable bytes end exactly at the guard; the NUL is the last one.
        unsafe {
            for index in 0..length {
                *start.add(index) = b'a';
            }
            *start.add(length) = 0;
            assert_eq!(raw::strlen(start), length);
        }
    }
}

#[test]
fn strnlen_never_reads_past_the_nul() {
    let guard = Guard::new();
    for length in 0usize..=200 {
        let start = guard.tail(length + 1);
        // SAFETY: the string and its NUL are inside the last writable bytes before the guard.
        unsafe {
            for index in 0..length {
                *start.add(index) = b'x';
            }
            *start.add(length) = 0;
            assert_eq!(raw::strnlen(start, 4096), length, "length {length}");
        }
    }
}

#[test]
fn scanning_functions_stop_at_the_nul_before_the_guard() {
    let guard = Guard::new();
    for length in 0usize..=128 {
        let start = guard.tail(length + 1);
        // SAFETY: the string and its NUL are inside the last writable bytes before the guard.
        unsafe {
            for index in 0..length {
                *start.add(index) = b'a' + (index % 26) as u8;
            }
            *start.add(length) = 0;
            assert_eq!(raw::strchr(start, i32::from(b'#')), core::ptr::null_mut());
            assert_eq!(*raw::strchrnul(start, i32::from(b'#')), 0);
            assert_eq!(raw::strrchr(start, i32::from(b'#')), core::ptr::null_mut());
            assert_eq!(
                raw::memchr(start, i32::from(b'#'), length),
                core::ptr::null_mut()
            );
            assert_eq!(
                raw::memrchr(start, i32::from(b'#'), length),
                core::ptr::null_mut()
            );
            assert_eq!(
                raw::strstr(start, b"not-there\0".as_ptr()),
                core::ptr::null_mut()
            );
            let mut destination = [0u8; 256];
            let returned = raw::stpcpy(destination.as_mut_ptr(), start);
            assert_eq!(*returned, 0);
            assert_eq!(returned as usize - destination.as_ptr() as usize, length);
        }
    }
}

#[test]
fn copy_functions_write_up_to_but_not_past_the_guard() {
    let guard = Guard::new();
    for length in 0usize..=128 {
        let destination = guard.tail(length);
        let mut source = vec![0x5Au8; length];
        for (index, slot) in source.iter_mut().enumerate() {
            *slot = index as u8;
        }
        // SAFETY: `length` writable bytes end at the guard; the source has exactly `length`
        // readable bytes.
        unsafe {
            raw::memcpy(destination, source.as_ptr(), length);
            for index in 0..length {
                assert_eq!(*destination.add(index), source[index]);
            }
            raw::memset(destination, 0xA5, length);
            for index in 0..length {
                assert_eq!(*destination.add(index), 0xA5);
            }
        }
    }
}

#[test]
fn memmove_overlaps_both_directions() {
    for distance in [1usize, 2, 7, 8, 15, 16, 17, 63, 64, 65, 129] {
        for length in 0usize..=300 {
            let mut buffer = vec![0u8; length + distance + 8];
            for (index, slot) in buffer.iter_mut().enumerate() {
                *slot = (index % 251) as u8;
            }
            let mut expected = buffer.clone();
            // SAFETY: both ranges are inside `buffer`; overlap is exactly what memmove allows.
            unsafe {
                raw::memmove(buffer.as_mut_ptr().add(distance), buffer.as_ptr(), length);
            }
            expected.copy_within(0..length, distance);
            assert_eq!(
                buffer, expected,
                "distance {distance} length {length} forward"
            );
            let mut buffer = vec![0u8; length + distance + 8];
            for (index, slot) in buffer.iter_mut().enumerate() {
                *slot = (index % 251) as u8;
            }
            let mut expected = buffer.clone();
            // SAFETY: as above, in the other direction.
            unsafe {
                raw::memmove(buffer.as_mut_ptr(), buffer.as_ptr().add(distance), length);
            }
            expected.copy_within(distance..distance + length, 0);
            assert_eq!(
                buffer, expected,
                "distance {distance} length {length} backward"
            );
        }
    }
}

#[test]
fn alignment_sweep_for_block_and_string_operations() {
    let mut source_storage = [0u8; 512];
    let mut destination_storage = [0u8; 512];
    for (index, slot) in source_storage.iter_mut().enumerate() {
        *slot = (index % 241) as u8;
    }
    for source_offset in 0usize..16 {
        for destination_offset in 0usize..16 {
            for length in 0usize..=64 {
                // SAFETY: all offsets plus 64 stay inside the 512-byte arrays.
                unsafe {
                    let source = source_storage.as_ptr().add(source_offset);
                    let destination = destination_storage.as_mut_ptr().add(destination_offset);
                    raw::memcpy(destination, source, length);
                    for index in 0..length {
                        assert_eq!(*destination.add(index), *source.add(index));
                    }
                    assert_eq!(
                        raw::memcmp(destination, source, length),
                        0,
                        "offset {source_offset}/{destination_offset} length {length}"
                    );
                    raw::memset(destination, 0xEE, length);
                    if length != 0 {
                        assert_eq!(
                            raw::rawmemchr(destination, 0xEE) as usize - destination as usize,
                            0
                        );
                    }
                    let mut string = [0u8; 65];
                    for index in 0..length {
                        string[index] = b'a' + (index % 26) as u8;
                    }
                    string[length] = 0;
                    assert_eq!(raw::strlen(string.as_ptr()), length);
                    assert_eq!(raw::strnlen(string.as_ptr(), length), length);
                    if length != 0 {
                        assert_eq!(*raw::strchr(string.as_ptr(), i32::from(b'a')), b'a');
                    } else {
                        assert_eq!(
                            raw::strchr(string.as_ptr(), i32::from(b'a')),
                            core::ptr::null_mut()
                        );
                    }
                }
            }
        }
    }
}
