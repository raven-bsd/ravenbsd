// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of `CEnv` (`env.rs`) over a byte arena addressed by raw pointers.

use super::*;
use crate::va::sysv_x86_64::{SysvReader, VaListSysV};

fn no_errno_text(_errnum: i32, _buf: &mut [u8; 64]) -> usize {
    0
}

fn no_alloc(_size: usize) -> *mut u8 {
    core::ptr::null_mut()
}

fn leaking_alloc(size: usize) -> *mut u8 {
    let mut buffer = vec![0u8; size];
    let pointer = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    pointer
}

/// An empty reader: these tests never read an argument.
fn empty_reader() -> SysvReader {
    SysvReader::from_list(VaListSysV {
        gp_offset: 48,
        fp_offset: 176,
        overflow_arg_area: core::ptr::null_mut(),
        reg_save_area: core::ptr::null_mut(),
    })
}

fn env(alloc: fn(usize) -> *mut u8) -> CEnv<SysvReader> {
    CEnv::new(empty_reader(), no_errno_text, alloc)
}

#[test]
fn cstr_len_stops_at_max_and_never_reads_past_it() {
    let mut unterminated = [0xCCu8; 32];
    unterminated[8..14].copy_from_slice(b"abcdef");
    // No NUL inside the whole arena: the walk must stop at `max`.
    let mut e = env(no_alloc);
    let ptr = unterminated.as_mut_ptr() as usize + 8;
    assert_eq!(e.cstr_len(ptr, 3), 3);
    assert_eq!(e.cstr_len(ptr, 0), 0);
    // A NUL ends the walk before the limit.
    let mut terminated = [0xCCu8; 32];
    terminated[8..12].copy_from_slice(b"abc\0");
    let ptr = terminated.as_mut_ptr() as usize + 8;
    assert_eq!(e.cstr_len(ptr, 6), 3);
    assert_eq!(e.cstr_len(0, 8), 0);
}

#[test]
fn read_bytes_copies_only_the_requested_window() {
    let mut arena = [0xCCu8; 16];
    arena[4..9].copy_from_slice(b"hello");
    let mut e = env(no_alloc);
    let mut buf = [0u8; 5];
    assert_eq!(e.read_bytes(arena.as_ptr() as usize, 4, &mut buf), 5);
    assert_eq!(&buf, b"hello");
    assert_eq!(e.read_bytes(0, 0, &mut buf), 0);
    assert_eq!(e.read_bytes(arena.as_ptr() as usize, 0, &mut []), 0);
}

#[test]
fn wide_strings_are_four_byte_units() {
    let mut arena = [0u8; 32];
    for (index, unit) in [0x61u32, 0x20AC, 0].iter().enumerate() {
        arena[index * 4..index * 4 + 4].copy_from_slice(&unit.to_le_bytes());
    }
    let mut e = env(no_alloc);
    let ptr = arena.as_mut_ptr() as usize;
    assert_eq!(e.wstr_len(ptr, 8), 2);
    assert_eq!(e.wstr_len(ptr, 1), 1);
    let mut buf = [0u32; 2];
    assert_eq!(e.read_wchars(ptr, 0, &mut buf), 2);
    assert_eq!(buf, [0x61, 0x20AC]);
    assert_eq!(e.read_wchars(ptr, 1, &mut buf[..1]), 1);
    assert_eq!(buf[0], 0x20AC);
}

#[test]
fn store_count_writes_each_size_without_touching_neighbours() {
    for (index, (size, bytes)) in [
        (StoreSize::Hh, 1usize),
        (StoreSize::H, 2),
        (StoreSize::Int, 4),
        (StoreSize::L, 8),
        (StoreSize::Ll, 8),
        (StoreSize::J, 8),
        (StoreSize::Z, 8),
        (StoreSize::T, 8),
    ]
    .into_iter()
    .enumerate()
    {
        let mut arena = [0xCCu8; 24];
        let ptr = arena.as_mut_ptr() as usize + 8;
        let mut e = env(no_alloc);
        e.store_count(ptr, size, -2);
        assert_eq!(
            &arena[..8],
            &[0xCC; 8],
            "row {index} wrote before the target"
        );
        assert_eq!(
            &arena[8 + bytes..],
            &[0xCC; 16][..16 - bytes],
            "row {index} wrote past the target"
        );
        let last = if matches!(size, StoreSize::Hh) {
            0xFE
        } else {
            0xFF
        };
        assert_eq!(arena[8 + bytes - 1], last, "row {index} did not truncate");
    }
    let mut e = env(no_alloc);
    e.store_count(0, StoreSize::Int, 1);
}

#[test]
fn store_int_truncates_to_the_named_width() {
    let mut arena = [0u8; 16];
    let ptr = arena.as_mut_ptr() as usize;
    let mut e = env(no_alloc);
    e.store_int(ptr, StoreSize::Hh, 0x1FF);
    assert_eq!(arena[0], 0xFF);
    e.store_int(ptr + 2, StoreSize::H, 0x1_2345);
    assert_eq!(&arena[2..4], &[0x45, 0x23]);
    e.store_int(ptr + 8, StoreSize::Ll, u64::MAX);
    assert_eq!(&arena[8..16], &[0xFF; 8]);
}

#[test]
fn float_bits_store_the_documented_widths() {
    let mut arena = [0u8; 32];
    let ptr = arena.as_mut_ptr() as usize;
    let mut e = env(no_alloc);
    e.store_float_bits(ptr, FloatKind::F64, 0x4004_0000_0000_0000);
    assert_eq!(
        u64::from_le_bytes(arena[0..8].try_into().unwrap()),
        0x4004_0000_0000_0000
    );
    e.store_float_bits(ptr + 8, FloatKind::LongDouble, 0x4000_c90f_daa2_2168_ffff);
    assert_eq!(
        u64::from_le_bytes(arena[8..16].try_into().unwrap()),
        0xc90f_daa2_2168_ffff
    );
    if cfg!(target_arch = "x86_64") {
        // The x87 image is 10 bytes (significand then sign+exponent); the six padding bytes
        // are zeroed.
        assert_eq!(&arena[16..18], &[0x00, 0x40]);
        assert_eq!(&arena[18..24], &[0u8; 6]);
    }
}

#[test]
fn alloc_store_reports_failure_and_sets_the_pointer() {
    let mut destination = core::ptr::null_mut::<u8>();
    let ptr = core::ptr::addr_of_mut!(destination) as usize;
    let mut failing = env(no_alloc);
    assert!(!failing.alloc_store(ptr, b"abc", false));
    assert!(destination.is_null());

    let mut ok = env(leaking_alloc);
    assert!(ok.alloc_store(ptr, b"abc", false));
    assert!(!destination.is_null());
    // SAFETY: the allocator returned a 4-byte block; `destination` points at it.
    unsafe {
        let mut copy = [0u8; 4];
        core::ptr::copy_nonoverlapping(destination, copy.as_mut_ptr(), 4);
        assert_eq!(&copy, b"abc\0");
    }
    assert!(!ok.alloc_store(0, b"abc", false));
}

#[test]
fn null_store_targets_are_no_ops() {
    let mut e = env(no_alloc);
    e.store_chars(0, b"abc");
    e.store_wchars(0, &[1, 2, 3]);
    e.store_float_bits(0, FloatKind::F64, 0);
    e.store_int(0, StoreSize::Int, 7);
    e.store_count(0, StoreSize::Int, 7);
}

#[test]
fn clone_state_copies_the_reader_position() {
    let list = VaListSysV {
        gp_offset: 16,
        fp_offset: 64,
        overflow_arg_area: core::ptr::null_mut(),
        reg_save_area: core::ptr::null_mut(),
    };
    let e = CEnv::new(SysvReader::from_list(list), no_errno_text, no_alloc);
    let copy = e.clone_state();
    assert_eq!(copy.reader().list().gp_offset, 16);
    assert_eq!(copy.reader().list().fp_offset, 64);
}
