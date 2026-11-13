// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent TLS-layout model and generators for the C57 differential (no C recipe).
//!
//! The model re-derives the musl `__init_tls.c` arithmetic with `%` and division; it never calls
//! the crate under test. `vectors.toml` in `corpus/tls/` pins the hand-derived golden table.

use raven_libc_tls::{Abi, TlsModuleIn};

/// One generated case.
#[derive(Clone, Debug)]
pub struct Case {
    /// The ABI variant.
    pub abi: Abi,
    /// The module list.
    pub modules: Vec<TlsModuleIn>,
}

fn round_up_div(value: usize, align: usize) -> usize {
    let rem = value % align;
    if rem == 0 {
        value
    } else {
        value + (align - rem)
    }
}

/// The independent aligned size.
#[must_use]
pub fn model_size(module: &TlsModuleIn) -> usize {
    let align = if module.align == 0 {
        1
    } else {
        module.align.min(4096)
    };
    round_up_div(module.image_addr + module.memsz, align) - module.image_addr
}

/// The independent per-module TPOFF bases.
#[must_use]
pub fn model_offsets(abi: Abi, modules: &[TlsModuleIn]) -> Vec<usize> {
    let mut offsets = Vec::new();
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
                let mut end = cursor + model_size(module) + align - 1;
                end -= (end % align + module.image_addr % align) % align;
                cursor = end;
                offsets.push(end);
            }
        }
    }
    offsets
}

/// A deterministic generator: `count` modules from the seeded LCG.
#[must_use]
pub fn generate(seed: u64, count: usize) -> Case {
    let mut state = seed ^ 0x9e37_79b9_7f4a_7c15;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        state
    };
    let abi = match next() % 3 {
        0 => Abi::VariantII { dtp_offset: 0 },
        1 => Abi::VariantI {
            gap_above_tp: 16,
            dtp_offset: 0,
        },
        _ => Abi::VariantI {
            gap_above_tp: 0,
            dtp_offset: 0x800,
        },
    };
    let mut modules = Vec::with_capacity(count);
    for _ in 0..count {
        let align = 1usize << (next() % 13);
        let image = 0x1000usize.wrapping_add((next() as usize) & 0x3ffff);
        let memsz = (next() as usize) & 0xffff;
        let filesz = if memsz == 0 { 0 } else { memsz / 2 };
        modules.push(TlsModuleIn::new(image, filesz, memsz, align));
    }
    Case { abi, modules }
}
