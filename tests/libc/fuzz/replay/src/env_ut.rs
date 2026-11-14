// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::Cursor;

fn env_from(bytes: &[u8]) -> FuzzEnv {
    let mut cursor = Cursor::new(bytes);
    FuzzEnv::from_cursor(&mut cursor)
}

#[test]
fn the_arena_is_filled_from_the_input() {
    let mut bytes = [0u8; 16];
    bytes[0] = 0xaa;
    bytes[1] = 0xbb;
    let env = env_from(&bytes);
    assert_eq!(env.arena()[0], 0xaa);
    assert_eq!(env.arena()[1], 0xbb);
}

#[test]
fn cstr_len_stops_at_nul_and_at_the_arena_end() {
    let mut env = env_from(&[]);
    env.arena[0] = b'a';
    env.arena[1] = b'b';
    env.arena[2] = 0;
    assert_eq!(env.cstr_len(0, 100), 2);
    assert_eq!(env.cstr_len(0, 1), 1);
    assert_eq!(env.cstr_len(255, 100), 0);
}

#[test]
fn a_pointer_outside_the_arena_is_recorded() {
    let mut env = env_from(&[]);
    assert_eq!(env.cstr_len(300, 10), 0);
    assert_eq!(env.out_of_arena(), 0);
    assert_eq!(env.read_bytes(256, 0, &mut [0u8; 4]), 0);
    assert_eq!(env.out_of_arena(), 1);
}

#[test]
fn read_bytes_returns_what_exists() {
    let mut env = env_from(&[]);
    env.arena[4] = 1;
    env.arena[5] = 2;
    let mut buffer = [0u8; 4];
    assert_eq!(env.read_bytes(4, 0, &mut buffer), 4);
    assert_eq!(&buffer[..2], &[1, 2]);
    assert_eq!(buffer[2], 0);
}

#[test]
fn wide_strings_count_words() {
    let mut env = env_from(&[]);
    env.arena[0] = b'a';
    env.arena[4] = b'b';
    assert_eq!(env.wstr_len(0, 10), 2);
    let mut buffer = [0u32; 2];
    assert_eq!(env.read_wchars(0, 0, &mut buffer), 2);
    assert_eq!(buffer, [u32::from(b'a'), u32::from(b'b')]);
}

#[test]
fn the_store_log_is_capped() {
    let mut env = env_from(&[]);
    for _ in 0..(STORE_LOG_MAX + 5) {
        env.store_int(0, raven_libc_fmt::StoreSize::Int, 1);
    }
    assert_eq!(env.stores().len(), STORE_LOG_MAX);
    assert_eq!(env.dropped_stores(), 5);
}

#[test]
fn only_four_allocations_succeed() {
    let mut env = env_from(&[]);
    for _ in 0..6 {
        let _ = env.alloc_store(0, b"x\0", false);
    }
    assert_eq!(env.alloc_calls(), 6);
    let successes = env
        .stores()
        .iter()
        .filter(|store| matches!(store, Store::Alloc { ok: true, .. }))
        .count();
    assert_eq!(successes, ALLOC_SUCCESSES as usize);
}

#[test]
fn errno_text_is_eight_bytes() {
    let mut env = env_from(&[]);
    let mut buffer = [0u8; 64];
    assert_eq!(env.errno_text(0, &mut buffer), 8);
    assert_eq!(&buffer[..8], b"errno:00");
}

#[test]
fn argument_words_have_hostile_upper_halves() {
    let mut bytes = vec![0u8; 256 + 128];
    bytes[256] = 0x11;
    bytes[260] = 0x22;
    let mut env = env_from(&bytes);
    let word = env.next_u64();
    assert_eq!(word & 0xffff_ffff, 0x11);
    assert_eq!(word >> 32, 0x22);
}

#[test]
fn stores_record_their_kind_and_width() {
    let mut env = env_from(&[]);
    env.store_count(4, raven_libc_fmt::StoreSize::Hh, -1);
    env.store_chars(8, b"abc");
    env.store_wchars(9, &[1, 2]);
    env.store_float_bits(10, raven_libc_fmt::FloatKind::F64, 0);
    assert_eq!(env.stores().len(), 4);
    match env.stores().first() {
        Some(Store::Integer { ptr, size, value }) => {
            assert_eq!(*ptr, 4);
            assert_eq!(*size, 1);
            assert_eq!(*value, u64::MAX);
        }
        other => panic!("unexpected store {other:?}"),
    }
}

#[test]
fn wide_reads_advance_by_four_bytes() {
    let mut env = env_from(&[]);
    env.arena[4] = 7;
    let mut buffer = [0u32; 1];
    assert_eq!(env.read_wchars(0, 1, &mut buffer), 1);
    assert_eq!(buffer[0], 7);
}

#[test]
fn clone_state_copies_the_read_position() {
    let mut bytes = vec![0u8; 256 + 128];
    bytes[256] = 1;
    bytes[264] = 2;
    let mut env = env_from(&bytes);
    let _ = env.next_u64();
    let mut clone = env.clone_state();
    assert_eq!(clone.next_u64(), 2);
}
