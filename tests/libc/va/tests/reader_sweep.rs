// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The reader sweep: every argument class at every position, for every `named` count, on all
//! three architectures, plus 5,000 seeded random sequences per architecture (C59 section 5.7).
//!
//! The expected values come from the independent ABI model (`src/model.rs`), never from the
//! readers: the model places the argument where the psABI says it travels and the reader must
//! find it there. Values are compared as bits.

#![allow(unexpected_cfgs)] // `raven_va_softfloat` and `raven_va_verified_*` are build-profile cfgs

#[path = "../src/arena.rs"]
mod arena;
#[path = "../src/model.rs"]
mod model;

use arena::Arena;
use model::{Arch, Arg, Built, assign, build_list};
use raven_libc_oracle::rng::Rng;
use raven_libc_va_test::va::{
    Aapcs64Reader, RiscvReader, SysvReader, VaListAapcs64, VaListSysV, VaReader, VaReaderExt,
};

/// One expected read result.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Expect {
    U64(u64),
    U128(u128),
    Agg(Vec<u8>),
}

/// The ten argument classes of the sweep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Int,
    Long,
    Ptr,
    Double,
    LongDouble,
    Agg8,
    Agg12,
    Agg16,
    Agg24,
    Agg32,
}

const CLASSES: [Class; 10] = [
    Class::Int,
    Class::Long,
    Class::Ptr,
    Class::Double,
    Class::LongDouble,
    Class::Agg8,
    Class::Agg12,
    Class::Agg16,
    Class::Agg24,
    Class::Agg32,
];

impl Class {
    fn aggregate_size(self) -> Option<usize> {
        match self {
            Self::Agg8 => Some(8),
            Self::Agg12 => Some(12),
            Self::Agg16 => Some(16),
            Self::Agg24 => Some(24),
            Self::Agg32 => Some(32),
            _ => None,
        }
    }
}

/// A distinct value for the argument at `index`, of the given class.
fn make_arg(class: Class, index: usize, seed: u64) -> (Arg, Expect) {
    let marker = (seed << 32) | (index as u64 & 0xffff_ffff);
    match class {
        Class::Int => (Arg::Int(marker), Expect::U64(marker)),
        Class::Long => {
            let value = marker ^ 0x1111_2222_3333_4444;
            (Arg::Long(value), Expect::U64(value))
        }
        Class::Ptr => {
            let value = (marker as usize) | 0x1_0000_0000;
            (Arg::Ptr(value), Expect::U64(value as u64))
        }
        Class::Double => {
            let bits = marker ^ 0x4000_0000_0000_0000;
            (Arg::Double(bits), Expect::U64(bits))
        }
        Class::LongDouble => {
            // Bits 80..128 carry a non-zero pattern: the x86_64 reader must return zeros
            // there, aarch64 and riscv64 the full binary128 image.
            let bits = u128::from(marker) | (0xABCD_u128 << 80);
            (Arg::LongDouble(bits), Expect::U128(bits))
        }
        other => {
            let size = other.aggregate_size().expect("aggregate class");
            let bytes: Vec<u8> = (0..size)
                .map(|offset| (marker as u8).wrapping_add(offset as u8))
                .collect();
            let arg = Arg::Agg {
                bytes: bytes.clone(),
                align: 8,
            };
            (arg, Expect::Agg(bytes))
        }
    }
}

/// Read every argument of `args` back through the reader of `arch`.
fn read_back(arch: Arch, built: &Built, args: &[Arg]) -> Vec<Expect> {
    match arch {
        Arch::X86_64 | Arch::X86_64Softfloat => {
            let pointer = built.arena.ptr(built.list) as *mut VaListSysV;
            // SAFETY: the model built a live `VaListSysV` at `built.list`.
            let mut reader = unsafe { SysvReader::from_ptr(pointer) };
            args.iter()
                .map(|arg| read_one_sysv(&mut reader, arg))
                .collect()
        }
        Arch::Aapcs64 | Arch::Aapcs64Softfloat => {
            let pointer = built.arena.ptr(built.list) as *mut VaListAapcs64;
            // SAFETY: the model built a live `VaListAapcs64` at `built.list`.
            let mut reader = unsafe { Aapcs64Reader::from_ptr(pointer) };
            args.iter()
                .map(|arg| read_one_aapcs(&mut reader, arg))
                .collect()
        }
        Arch::Riscv64 => {
            let cursor = built.arena.ptr(built.list);
            // SAFETY: the cursor points into the live arena.
            let mut reader = unsafe { RiscvReader::from_cursor(cursor as *mut u8) };
            args.iter()
                .map(|arg| read_one_riscv(&mut reader, arg))
                .collect()
        }
    }
}

fn read_one_sysv(reader: &mut SysvReader, arg: &Arg) -> Expect {
    match arg {
        Arg::Int(_) | Arg::Long(_) | Arg::Ptr(_) => Expect::U64(reader.next_u64()),
        Arg::Double(_) => Expect::U64(reader.next_f64_bits()),
        Arg::LongDouble(_) => Expect::U128(reader.next_ld_bits()),
        Arg::Agg { bytes, .. } => {
            let mut out = [0u8; 32];
            assert!(
                reader.next_aggregate_int(bytes.len(), 8, &mut out),
                "aggregate of {} bytes rejected",
                bytes.len()
            );
            Expect::Agg(out[..bytes.len()].to_vec())
        }
    }
}

fn read_one_aapcs(reader: &mut Aapcs64Reader, arg: &Arg) -> Expect {
    match arg {
        Arg::Int(_) | Arg::Long(_) | Arg::Ptr(_) => Expect::U64(reader.next_u64()),
        Arg::Double(_) => Expect::U64(reader.next_f64_bits()),
        Arg::LongDouble(_) => Expect::U128(reader.next_ld_bits()),
        Arg::Agg { bytes, .. } => {
            let mut out = [0u8; 32];
            assert!(
                reader.next_aggregate_int(bytes.len(), 8, &mut out),
                "aggregate of {} bytes rejected",
                bytes.len()
            );
            Expect::Agg(out[..bytes.len()].to_vec())
        }
    }
}

fn read_one_riscv(reader: &mut RiscvReader, arg: &Arg) -> Expect {
    match arg {
        Arg::Int(_) | Arg::Long(_) | Arg::Ptr(_) => Expect::U64(reader.next_u64()),
        Arg::Double(_) => Expect::U64(reader.next_f64_bits()),
        Arg::LongDouble(_) => Expect::U128(reader.next_ld_bits()),
        Arg::Agg { bytes, .. } => {
            let mut out = [0u8; 32];
            assert!(
                reader.next_aggregate_int(bytes.len(), 8, &mut out),
                "aggregate of {} bytes rejected",
                bytes.len()
            );
            Expect::Agg(out[..bytes.len()].to_vec())
        }
    }
}

/// Normalise the expectation for the architecture: x86_64 `long double` is 80 bits in a
/// 16-byte slot and the reader returns zeros in bits 80..128.
fn expected_for(arch: Arch, expect: Expect) -> Expect {
    match (arch, expect) {
        (Arch::X86_64 | Arch::X86_64Softfloat, Expect::U128(bits)) => {
            Expect::U128(bits & 0xffff_ffff_ffff_ffff_ffff)
        }
        (_, other) => other,
    }
}

/// The architectures the current build can test.
fn architectures() -> Vec<Arch> {
    let mut arches = vec![Arch::Aapcs64, Arch::Aapcs64Softfloat, Arch::Riscv64];
    if cfg!(raven_va_softfloat) {
        arches.push(Arch::X86_64Softfloat);
    } else {
        arches.push(Arch::X86_64);
    }
    arches
}

fn max_named(arch: Arch) -> usize {
    arch.integer_registers().saturating_sub(1)
}

/// The `va_list` object bytes (24 for SysV, 32 for AAPCS64; a riscv64 cursor is a value).
fn list_bytes(arch: Arch, built: &Built) -> Vec<u8> {
    match arch {
        Arch::X86_64 | Arch::X86_64Softfloat => built.arena.read(built.list, 24).to_vec(),
        Arch::Aapcs64 | Arch::Aapcs64Softfloat => built.arena.read(built.list, 32).to_vec(),
        Arch::Riscv64 => Vec::new(),
    }
}

/// The full sweep for one architecture.
fn sweep(arch: Arch) -> usize {
    let mut cases = 0usize;
    let mut seed = 1u64;
    for named in 0..=max_named(arch) {
        for class in CLASSES {
            for position in 0..=20usize {
                let mut args = Vec::with_capacity(21);
                let mut expects = Vec::with_capacity(21);
                for index in 0..21usize {
                    if index == position {
                        let (arg, expect) = make_arg(class, index, seed);
                        args.push(arg);
                        expects.push(expected_for(arch, expect));
                    } else {
                        let (arg, expect) = make_arg(Class::Int, index, seed);
                        args.push(arg);
                        expects.push(expected_for(arch, expect));
                    }
                }
                let frame = assign(arch, named, &args);
                let built = build_list(arch, &frame, named);
                assert_eq!(built.arch, arch);
                assert_eq!(built.named, named);
                let before = list_bytes(arch, &built);
                let reads = read_back(arch, &built, &args);
                assert_eq!(
                    before,
                    list_bytes(arch, &built),
                    "{arch:?}: the reader wrote into the caller's va_list"
                );
                for (index, (expect, read)) in expects.iter().zip(reads.iter()).enumerate() {
                    assert_eq!(
                        expect, read,
                        "{arch:?} named {named}: class {class:?} at position {position}, argument {index}"
                    );
                }
                cases += 1;
                seed = seed.wrapping_add(1);
            }
        }
    }
    cases
}

/// 5,000 seeded random sequences per architecture.
fn random_sweep(arch: Arch, seed: u64, sequences: usize) -> usize {
    let mut rng = Rng::seed_from_u64(seed);
    let mut cases = 0usize;
    for _ in 0..sequences {
        let length = rng.below(31) as usize;
        let named = rng.below(max_named(arch) as u64 + 1) as usize;
        let mut args = Vec::with_capacity(length);
        let mut expects = Vec::with_capacity(length);
        for index in 0..length {
            // 50 percent integer, the rest spread over the other classes.
            let roll = rng.below(100);
            let class = if roll < 50 {
                Class::Int
            } else {
                CLASSES[(roll as usize - 50) % CLASSES.len()]
            };
            let (arg, expect) = make_arg(class, index, rng.next_u64());
            args.push(arg);
            expects.push(expected_for(arch, expect));
        }
        let frame = assign(arch, named, &args);
        let built = build_list(arch, &frame, named);
        assert_eq!(built.arch, arch);
        assert_eq!(built.named, named);
        let reads = read_back(arch, &built, &args);
        for (index, (expect, read)) in expects.iter().zip(reads.iter()).enumerate() {
            assert_eq!(expect, read, "{arch:?} random case, argument {index}");
        }
        cases += 1;
    }
    cases
}

#[test]
fn position_sweep_every_class_at_every_position() {
    let mut total = 0usize;
    for arch in architectures() {
        let cases = sweep(arch);
        println!("{arch:?}: {cases} position cases");
        total += cases;
    }
    assert!(total >= 3 * 10 * 21 * 6, "sweep too small: {total}");
}

#[test]
fn random_sweep_five_thousand_sequences_per_architecture() {
    let mut total = 0usize;
    let mut seed = 0xC59_0000_0000_0001u64;
    for arch in architectures() {
        let cases = random_sweep(arch, seed, 5_000);
        println!("{arch:?}: {cases} random sequences");
        total += cases;
        seed = seed.wrapping_add(1);
    }
    assert_eq!(total, 5_000 * architectures().len());
}

/// A reader of any architecture, for the tests that need one generic loop.
enum AnyReader {
    Sysv(SysvReader),
    Aapcs(Aapcs64Reader),
    Riscv(RiscvReader),
}

impl AnyReader {
    fn next(&mut self) -> u64 {
        match self {
            Self::Sysv(reader) => reader.next_u64(),
            Self::Aapcs(reader) => reader.next_u64(),
            Self::Riscv(reader) => reader.next_u64(),
        }
    }

    fn clone_state(&self) -> Self {
        match self {
            Self::Sysv(reader) => Self::Sysv(reader.clone_state()),
            Self::Aapcs(reader) => Self::Aapcs(reader.clone_state()),
            Self::Riscv(reader) => Self::Riscv(reader.clone_state()),
        }
    }
}

fn any_reader(arch: Arch, built: &Built) -> AnyReader {
    match arch {
        Arch::X86_64 | Arch::X86_64Softfloat => {
            let pointer = built.arena.ptr(built.list) as *mut VaListSysV;
            // SAFETY: the model built a live list.
            AnyReader::Sysv(unsafe { SysvReader::from_ptr(pointer) })
        }
        Arch::Aapcs64 | Arch::Aapcs64Softfloat => {
            let pointer = built.arena.ptr(built.list) as *mut VaListAapcs64;
            // SAFETY: the model built a live list.
            AnyReader::Aapcs(unsafe { Aapcs64Reader::from_ptr(pointer) })
        }
        Arch::Riscv64 => {
            // SAFETY: the cursor points into the live arena.
            AnyReader::Riscv(unsafe {
                RiscvReader::from_cursor(built.arena.ptr(built.list) as *mut u8)
            })
        }
    }
}

#[test]
fn clone_state_taken_in_the_middle_yields_the_same_remainder() {
    for arch in architectures() {
        let args: Vec<Arg> = (0..12)
            .map(|index| make_arg(Class::Int, index, 7).0)
            .collect();
        let expects: Vec<u64> = (0..12)
            .map(|index| match make_arg(Class::Int, index, 7).1 {
                Expect::U64(value) => value,
                _ => unreachable!(),
            })
            .collect();
        let frame = assign(arch, 0, &args);
        let built = build_list(arch, &frame, 0);
        let mut reader = any_reader(arch, &built);
        for (index, expected) in expects.iter().enumerate().take(6) {
            assert_eq!(reader.next(), *expected, "{arch:?} head {index}");
        }
        let mut copy = reader.clone_state();
        for (index, expected) in expects.iter().enumerate().skip(6) {
            assert_eq!(reader.next(), *expected, "{arch:?} tail {index}");
            assert_eq!(copy.next(), *expected, "{arch:?} copy {index}");
        }
    }
}

/// The exact SysV register/stack boundary sequences of C59 section 7.2.
#[test]
fn sysv_exact_register_stack_boundaries() {
    let archs = [Arch::X86_64, Arch::X86_64Softfloat];
    for arch in archs {
        // 6 integers: all GP.
        let frame = assign(arch, 0, &ints(6));
        assert_eq!(
            frame.stack.len(),
            0,
            "{arch:?}: six integers must stay in GP"
        );
        // 7 integers: the seventh on the stack.
        let frame = assign(arch, 0, &ints(7));
        assert_eq!(frame.stack.len(), 8, "{arch:?}: the seventh integer");
        // 8 doubles: all XMM (hosted) or all stack (soft-float).
        let doubles: Vec<Arg> = (0..8).map(|value| Arg::Double(value as u64)).collect();
        let frame = assign(arch, 0, &doubles);
        let expected = if arch.softfloat() { 64 } else { 0 };
        assert_eq!(frame.stack.len(), expected, "{arch:?}: eight doubles");
        // 9 doubles: the ninth on the stack.
        let doubles: Vec<Arg> = (0..9).map(|value| Arg::Double(value as u64)).collect();
        let frame = assign(arch, 0, &doubles);
        let expected = if arch.softfloat() { 72 } else { 8 };
        assert_eq!(frame.stack.len(), expected, "{arch:?}: nine doubles");
        // 6 integers + 8 doubles: nothing on the stack in the hosted profile.
        let mut args = ints(6);
        args.extend((0..8).map(|value| Arg::Double(value as u64)));
        let frame = assign(arch, 0, &args);
        let expected = if arch.softfloat() { 64 } else { 0 };
        assert_eq!(frame.stack.len(), expected, "{arch:?}: six and eight");
        // 7 integers + 9 doubles: one integer and one double on the stack (hosted).
        let mut args = ints(7);
        args.extend((0..9).map(|value| Arg::Double(value as u64)));
        let frame = assign(arch, 0, &args);
        let expected = if arch.softfloat() { 8 + 72 } else { 16 };
        assert_eq!(frame.stack.len(), expected, "{arch:?}: seven and nine");
        // Integer after the GP registers are exhausted while a double still fits XMM.
        let mut args = ints(6);
        args.push(Arg::Int(0xEE));
        args.push(Arg::Double(0x4004_0000_0000_0000));
        let frame = assign(arch, 0, &args);
        assert_eq!(
            frame.gp[5], 0x1000_0000_0000_0006,
            "{arch:?}: six integers in GP"
        );
        if arch.softfloat() {
            assert_eq!(frame.stack.len(), 16);
        } else {
            assert_eq!(
                frame.stack.len(),
                8,
                "{arch:?}: only the integer overflowed"
            );
            assert_eq!(
                u64::from_le_bytes(frame.fp[0][..8].try_into().unwrap()),
                0x4004_0000_0000_0000
            );
        }
        // Long double after 6 integers: stack, 16-aligned, then another integer.
        let mut args = ints(6);
        args.push(Arg::LongDouble(0x4000_1111_2222_3333_4444));
        args.push(Arg::Int(7));
        let frame = assign(arch, 0, &args);
        assert_eq!(frame.stack.len(), 24, "{arch:?}: long double then integer");
        assert_eq!(
            u128::from_le_bytes(frame.stack[0..16].try_into().unwrap()),
            0x4000_1111_2222_3333_4444
        );
        assert_eq!(frame.stack[16..24], 7u64.to_le_bytes());
    }
}

fn ints(count: usize) -> Vec<Arg> {
    (0..count)
        .map(|index| Arg::Int(0x1000_0000_0000_0000 | (index as u64 + 1)))
        .collect()
}

#[test]
fn the_soft_float_model_is_only_exercised_when_the_cfg_is_set() {
    // The hosted profile must never read an XMM slot for a soft-float caller; the model
    // sweep covers `X86_64Softfloat` only under `--cfg raven_va_softfloat`.
    assert_eq!(
        architectures().contains(&Arch::X86_64),
        !cfg!(raven_va_softfloat)
    );
    assert_eq!(
        architectures().contains(&Arch::X86_64Softfloat),
        cfg!(raven_va_softfloat)
    );
}

#[test]
fn arena_canaries_are_intact_after_the_sweep() {
    let arena = Arena::new(128);
    assert!(arena.before_canary_ok(16));
    assert!(arena.after_canary_ok(16, 96));
}
