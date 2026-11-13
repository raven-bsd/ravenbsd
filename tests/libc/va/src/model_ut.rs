// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Self-tests of the ABI model (`model.rs`): the model must follow the ABI text before it can
//! judge a reader.

use super::*;

fn ints(count: usize) -> Vec<Arg> {
    (0..count).map(|index| Arg::Int(index as u64 + 1)).collect()
}

#[test]
fn sysv_six_gp_then_stack() {
    let frame = assign(Arch::X86_64, 0, &ints(7));
    assert_eq!(&frame.gp[..6], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(frame.stack, 7u64.to_le_bytes());
}

#[test]
fn sysv_named_arguments_shift_the_gp_window() {
    let frame = assign(Arch::X86_64, 3, &ints(4));
    assert_eq!(&frame.gp[..6], &[0, 0, 0, 1, 2, 3]);
    assert_eq!(frame.stack, 4u64.to_le_bytes());
}

#[test]
fn sysv_eight_doubles_in_xmm_and_the_ninth_on_the_stack() {
    let args: Vec<Arg> = (1..=9).map(|value| Arg::Double(value as u64)).collect();
    let frame = assign(Arch::X86_64, 0, &args);
    assert_eq!(u64::from_le_bytes(frame.fp[0][..8].try_into().unwrap()), 1);
    assert_eq!(u64::from_le_bytes(frame.fp[7][..8].try_into().unwrap()), 8);
    assert_eq!(frame.stack, 9u64.to_le_bytes());
}

#[test]
fn softfloat_profiles_send_doubles_to_the_stack() {
    for arch in [Arch::X86_64Softfloat, Arch::Aapcs64Softfloat] {
        let frame = assign(arch, 0, &[Arg::Double(0x4004_0000_0000_0000)]);
        assert_eq!(
            frame.stack,
            0x4004_0000_0000_0000u64.to_le_bytes(),
            "{arch:?}"
        );
        assert_eq!(frame.fp[0], [0u8; 16], "{arch:?}");
    }
}

#[test]
fn sysv_long_double_is_16_aligned_on_the_stack() {
    let mut args = ints(6);
    args.push(Arg::Double(0x4000_0000_0000_0000));
    args.push(Arg::LongDouble(0x4000_1111_2222_3333_4444));
    args.push(Arg::Int(3));
    // The soft-float profile puts the `double` on the stack, where the alignment matters.
    let frame = assign(Arch::X86_64Softfloat, 0, &args);
    // 8 bytes of stack `double`, then 8 bytes of padding so the long double is 16-aligned.
    assert_eq!(frame.stack.len(), 40);
    assert_eq!(
        u64::from_le_bytes(frame.stack[0..8].try_into().unwrap()),
        0x4000_0000_0000_0000
    );
    assert_eq!(&frame.stack[8..16], &[PAD; 8]);
    assert_eq!(
        u128::from_le_bytes(frame.stack[16..32].try_into().unwrap()),
        0x4000_1111_2222_3333_4444
    );
    assert_eq!(frame.stack[32..40], 3u64.to_le_bytes());
}

#[test]
fn aapcs64_gr_and_vr_boundaries() {
    let mut args = ints(9);
    args.extend((1..=9).map(|value| Arg::Double(value as u64)));
    let frame = assign(Arch::Aapcs64, 0, &args);
    assert_eq!(frame.gp[7], 8);
    // The 9th integer and the 9th double are both on the stack, in call order.
    assert_eq!(frame.stack.len(), 16);
    assert_eq!(u64::from_le_bytes(frame.stack[0..8].try_into().unwrap()), 9);
    assert_eq!(
        u64::from_le_bytes(frame.stack[8..16].try_into().unwrap()),
        9
    );
}

#[test]
fn aapcs64_partial_register_fit_uses_the_stack_for_the_whole_aggregate() {
    let aggregate = Arg::Agg {
        bytes: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        align: 8,
    };
    let mut args = ints(7);
    args.push(aggregate);
    let frame = assign(Arch::Aapcs64, 0, &args);
    // Seven named... no: seven variadic integers occupy x0..x6; the 16-byte aggregate needs
    // two GR slots but only one remains, so it goes to the stack and NGRN is reset.
    assert_eq!(frame.gp[7], 0);
    assert_eq!(frame.stack.len(), 16);
    assert_eq!(frame.stack[0], 1);
}

#[test]
fn riscv64_uses_one_contiguous_window() {
    let mut args = ints(8);
    args.push(Arg::LongDouble(0x4000_aaaa_bbbb_cccc_dddd));
    let frame = assign(Arch::Riscv64, 0, &args);
    assert_eq!(frame.gp[7], 8);
    // The long double is 16-byte aligned relative to the start of the contiguous area: the
    // cursor is at byte 64, which is already aligned, so it goes into the stack area.
    assert_eq!(frame.stack.len(), 16);
    assert_eq!(
        u128::from_le_bytes(frame.stack[0..16].try_into().unwrap()),
        0x4000_aaaa_bbbb_cccc_dddd
    );
}

#[test]
fn build_list_places_the_va_list_where_the_stub_does() {
    let frame = assign(Arch::X86_64, 3, &ints(2));
    let built = build_list(Arch::X86_64, &frame, 3);
    assert_eq!(built.list, 16 + 176);
    assert_eq!(built.arena.read_u32(built.list), 24);
    assert_eq!(built.arena.read_u32(built.list + 4), 48);
    // reg_save_area points at the save area, overflow_arg_area at the stack.
    assert_eq!(
        built.arena.read_u64(built.list + 16) as usize,
        built.arena.ptr(16)
    );
    assert_eq!(
        built.arena.read_u64(built.list + 8) as usize,
        built.arena.ptr(built.stack)
    );
}

#[test]
fn build_list_aapcs64_offsets() {
    let frame = assign(Arch::Aapcs64, 2, &ints(1));
    let built = build_list(Arch::Aapcs64, &frame, 2);
    assert_eq!(
        built.arena.read_u64(built.list) as usize,
        built.arena.ptr(built.stack)
    );
    assert_eq!(
        built.arena.read_u64(built.list + 8) as usize,
        built.arena.ptr(16 + 80)
    );
    assert_eq!(
        built.arena.read_u64(built.list + 16) as usize,
        built.arena.ptr(16 + 208)
    );
    assert_eq!(
        built.arena.read_u32(built.list + 24),
        (-(64 - 16i32)) as u32
    );
}

#[test]
fn build_list_riscv64_cursor_is_the_first_free_register() {
    let frame = assign(Arch::Riscv64, 2, &ints(1));
    let built = build_list(Arch::Riscv64, &frame, 2);
    assert_eq!(built.list, 16 + 16);
    assert_eq!(built.arena.read_u64(built.list), 1);
}

#[test]
fn riscv64_large_aggregates_are_passed_by_reference() {
    let payload: Vec<u8> = (0..32u8).collect();
    let frame = assign(
        Arch::Riscv64,
        0,
        &[Arg::Agg {
            bytes: payload.clone(),
            align: 8,
        }],
    );
    let built = build_list(Arch::Riscv64, &frame, 0);
    // The cursor slot holds the address of the object.
    let pointer = built.arena.read_u64(built.list) as usize;
    assert_ne!(pointer, 0);
    // SAFETY: `patch_refs` pointed the slot inside the live arena.
    let bytes = unsafe { core::slice::from_raw_parts(pointer as *const u8, 32) };
    assert_eq!(bytes, &payload[..]);
}
