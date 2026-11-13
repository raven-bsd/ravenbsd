// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::consts::{BUILTIN_TLS_SIZE, DTV_HEADROOM, MIN_TLS_ALIGN};
use crate::layout::{TlsModuleIn, compute_static_layout_for};
use crate::tcb::Pthread;

const TCB: usize = core::mem::size_of::<Pthread>();
const X86: Abi = Abi::VariantII { dtp_offset: 0 };
const ARM: Abi = Abi::VariantI {
    gap_above_tp: 16,
    dtp_offset: 0,
};

#[test]
fn variant_two_plan_matches_the_donor_placement() {
    let modules = [TlsModuleIn::new(0x1000, 0x30, 0x30, 16)];
    let layout = compute_static_layout_for(X86, &modules, TCB).unwrap();
    let plan = plan_copy_for(X86, &layout, 0x10_0000, TCB, &modules).unwrap();
    assert_eq!(plan.dtv, 0x10_0000);
    assert_eq!(plan.tcb, 0x10_0000 + layout.total - TCB);
    assert_eq!(plan.blocks[0], plan.tcb - 0x30);
    assert_eq!(plan.len, 1);
    assert_eq!(plan.words, 2);
}

#[test]
fn variant_one_plan_matches_the_donor_placement() {
    let modules = [TlsModuleIn::new(0x1000, 0x30, 0x30, 16)];
    let layout = compute_static_layout_for(ARM, &modules, TCB).unwrap();
    let plan = plan_copy_for(ARM, &layout, 0x10_0000, TCB, &modules).unwrap();
    assert_eq!(plan.dtv, 0x10_0000 + layout.total - 2 * 8);
    assert_eq!(plan.tcb, 0x10_0000);
    assert_eq!(plan.blocks[0], plan.tcb + TCB + 16);
    assert_eq!(plan.blocks[0] + 0x30, plan.dtv - 16);
}

#[test]
fn plan_of_no_module_still_has_one_dtv_word() {
    for abi in [X86, ARM] {
        let layout = compute_static_layout_for(abi, &[], TCB).unwrap();
        let plan = plan_copy_for(abi, &layout, 0x20_0000, TCB, &[]).unwrap();
        assert_eq!(plan.len, 0);
        assert_eq!(plan.words, 1);
        assert!(plan.dtv >= 0x20_0000);
        assert!(plan.tcb >= 0x20_0000);
    }
}

#[test]
fn plan_keeps_the_tcb_inside_the_area() {
    for abi in [X86, ARM] {
        for memsz in [0usize, 1, 0x30, 0x1000, 0x2000] {
            let modules = [TlsModuleIn::new(0x1000, memsz, memsz, 16)];
            let layout = compute_static_layout_for(abi, &modules, TCB).unwrap();
            let base = 0x40_0000usize;
            let plan = plan_copy_for(abi, &layout, base, TCB, &modules).unwrap();
            assert!(plan.tcb >= base, "{abi:?} {memsz}");
            assert!(plan.tcb + TCB <= base + layout.total, "{abi:?} {memsz}");
            assert!(plan.dtv >= base && plan.dtv < base + layout.total);
            for index in 0..plan.len {
                assert!(plan.blocks[index] >= base);
                assert!(plan.blocks[index] + memsz <= base + layout.total);
            }
        }
    }
}

#[test]
fn plan_aligns_the_pthread_on_variant_one() {
    for base in [0x10_0000usize, 0x10_0001, 0x10_0004, 0x10_0008] {
        let modules = [TlsModuleIn::new(0x1000, 0x30, 0x30, 16)];
        let layout = compute_static_layout_for(ARM, &modules, TCB).unwrap();
        let plan = plan_copy_for(ARM, &layout, base, TCB, &modules).unwrap();
        assert_eq!(plan.tcb % MIN_TLS_ALIGN, 0);
        assert!(plan.tcb >= base);
        assert!(plan.tcb < base + layout.total);
    }
}

#[test]
fn plan_is_deterministic() {
    let modules = [
        TlsModuleIn::new(0x1000, 0x30, 0x30, 16),
        TlsModuleIn::new(0x2000, 0x40, 0x50, 32),
    ];
    for abi in [X86, ARM] {
        let layout = compute_static_layout_for(abi, &modules, TCB).unwrap();
        let a = plan_copy_for(abi, &layout, 0x10_0000, TCB, &modules).unwrap();
        let b = plan_copy_for(abi, &layout, 0x10_0000, TCB, &modules).unwrap();
        assert_eq!(a, b);
    }
}

#[test]
fn reset_and_zero_bytes_cover_filesz_and_memsz() {
    let module = TlsModuleIn::new(0x1000, 0x20, 0x50, 16);
    assert_eq!(reset_bytes(&module), 0x20);
    assert_eq!(zero_bytes(&module), 0x30);
    assert_eq!(reset_bytes(&module) + zero_bytes(&module), module.memsz);
    let empty = TlsModuleIn::new(0, 0, 0, 1);
    assert_eq!(zero_bytes(&empty), 0);
}

#[test]
fn builtin_buffer_has_headroom_for_a_small_layout() {
    assert_eq!(BUILTIN_TLS.capacity(), BUILTIN_TLS_SIZE);
    assert!(BUILTIN_TLS_SIZE.is_power_of_two());
    let layout =
        compute_static_layout_for(X86, &[TlsModuleIn::new(0x1000, 0x30, 0x30, 16)], TCB).unwrap();
    assert!(layout.total <= BUILTIN_TLS_SIZE);
}

#[test]
fn headroom_accounts_for_the_dtv_words() {
    let modules = [TlsModuleIn::new(0x1000, 0x30, 0x30, 16)];
    for abi in [X86, ARM] {
        let layout = compute_static_layout_for(abi, &modules, TCB).unwrap();
        assert!(layout.total >= DTV_HEADROOM + TCB);
    }
}
