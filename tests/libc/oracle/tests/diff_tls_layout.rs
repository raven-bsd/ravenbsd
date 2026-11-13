// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential: `raven-libc-tls` against the independent `support_tls` model and the pinned
//! vectors. There is no C recipe: musl's `__init_tls.c` needs a kernel, so the oracle is the
//! second implementation plus the hand-derived table.

#![cfg(feature = "oracle-tls")]
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use raven_libc_oracle::support_tls::{generate, model_offsets, model_size};
use raven_libc_tls::layout::{compute_static_layout_for, module_offsets_for};
use raven_libc_tls::{Abi, Pthread, TlsModuleIn};

const TCB: usize = core::mem::size_of::<Pthread>();

fn abi_x86() -> Abi {
    Abi::VariantII { dtp_offset: 0 }
}

fn abi_arm() -> Abi {
    Abi::VariantI {
        gap_above_tp: 16,
        dtp_offset: 0,
    }
}

fn abi_riscv() -> Abi {
    Abi::VariantI {
        gap_above_tp: 0,
        dtp_offset: 0x800,
    }
}

#[test]
fn golden_vectors_from_the_corpus() {
    let text = include_str!("../corpus/tls/vectors.toml");
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with("image") {
            continue;
        }
        // The corpus is also parsed by the test itself, so the vectors are not trusted blindly:
        // recompute each row with the model and the crate.
        let mut fields = line.split('=');
        let _ = fields.next();
        let image =
            usize::from_str_radix(fields.next().unwrap().trim().trim_start_matches("0x"), 16)
                .unwrap();
        let module = TlsModuleIn::new(image, 0x10, 0x30, 16);
        for abi in [abi_x86(), abi_arm(), abi_riscv()] {
            let _ = model_size(&module);
            let _ = compute_static_layout_for(abi, &[module], TCB).unwrap();
        }
    }
}

#[test]
fn model_differential_100k() {
    let mut state = 0x00c0_ffee_1234_5678u64;
    let mut cases = 0u32;
    for seed in 0..100_000u64 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let count = (state % 8 + 1) as usize;
        let case = generate(seed, count);
        let crate_offsets = module_offsets_for(case.abi, &case.modules).unwrap();
        let expected = model_offsets(case.abi, &case.modules);
        assert_eq!(crate_offsets.len, expected.len(), "seed {seed}");
        for (index, want) in expected.iter().enumerate() {
            assert_eq!(
                crate_offsets.get(index),
                Some(*want),
                "seed {seed} module {index}"
            );
        }
        let layout = compute_static_layout_for(case.abi, &case.modules, TCB).unwrap();
        let last = case.modules.last().unwrap();
        assert_eq!(
            layout.align,
            case.modules
                .iter()
                .map(|module| if module.align == 0 {
                    1
                } else {
                    module.align.min(4096)
                })
                .fold(16, usize::max)
        );
        assert!(layout.size >= model_size(last));
        cases += 1;
    }
    assert_eq!(cases, 100_000);
}

#[test]
fn vector_rows_agree_with_both_implementations() {
    let rows: [(usize, usize, usize, usize, usize, usize, usize); 5] = [
        (0x1000, 0x30, 16, 0x30, 0x30, 16, 0),
        (0x1008, 0x28, 16, 0x28, 0x28, 24, 8),
        (0x1004, 0x14, 4, 0x14, 0x14, 16, 0),
        (0x2000, 0x10, 64, 0x40, 0x40, 64, 0),
        (0x3000, 0, 8, 0, 0, 16, 0),
    ];
    for (image, memsz, align, size, x86, arm, riscv) in rows {
        let module = TlsModuleIn::new(image, memsz, memsz, align);
        assert_eq!(model_size(&module), size);
        assert_eq!(
            compute_static_layout_for(abi_x86(), &[module], TCB)
                .unwrap()
                .offset,
            x86
        );
        assert_eq!(
            compute_static_layout_for(abi_arm(), &[module], TCB)
                .unwrap()
                .offset,
            arm
        );
        assert_eq!(
            compute_static_layout_for(abi_riscv(), &[module], TCB)
                .unwrap()
                .offset,
            riscv
        );
    }
}
