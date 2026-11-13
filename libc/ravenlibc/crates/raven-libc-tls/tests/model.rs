// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! An independent second implementation of the TLS arithmetic (C57 7.1).
//!
//! This model never uses the crate's bit-mask congruence: it computes remainders with `%` and
//! rounds with division. A mismatch is a diff failure; the seed is printed with the case.

use raven_libc_tls::consts::DTV_HEADROOM;
use raven_libc_tls::layout::{compute_static_layout_for, module_offsets_for};
use raven_libc_tls::{Abi, Pthread, TlsModuleIn};

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

fn round_up_div(value: usize, align: usize) -> usize {
    let rem = value % align;
    if rem == 0 {
        value
    } else {
        value + (align - rem)
    }
}

/// The aligned block size, computed by rounding the image end up to the alignment.
fn model_size(module: &TlsModuleIn) -> usize {
    let align = if module.align == 0 {
        1
    } else {
        module.align.min(4096)
    };
    let end = module
        .image_addr
        .checked_add(module.memsz)
        .expect("model domain stays below the address-space top");
    round_up_div(end, align) - module.image_addr
}

fn model_offsets(abi: Abi, modules: &[TlsModuleIn]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(modules.len());
    match abi {
        Abi::VariantI { gap_above_tp, .. } => {
            let mut cursor = gap_above_tp;
            for module in modules {
                let align = if module.align == 0 {
                    1
                } else {
                    module.align.min(4096)
                };
                let delta = (module.image_addr % align + align - cursor % align) % align;
                let offset = cursor + delta;
                offsets.push(offset);
                cursor = offset + model_size(module);
            }
        }
        Abi::VariantII { .. } => {
            let mut cursor = 0usize;
            for module in modules {
                let align = if module.align == 0 {
                    1
                } else {
                    module.align.min(4096)
                };
                let size = model_size(module);
                let mut end = cursor + size + align - 1;
                end -= (end % align + module.image_addr % align) % align;
                cursor = end;
                offsets.push(end);
            }
        }
    }
    offsets
}

fn model_layout(abi: Abi, modules: &[TlsModuleIn]) -> (usize, usize, usize, usize) {
    let offsets = model_offsets(abi, modules);
    let (size, offset) = if modules.is_empty() {
        (
            0,
            if abi.is_above_tp() {
                abi.gap_above_tp()
            } else {
                0
            },
        )
    } else if abi.is_above_tp() {
        let first = offsets[0];
        let last_size = model_size(modules.last().unwrap());
        let last_offset = *offsets.last().unwrap();
        (last_offset + last_size - first, first)
    } else {
        let last = *offsets.last().unwrap();
        (last, offsets[0])
    };
    let align = modules
        .iter()
        .map(|module| {
            if module.align == 0 {
                1
            } else {
                module.align.min(4096)
            }
        })
        .fold(16usize, usize::max);
    let mut sum = DTV_HEADROOM + TCB;
    if abi.is_above_tp() {
        sum += offset;
    }
    sum += size + align + 15;
    (size, offset, align, sum / 16 * 16)
}

fn lcg(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *state
}

#[test]
fn model_agrees_on_the_golden_cases() {
    let cases = [
        (0x1000usize, 0x30usize, 16usize),
        (0x1008, 0x28, 16),
        (0x1004, 0x14, 4),
        (0x2000, 0x10, 64),
        (0x3000, 0, 8),
    ];
    for abi in [X86, ARM, RISCV] {
        for (image, memsz, align) in cases {
            let module = TlsModuleIn::new(image, memsz, memsz, align);
            let got = compute_static_layout_for(abi, &[module], TCB).unwrap();
            let (size, offset, align_model, total) = model_layout(abi, &[module]);
            assert_eq!(got.size, size, "{abi:?} {image:#x}");
            assert_eq!(got.offset, offset, "{abi:?} {image:#x}");
            assert_eq!(got.align, align_model, "{abi:?} {image:#x}");
            assert_eq!(got.total, total, "{abi:?} {image:#x}");
        }
    }
}

#[test]
fn model_differential_100k_cases() {
    let mut state = 0x5eed_1234_9876_abcdu64;
    for case in 0..100_000u32 {
        let count = (lcg(&mut state) % 8 + 1) as usize;
        let abi = match lcg(&mut state) % 3 {
            0 => X86,
            1 => ARM,
            _ => RISCV,
        };
        let mut modules = Vec::with_capacity(count);
        for _ in 0..count {
            let align = 1usize << (lcg(&mut state) % 13);
            let image = 0x1000usize.wrapping_add((lcg(&mut state) as usize) & 0x3ffff);
            let memsz = (lcg(&mut state) as usize) & 0xffff;
            let filesz = if memsz == 0 { 0 } else { memsz / 2 };
            modules.push(TlsModuleIn::new(image, filesz, memsz, align));
        }
        let got = compute_static_layout_for(abi, &modules, TCB).unwrap();
        let offsets = module_offsets_for(abi, &modules).unwrap();
        let expected = model_offsets(abi, &modules);
        assert_eq!(
            offsets.len,
            expected.len(),
            "case {case} seed 0x5eed12349876abcd"
        );
        for (index, want) in expected.iter().enumerate() {
            assert_eq!(
                offsets.get(index),
                Some(*want),
                "case {case} module {index} {abi:?}"
            );
        }
        let (size, offset, align, total) = model_layout(abi, &modules);
        assert_eq!(got.size, size, "case {case} {abi:?}");
        assert_eq!(got.offset, offset, "case {case} {abi:?}");
        assert_eq!(got.align, align, "case {case} {abi:?}");
        assert_eq!(got.total, total, "case {case} {abi:?}");
    }
}

#[test]
fn model_rejects_what_the_crate_rejects() {
    for align in [3usize, 6, 9, 17, 4097] {
        let module = TlsModuleIn::new(0x1000, 8, 8, align);
        assert!(compute_static_layout_for(X86, &[module], TCB).is_err());
    }
}

#[test]
fn model_and_crate_agree_on_the_no_module_case() {
    for abi in [X86, ARM, RISCV] {
        let got = compute_static_layout_for(abi, &[], TCB).unwrap();
        let (size, offset, align, total) = model_layout(abi, &[]);
        assert_eq!(
            (got.size, got.offset, got.align, got.total),
            (size, offset, align, total)
        );
    }
}
