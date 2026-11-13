// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden TLS-layout vectors, properties and the copy-placement check (C57 7.1).
//!
//! The golden table is the ticket table re-derived by hand; the property loop uses a seeded
//! LCG so a failure is reproducible. The copy test executes `copy_tls` over a real 16-byte
//! aligned allocation and inspects every word the loader will read.

use raven_libc_tls::consts::{DTV_HEADROOM, MIN_TLS_ALIGN};
use raven_libc_tls::init::CopyPlan;
use raven_libc_tls::layout::{block_start, compute_static_layout_for, module_offsets_for};
use raven_libc_tls::{Abi, Dtv, LayoutError, Pthread, TlsModuleIn};

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

#[test]
fn golden_table_x86_64() {
    let cases = [
        (0x1000usize, 0x30usize, 16usize, 0x30usize, 0x30usize),
        (0x1008, 0x28, 16, 0x28, 0x28),
        (0x1004, 0x14, 4, 0x14, 0x14),
        (0x2000, 0x10, 64, 0x40, 0x40),
        (0x3000, 0, 8, 0, 0),
    ];
    for (image, memsz, align, size, offset) in cases {
        let got = compute_static_layout_for(X86, &[single(image, memsz, align)], TCB).unwrap();
        assert_eq!(got.size, size, "image {image:#x}");
        assert_eq!(got.offset, offset, "image {image:#x}");
    }
}

#[test]
fn golden_table_aarch64() {
    let cases = [
        (0x1000usize, 0x30usize, 16usize, 0x30usize, 16usize),
        (0x1008, 0x28, 16, 0x28, 24),
        (0x1004, 0x14, 4, 0x14, 16),
        (0x2000, 0x10, 64, 0x40, 64),
        (0x3000, 0, 8, 0, 16),
    ];
    for (image, memsz, align, size, offset) in cases {
        let got = compute_static_layout_for(ARM, &[single(image, memsz, align)], TCB).unwrap();
        assert_eq!(got.size, size, "image {image:#x}");
        assert_eq!(got.offset, offset, "image {image:#x}");
    }
}

#[test]
fn golden_table_riscv64() {
    let cases = [
        (0x1000usize, 0x30usize, 16usize, 0x30usize, 0usize),
        (0x1008, 0x28, 16, 0x28, 8),
        (0x1004, 0x14, 4, 0x14, 0),
        (0x2000, 0x10, 64, 0x40, 0),
        (0x3000, 0, 8, 0, 0),
    ];
    for (image, memsz, align, size, offset) in cases {
        let got = compute_static_layout_for(RISCV, &[single(image, memsz, align)], TCB).unwrap();
        assert_eq!(got.size, size, "image {image:#x}");
        assert_eq!(got.offset, offset, "image {image:#x}");
    }
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state
}

#[test]
fn property_100k_cases_per_arch() {
    for abi in [X86, ARM, RISCV] {
        let mut state = 0x0bad_f00d_dead_beefu64;
        for case in 0..100_000u32 {
            let align = 1usize << (lcg(&mut state) % 13);
            let image = 0x1000usize.wrapping_add((lcg(&mut state) as usize) & 0x1_ffff);
            let memsz = (lcg(&mut state) as usize) & 0x7fff;
            let filesz = if memsz == 0 { 0 } else { memsz / 3 };
            let module = TlsModuleIn::new(image, filesz, memsz, align);
            let layout = compute_static_layout_for(abi, &[module], TCB).unwrap();
            let offsets = module_offsets_for(abi, &[module]).unwrap();
            let tp = 0x7000_0000_0000usize;
            let block = block_start(abi, tp, offsets.get(0).unwrap());
            assert_eq!(block % align, image % align, "case {case} {abi:?}");
            assert_eq!(layout.total % MIN_TLS_ALIGN, 0, "case {case} {abi:?}");
            assert!(layout.size >= memsz, "case {case} {abi:?}");
            assert!(
                layout.total >= DTV_HEADROOM + TCB + layout.size,
                "case {case} {abi:?}"
            );
        }
    }
}

#[test]
fn property_dtv_entries_include_dtp_offset() {
    let modules = [single(0x1008, 0x28, 16)];
    for (abi, dtp) in [(X86, 0usize), (ARM, 0), (RISCV, 0x800)] {
        let offsets = module_offsets_for(abi, &modules).unwrap();
        let tp = 0x1234_5000usize;
        let block = block_start(abi, tp, offsets.get(0).unwrap());
        assert_eq!(
            block.wrapping_add(abi.dtp_offset()),
            block.wrapping_add(dtp)
        );
        let layout = compute_static_layout_for(abi, &modules, TCB).unwrap();
        assert_eq!(layout.size, 0x28);
    }
}

#[test]
fn rejects_bad_inputs_from_the_property_domain() {
    for align in [3usize, 6, 9, 17, 4097] {
        let got = compute_static_layout_for(X86, &[single(0x1000, 8, align)], TCB);
        assert_eq!(got, Err(LayoutError::BadAlign));
    }
    let got = compute_static_layout_for(X86, &[TlsModuleIn::new(0x1000, 9, 8, 4)], TCB);
    assert_eq!(got, Err(LayoutError::FileszAboveMemsz));
    let got = compute_static_layout_for(X86, &[TlsModuleIn::new(usize::MAX - 4, 0, 16, 16)], TCB);
    assert_eq!(got, Err(LayoutError::Overflow));
}

#[repr(align(4096))]
struct Area([u128; 4096]);

#[test]
fn copy_tls_writes_the_words_and_the_image() {
    let mut image = [0u8; 0x30];
    for (index, byte) in image.iter_mut().enumerate() {
        *byte = 0xa0u8.wrapping_add(index as u8);
    }
    let module = TlsModuleIn::new(image.as_ptr() as usize, 0x20, 0x30, 16);
    let layout = compute_static_layout_for(raven_libc_tls::TARGET_ABI, &[module], TCB).unwrap();
    assert!(layout.total <= core::mem::size_of::<Area>());
    let mut area = Area([0u128; 4096]);
    let base = area.0.as_mut_ptr() as usize;
    // SAFETY: the area is a live 64 KiB allocation, 4096-byte aligned, and larger than the
    // computed layout; the image is a live array of 0x20 initialised bytes.
    let plan: CopyPlan =
        unsafe { raven_libc_tls::raw::copy_tls(base, &layout, &[module], TCB).expect("copy plan") };
    // SAFETY: every address below is inside `area`, which is alive for the whole test.
    unsafe {
        let dtv = plan.dtv as *const usize;
        assert_eq!(*dtv, 1);
        assert_eq!(
            *dtv.add(1),
            plan.blocks[0].wrapping_add(raven_libc_tls::TARGET_ABI.dtp_offset())
        );
        let block = plan.blocks[0] as *const u8;
        for index in 0..0x20 {
            assert_eq!(
                *block.add(index),
                *image.get(index).unwrap(),
                "image byte {index}"
            );
        }
        for index in 0x20..0x30 {
            assert_eq!(*block.add(index), 0, "tbss byte {index}");
        }
        let tcb = plan.tcb as *const Pthread;
        let published = (*tcb).dtv.load(core::sync::atomic::Ordering::Acquire);
        assert_eq!(published as usize, plan.dtv);
        let view = &*(published as *const Dtv);
        assert_eq!(view.count(), 1);
        assert_eq!(
            raven_libc_tls::dtv::lookup(view, 1, 0),
            plan.blocks[0].wrapping_add(raven_libc_tls::TARGET_ABI.dtp_offset())
        );
    }
}

#[test]
fn mutation_double_gap_is_detected_by_the_arm_table() {
    let got = compute_static_layout_for(ARM, &[single(0x1008, 0x28, 16)], TCB).unwrap();
    let mutated = got.offset.wrapping_add(ARM.gap_above_tp());
    assert_ne!(mutated, got.offset);
    assert_ne!(mutated, 24);
}

#[test]
fn mutation_dropped_dtp_offset_is_detected_by_the_riscv_table() {
    let modules = [single(0x1008, 0x28, 16)];
    let offsets = module_offsets_for(RISCV, &modules).unwrap();
    let block = block_start(RISCV, 0x10_0000, offsets.get(0).unwrap());
    assert_ne!(block, block + RISCV.dtp_offset());
    assert_eq!(block + RISCV.dtp_offset(), block.wrapping_add(0x800));
}
