// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::consts::{DTV_HEADROOM, MAX_TLS_MODULES, MIN_TLS_ALIGN};
use crate::tcb::Pthread;

const TCB: usize = core::mem::size_of::<Pthread>();
const X86: Abi = Abi::VariantII { dtp_offset: 0 };
const ARM: Abi = Abi::VariantI {
    gap_above_tp: 16,
    dtp_offset: 0,
};
const RISCV: Abi = Abi::VariantI {
    gap_above_tp: 0,
    dtp_offset: 0x800,
};

fn single(image: usize, memsz: usize, align: usize) -> TlsModuleIn {
    TlsModuleIn::new(image, memsz, memsz, align)
}

fn layout(abi: Abi, image: usize, memsz: usize, align: usize) -> StaticLayout {
    compute_static_layout_for(abi, &[single(image, memsz, align)], TCB).unwrap()
}

#[test]
fn golden_x86_64() {
    assert_eq!(
        (
            layout(X86, 0x1000, 0x30, 16).size,
            layout(X86, 0x1000, 0x30, 16).offset
        ),
        (0x30, 0x30)
    );
    assert_eq!(
        (
            layout(X86, 0x1008, 0x28, 16).size,
            layout(X86, 0x1008, 0x28, 16).offset
        ),
        (0x28, 0x28)
    );
    assert_eq!(
        (
            layout(X86, 0x1004, 0x14, 4).size,
            layout(X86, 0x1004, 0x14, 4).offset
        ),
        (0x14, 0x14)
    );
    assert_eq!(
        (
            layout(X86, 0x2000, 0x10, 64).size,
            layout(X86, 0x2000, 0x10, 64).offset
        ),
        (0x40, 0x40)
    );
    assert_eq!(
        (
            layout(X86, 0x3000, 0, 8).size,
            layout(X86, 0x3000, 0, 8).offset
        ),
        (0, 0)
    );
}

#[test]
fn golden_aarch64() {
    assert_eq!(layout(ARM, 0x1000, 0x30, 16).size, 0x30);
    assert_eq!(layout(ARM, 0x1000, 0x30, 16).offset, 16);
    assert_eq!(layout(ARM, 0x1008, 0x28, 16).size, 0x28);
    assert_eq!(layout(ARM, 0x1008, 0x28, 16).offset, 24);
    assert_eq!(layout(ARM, 0x1004, 0x14, 4).size, 0x14);
    assert_eq!(layout(ARM, 0x1004, 0x14, 4).offset, 16);
    assert_eq!(layout(ARM, 0x2000, 0x10, 64).size, 0x40);
    assert_eq!(layout(ARM, 0x2000, 0x10, 64).offset, 64);
    assert_eq!(layout(ARM, 0x3000, 0, 8).size, 0);
    assert_eq!(layout(ARM, 0x3000, 0, 8).offset, 16);
}

#[test]
fn golden_riscv64() {
    assert_eq!(layout(RISCV, 0x1000, 0x30, 16).size, 0x30);
    assert_eq!(layout(RISCV, 0x1000, 0x30, 16).offset, 0);
    assert_eq!(layout(RISCV, 0x1008, 0x28, 16).size, 0x28);
    assert_eq!(layout(RISCV, 0x1008, 0x28, 16).offset, 8);
    assert_eq!(layout(RISCV, 0x1004, 0x14, 4).size, 0x14);
    assert_eq!(layout(RISCV, 0x1004, 0x14, 4).offset, 0);
    assert_eq!(layout(RISCV, 0x2000, 0x10, 64).size, 0x40);
    assert_eq!(layout(RISCV, 0x2000, 0x10, 64).offset, 0);
    assert_eq!(layout(RISCV, 0x3000, 0, 8).size, 0);
    assert_eq!(layout(RISCV, 0x3000, 0, 8).offset, 0);
}

#[test]
fn each_golden_arch_matches_its_target_table() {
    let table = [
        (0x1000usize, 0x30usize, 16usize),
        (0x1008, 0x28, 16),
        (0x1004, 0x14, 4),
        (0x2000, 0x10, 64),
        (0x3000, 0, 8),
    ];
    let expected = [
        (
            X86,
            [0x30usize, 0x28, 0x14, 0x40, 0],
            [0x30usize, 0x28, 0x14, 0x40, 0],
        ),
        (ARM, [0x30, 0x28, 0x14, 0x40, 0], [16, 24, 16, 64, 16]),
        (RISCV, [0x30, 0x28, 0x14, 0x40, 0], [0, 8, 0, 0, 0]),
    ];
    for (abi, sizes, offsets) in expected {
        for (index, (image, memsz, align)) in table.iter().enumerate() {
            let got = layout(abi, *image, *memsz, *align);
            assert_eq!(got.size, sizes[index], "size case {index} for {abi:?}");
            assert_eq!(
                got.offset, offsets[index],
                "offset case {index} for {abi:?}"
            );
        }
    }
}

#[test]
fn no_module_has_zero_count_and_gap_offset() {
    let x86 = compute_static_layout_for(X86, &[], TCB).unwrap();
    assert_eq!(x86.tls_cnt, 0);
    assert_eq!(x86.size, 0);
    assert_eq!(x86.offset, 0);
    let arm = compute_static_layout_for(ARM, &[], TCB).unwrap();
    assert_eq!(arm.tls_cnt, 0);
    assert_eq!(arm.offset, 16);
    let riscv = compute_static_layout_for(RISCV, &[], TCB).unwrap();
    assert_eq!(riscv.tls_cnt, 0);
    assert_eq!(riscv.offset, 0);
}

#[test]
fn total_rounds_up_to_min_alignment() {
    for abi in [X86, ARM, RISCV] {
        for memsz in [0usize, 1, 7, 16, 0x30, 255, 4096] {
            let got = layout(abi, 0x1000, memsz, 16);
            assert_eq!(got.total % MIN_TLS_ALIGN, 0, "{abi:?} memsz {memsz}");
            assert!(got.total >= DTV_HEADROOM + TCB + got.size + got.offset.min(16));
        }
    }
}

#[test]
fn total_follows_the_donor_precedence() {
    let got = layout(X86, 0x1000, 0x30, 16);
    let expected =
        (DTV_HEADROOM + TCB + got.size + got.align + MIN_TLS_ALIGN - 1) & !(MIN_TLS_ALIGN - 1);
    assert_eq!(got.total, expected);
    let arm = layout(ARM, 0x1000, 0x30, 16);
    let expected = (DTV_HEADROOM + TCB + arm.offset + arm.size + arm.align + MIN_TLS_ALIGN - 1)
        & !(MIN_TLS_ALIGN - 1);
    assert_eq!(arm.total, expected);
}

#[test]
fn alignment_is_capped_at_4096() {
    let got = layout(X86, 0x1000, 0x10, 8192);
    assert_eq!(got.align, 4096);
    let got = layout(X86, 0x1000, 0x10, 0);
    assert_eq!(got.align, MIN_TLS_ALIGN);
}

#[test]
fn align_zero_means_one() {
    let got = layout(X86, 0x1001, 3, 0);
    assert_eq!(got.size, 3);
    assert_eq!(got.align, MIN_TLS_ALIGN);
}

#[test]
fn rejects_non_power_of_two_alignment() {
    for align in [3usize, 5, 6, 7, 9, 12, 100, 4097] {
        assert_eq!(
            compute_static_layout_for(X86, &[single(0x1000, 0x10, align)], TCB),
            Err(LayoutError::BadAlign),
            "align {align}"
        );
    }
}

#[test]
fn rejects_filesz_above_memsz() {
    let module = TlsModuleIn::new(0x1000, 0x20, 0x10, 16);
    assert_eq!(
        compute_static_layout_for(X86, &[module], TCB),
        Err(LayoutError::FileszAboveMemsz)
    );
}

#[test]
fn rejects_overflow_near_the_top_of_the_address_space() {
    let module = TlsModuleIn::new(usize::MAX - 8, 0, 0x20, 16);
    assert_eq!(
        compute_static_layout_for(X86, &[module], TCB),
        Err(LayoutError::Overflow)
    );
}

#[test]
fn rejects_oversized_module() {
    let module = TlsModuleIn::new(0x1000, 0, isize::MAX as usize, 1);
    assert_eq!(
        compute_static_layout_for(X86, &[module], TCB),
        Err(LayoutError::TooLarge)
    );
}

#[test]
fn rejects_more_than_the_module_limit() {
    let modules = [single(0x1000, 0x10, 16); MAX_TLS_MODULES + 1];
    assert_eq!(
        compute_static_layout_for(X86, &modules, TCB),
        Err(LayoutError::TooLarge)
    );
}

#[test]
fn error_errno_mapping() {
    assert_eq!(LayoutError::BadAlign.errno(), 8);
    assert_eq!(LayoutError::FileszAboveMemsz.errno(), 8);
    assert_eq!(LayoutError::Overflow.errno(), 12);
    assert_eq!(LayoutError::TooLarge.errno(), 12);
}

#[test]
fn block_start_is_tp_relative_by_variant() {
    assert_eq!(block_start(X86, 0x1_0000, 0x40), 0x1_0000 - 0x40);
    assert_eq!(block_start(ARM, 0x1_0000, 0x40), 0x1_0000 + 0x40);
    assert_eq!(dtv_entry(RISCV, 0x1_0000, 0x40), 0x1_0000 + 0x40 + 0x800);
}

#[test]
fn module_offsets_are_cumulative_and_congruent() {
    let modules = [
        TlsModuleIn::new(0x1000, 0x30, 0x30, 16),
        TlsModuleIn::new(0x1008, 0x28, 0x40, 32),
        TlsModuleIn::new(0x1004, 0x14, 0x14, 4),
    ];
    for abi in [X86, ARM, RISCV] {
        let offsets = module_offsets_for(abi, &modules).unwrap();
        assert_eq!(offsets.len, 3);
        let tp = 0x1_0000_0000usize;
        for (index, module) in modules.iter().enumerate() {
            let offset = offsets.get(index).unwrap();
            let block = block_start(abi, tp, offset);
            assert_eq!(
                block % module.align,
                module.image_addr % module.align,
                "{abi:?} module {index}"
            );
            assert_eq!(offsets.get(index + 1).is_some(), index + 1 < modules.len());
        }
    }
    assert_eq!(module_offsets_for(X86, &modules).unwrap().get(3), None);
}

#[test]
fn module_offsets_match_the_layout_for_one_module() {
    for abi in [X86, ARM, RISCV] {
        for (image, memsz, align) in [
            (0x1000usize, 0x30usize, 16usize),
            (0x1008, 0x28, 16),
            (0x2000, 0x10, 64),
            (0x3000, 0, 8),
        ] {
            let modules = [single(image, memsz, align)];
            let offsets = module_offsets_for(abi, &modules).unwrap();
            let got = compute_static_layout_for(abi, &modules, TCB).unwrap();
            assert_eq!(offsets.get(0), Some(got.offset), "{abi:?} {image:x}");
        }
    }
}

#[test]
fn random_modules_keep_the_congruence() {
    let mut state = 0x1234_5678_9abc_def0u64;
    for case in 0..2000 {
        let mut next = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            state
        };
        let align = 1usize << (next() % 13);
        let image = 0x1000usize.wrapping_add((next() as usize) & 0xffff);
        let memsz = (next() as usize) & 0x3fff;
        let module = TlsModuleIn::new(image, memsz / 2, memsz, align);
        for abi in [X86, ARM, RISCV] {
            let offsets = module_offsets_for(abi, &[module]).unwrap();
            let block = block_start(abi, 0x10_0000, offsets.get(0).unwrap());
            assert_eq!(block % align, image % align, "case {case} {abi:?}");
            let layout = compute_static_layout_for(abi, &[module], TCB).unwrap();
            assert_eq!(layout.total % MIN_TLS_ALIGN, 0);
        }
    }
}
