// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-builder based tests for the parser and the load planner.

use super::*;
use std::vec;
use std::vec::Vec;

const USER_MAX: u64 = 0x7fff_ffff_ffff;
const BASE: u64 = 0x0040_0000;
const PAGE: u64 = 4096;

#[derive(Clone)]
struct Spec {
    p_type: u32,
    flags: u32,
    vaddr: u64,
    offset: Option<u64>,
    filesz: Option<u64>,
    memsz: Option<u64>,
    align: u64,
    data: Vec<u8>,
}

impl Spec {
    fn load(vaddr: u64, data: &[u8], memsz: u64, flags: u32) -> Self {
        Self {
            p_type: PT_LOAD,
            flags,
            vaddr,
            offset: None,
            filesz: None,
            memsz: Some(memsz),
            align: PAGE,
            data: data.to_vec(),
        }
    }

    fn load_at(vaddr: u64, offset: u64, data: &[u8], memsz: u64, flags: u32) -> Self {
        Self {
            p_type: PT_LOAD,
            flags,
            vaddr,
            offset: Some(offset),
            filesz: None,
            memsz: Some(memsz),
            align: PAGE,
            data: data.to_vec(),
        }
    }

    fn raw(p_type: u32, vaddr: u64, data: &[u8]) -> Self {
        Self {
            p_type,
            flags: 0,
            vaddr,
            offset: None,
            filesz: None,
            memsz: None,
            align: 1,
            data: data.to_vec(),
        }
    }
}

struct Builder {
    class: u8,
    encoding: u8,
    e_type: u16,
    machine: u16,
    entry: Option<u64>,
    ehsize: u16,
    phentsize: u16,
    phoff: u64,
    specs: Vec<Spec>,
}

impl Builder {
    fn new(machine: u16, e_type: u16) -> Self {
        Self {
            class: 2,
            encoding: 1,
            e_type,
            machine,
            entry: None,
            ehsize: 64,
            phentsize: 56,
            phoff: 64,
            specs: Vec::new(),
        }
    }

    fn entry(mut self, entry: u64) -> Self {
        self.entry = Some(entry);
        self
    }

    fn class(mut self, class: u8) -> Self {
        self.class = class;
        self
    }

    fn encoding(mut self, encoding: u8) -> Self {
        self.encoding = encoding;
        self
    }

    fn e_type(mut self, e_type: u16) -> Self {
        self.e_type = e_type;
        self
    }

    fn ehsize(mut self, ehsize: u16) -> Self {
        self.ehsize = ehsize;
        self
    }

    fn phentsize(mut self, phentsize: u16) -> Self {
        self.phentsize = phentsize;
        self
    }

    fn phoff(mut self, phoff: u64) -> Self {
        self.phoff = phoff;
        self
    }

    fn add(mut self, spec: Spec) -> Self {
        self.specs.push(spec);
        self
    }

    fn build(&self) -> Vec<u8> {
        let count = self.specs.len();
        let table_end = 64usize + count * usize::from(self.phentsize);
        let mut offsets = Vec::with_capacity(count);
        let mut cursor = 4096usize;
        for spec in &self.specs {
            let offset = spec.offset.unwrap_or(cursor as u64);
            offsets.push(offset);
            if spec.offset.is_none()
                && let Ok(start) = usize::try_from(offset)
            {
                cursor = align_up(start.saturating_add(spec.data.len()), 4096);
            }
        }

        let mut file_len = 4096usize.max(table_end);
        for (spec, offset) in self.specs.iter().zip(&offsets) {
            if let Some(end) = offset
                .checked_add(spec.data.len() as u64)
                .and_then(|value| usize::try_from(value).ok())
                && end <= (1 << 20)
            {
                file_len = file_len.max(end);
            }
        }
        let mut file = vec![0u8; file_len];
        for (spec, offset) in self.specs.iter().zip(&offsets) {
            if let Some(end) = offset
                .checked_add(spec.data.len() as u64)
                .and_then(|value| usize::try_from(value).ok())
                && let Ok(start) = usize::try_from(*offset)
                && end <= file.len()
            {
                file[start..end].copy_from_slice(&spec.data);
            }
        }

        for (index, spec) in self.specs.iter().enumerate() {
            let Some(base) = usize::try_from(self.phoff)
                .ok()
                .and_then(|start| start.checked_add(index * usize::from(self.phentsize)))
            else {
                continue;
            };
            let Some(end) = base.checked_add(56) else {
                continue;
            };
            if end > file.len() {
                continue;
            }
            put_u32(&mut file, base, spec.p_type);
            put_u32(&mut file, base + 4, spec.flags);
            put_u64(&mut file, base + 8, offsets[index]);
            put_u64(&mut file, base + 16, spec.vaddr);
            put_u64(&mut file, base + 24, 0);
            let filesz = spec.filesz.unwrap_or(spec.data.len() as u64);
            let memsz = spec.memsz.unwrap_or(filesz);
            put_u64(&mut file, base + 32, filesz);
            put_u64(&mut file, base + 40, memsz);
            put_u64(&mut file, base + 48, spec.align);
        }

        let entry = self.entry.unwrap_or_else(|| {
            self.specs
                .iter()
                .find(|spec| spec.p_type == PT_LOAD)
                .map_or(0, |spec| spec.vaddr)
        });
        let mut header = [0u8; 64];
        header[0..4].copy_from_slice(b"\x7fELF");
        header[4] = self.class;
        header[5] = self.encoding;
        header[6] = 1;
        header[16..18].copy_from_slice(&self.e_type.to_le_bytes());
        header[18..20].copy_from_slice(&self.machine.to_le_bytes());
        header[20..24].copy_from_slice(&1u32.to_le_bytes());
        header[24..32].copy_from_slice(&entry.to_le_bytes());
        header[32..40].copy_from_slice(&self.phoff.to_le_bytes());
        header[52..54].copy_from_slice(&self.ehsize.to_le_bytes());
        header[54..56].copy_from_slice(&self.phentsize.to_le_bytes());
        header[56..58].copy_from_slice(&(count as u16).to_le_bytes());
        file[0..64].copy_from_slice(&header);
        file
    }
}

fn put_u32(file: &mut [u8], offset: usize, value: u32) {
    file[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(file: &mut [u8], offset: usize, value: u64) {
    file[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn align_up(value: usize, alignment: usize) -> usize {
    let remainder = value % alignment;
    if remainder == 0 {
        value
    } else {
        value + (alignment - remainder)
    }
}

fn parse_err(builder: &Builder, machine: Machine) -> ElfError {
    let bytes = builder.build();
    Elf::parse(&bytes, machine, 0..USER_MAX).unwrap_err()
}

fn parse_err_in(
    builder: &Builder,
    machine: Machine,
    user_range: core::ops::Range<u64>,
) -> ElfError {
    let bytes = builder.build();
    Elf::parse(&bytes, machine, user_range).unwrap_err()
}

fn basic(machine: u16) -> Builder {
    basic_typed(machine, ET_EXEC)
}

fn basic_typed(machine: u16, e_type: u16) -> Builder {
    Builder::new(machine, e_type)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
}

fn data_binary(data_filesz: usize) -> Vec<u8> {
    Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(
            BASE,
            0x1000,
            &vec![0xAA; 0x2000],
            0x2000,
            PF_R | PF_X,
        ))
        .add(Spec::load_at(
            BASE + 0x2000,
            0x3000,
            &vec![0xBB; 0x1000],
            0x1000,
            PF_R,
        ))
        .add(Spec::load_at(
            BASE + 0x3000,
            0x4000,
            &vec![0xCC; data_filesz],
            0x3000,
            PF_R | PF_W,
        ))
        .build()
}

fn plan_steps(bytes: &[u8], mode: LoadMode) -> Vec<MapStep> {
    let elf = Elf::parse(bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    plan_load(&elf, PAGE, mode).unwrap().collect()
}

#[test]
fn parses_valid_exec_x86_64() {
    let payload = vec![0xAAu8; 0x2000];
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &payload, 0x2000, PF_R | PF_X))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.phnum(), 1);
    assert_eq!(elf.interp(), None);
    assert_eq!(elf.phdr_vaddr(), None);
    let segments: Vec<_> = elf.segments().collect();
    assert_eq!(segments.len(), 1);
    let segment = segments[0];
    assert_eq!(segment.vaddr, BASE);
    assert_eq!(segment.memsz, 0x2000);
    assert_eq!(segment.filesz, 0x2000);
    assert_eq!(segment.offset, 0x1000);
    assert_eq!(segment.prot, 1 | 4);
    assert_eq!(elf.segment_data(&segment), &payload[..]);
}

#[test]
fn parses_valid_dyn_x86_64() {
    let bytes = basic_typed(EM_X86_64, ET_DYN).build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.segments().count(), 1);
}

#[test]
fn parses_valid_exec_aarch64() {
    let bytes = basic_typed(EM_AARCH64, ET_EXEC).build();
    let elf = Elf::parse(&bytes, Machine::Aarch64, 0..USER_MAX).unwrap();
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.segments().count(), 1);
}

#[test]
fn parses_valid_dyn_aarch64() {
    let bytes = basic_typed(EM_AARCH64, ET_DYN).build();
    let elf = Elf::parse(&bytes, Machine::Aarch64, 0..USER_MAX).unwrap();
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.segments().count(), 1);
}

#[test]
fn parses_valid_exec_riscv64() {
    let bytes = basic_typed(EM_RISCV, ET_EXEC).build();
    let elf = Elf::parse(&bytes, Machine::Riscv64, 0..USER_MAX).unwrap();
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.segments().count(), 1);
}

#[test]
fn parses_valid_dyn_riscv64() {
    let bytes = basic_typed(EM_RISCV, ET_DYN).build();
    let elf = Elf::parse(&bytes, Machine::Riscv64, 0..USER_MAX).unwrap();
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.segments().count(), 1);
}

#[test]
fn machine_mismatch_is_rejected() {
    let bytes = basic(EM_X86_64).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::Aarch64, 0..USER_MAX).unwrap_err(),
        ElfError::BadMachine
    );
    let bytes = basic(0).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::BadMachine
    );
}

#[test]
fn too_short_is_rejected() {
    assert_eq!(
        Elf::parse(&[], Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::TooShort
    );
    assert_eq!(
        Elf::parse(&[0u8; 63], Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::TooShort
    );
}

#[test]
fn bad_magic_is_rejected() {
    let bytes = vec![0u8; 128];
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::BadMagic
    );
}

#[test]
fn not_elf64_is_rejected() {
    let bytes = basic(EM_X86_64).class(1).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::NotElf64
    );
}

#[test]
fn big_endian_is_rejected() {
    let bytes = basic(EM_X86_64).encoding(2).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::NotLittleEndian
    );
}

#[test]
fn bad_type_is_rejected() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC).e_type(1).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::BadType
    );
}

#[test]
fn bad_ehsize_is_rejected() {
    let bytes = basic(EM_X86_64).ehsize(63).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::BadPhdr
    );
}

#[test]
fn bad_phentsize_is_rejected() {
    let bytes = basic(EM_X86_64).phentsize(55).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::BadPhdr
    );
}

#[test]
fn phnum_zero_is_rejected() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC).build();
    assert_eq!(
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap_err(),
        ElfError::BadPhdr
    );
}

#[test]
fn phnum_65_is_rejected() {
    let mut builder = Builder::new(EM_X86_64, ET_EXEC);
    for _ in 0..65 {
        builder = builder.add(Spec::raw(0, 0, &[]));
    }
    assert_eq!(
        parse_err(&builder, Machine::X86_64),
        ElfError::TooManySegments
    );
}

#[test]
fn phdr_table_out_of_bounds_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .phoff(4090)
        .add(Spec::raw(0, 0, &[]));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn too_many_load_segments_is_rejected() {
    let mut builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE);
    for index in 0..17u64 {
        builder = builder.add(Spec::load(
            BASE + index * PAGE,
            &[0x11; 16],
            16,
            PF_R | PF_X,
        ));
    }
    assert_eq!(
        parse_err(&builder, Machine::X86_64),
        ElfError::TooManySegments
    );
}

#[test]
fn filesz_greater_than_memsz_is_rejected() {
    let mut spec = Spec::load(BASE, &[0xAA; 0x100], 0x80, PF_R | PF_X);
    spec.filesz = Some(0x100);
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(spec);
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn segment_file_range_past_eof_is_rejected() {
    let mut spec = Spec::load(BASE, &[0xAA; 0x1000], 0x2000, PF_R | PF_X);
    spec.filesz = Some(0x2000);
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(spec);
    assert_eq!(
        parse_err(&builder, Machine::X86_64),
        ElfError::SegmentOutOfBounds
    );
}

#[test]
fn segment_file_offset_overflow_is_rejected() {
    let mut spec = Spec::load(BASE, &[], 4, PF_R | PF_X);
    spec.offset = Some(u64::MAX - 1);
    spec.filesz = Some(4);
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(spec);
    assert_eq!(
        parse_err(&builder, Machine::X86_64),
        ElfError::SegmentOutOfBounds
    );
}

#[test]
fn bad_palign_is_rejected() {
    let mut spec = Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X);
    spec.align = 3;
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(spec);
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn misaligned_congruence_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(BASE, 0x1001, &[0xAA; 16], 16, PF_R | PF_X));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn writable_and_executable_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(Spec::load(
        BASE,
        &[0xAA; 16],
        16,
        PF_R | PF_W | PF_X,
    ));
    assert_eq!(
        parse_err(&builder, Machine::X86_64),
        ElfError::WritableAndExecutable
    );
}

#[test]
fn segment_below_user_range_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(0x1000)
        .add(Spec::load(0x1000, &[0xAA; 16], 16, PF_R | PF_X));
    assert_eq!(
        parse_err_in(&builder, Machine::X86_64, BASE..0x800000),
        ElfError::SegmentOutsideUserRange
    );
}

#[test]
fn segment_above_user_range_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE + 0x10_0000)
        .add(Spec::load(BASE + 0x10_0000, &[0xAA; 16], 16, PF_R | PF_X));
    assert_eq!(
        parse_err_in(&builder, Machine::X86_64, BASE..BASE + 0x10_0000),
        ElfError::SegmentOutsideUserRange
    );
}

#[test]
fn segment_straddling_user_range_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(Spec::load(
        BASE + 0x1000,
        &[0xAA; 16],
        0x2000,
        PF_R | PF_X,
    ));
    assert_eq!(
        parse_err_in(&builder, Machine::X86_64, BASE..BASE + 0x2000),
        ElfError::SegmentOutsideUserRange
    );
}

#[test]
fn segment_vaddr_overflow_is_rejected() {
    let vaddr = u64::MAX - 0x10;
    let mut spec = Spec::load_at(vaddr, vaddr % PAGE, &[0xAA; 16], 0x100, PF_R | PF_X);
    spec.filesz = Some(16);
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(vaddr).add(spec);
    assert_eq!(
        parse_err_in(&builder, Machine::X86_64, 0..u64::MAX),
        ElfError::SegmentOutsideUserRange
    );
}

#[test]
fn overlapping_segments_after_page_rounding_are_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(BASE, 0x1000, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::load_at(BASE + 0x800, 0x1800, &[0xBB; 16], 16, PF_R));
    assert_eq!(
        parse_err(&builder, Machine::X86_64),
        ElfError::SegmentOverlap
    );
}

#[test]
fn adjacent_segments_are_accepted() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(BASE, 0x1000, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::load_at(BASE + 0x1000, 0x2000, &[0xBB; 16], 16, PF_R))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.segments().count(), 2);
}

#[test]
fn entry_outside_segments_is_rejected() {
    let builder = basic(EM_X86_64).entry(BASE + 0x8000);
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn entry_in_non_executable_segment_is_rejected() {
    let builder =
        Builder::new(EM_X86_64, ET_EXEC)
            .entry(BASE)
            .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn entry_at_segment_end_is_rejected() {
    let builder = basic(EM_X86_64).entry(BASE + 16);
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn interp_is_parsed() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, b"/lib/ld.so\0"))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.interp(), Some("/lib/ld.so"));
}

#[test]
fn interp_without_nul_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, b"/lib/ld.so"));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn interp_non_utf8_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, &[0xFF, 0]));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn interp_empty_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, &[0]));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn interp_huge_is_rejected() {
    let mut path = vec![b'a'; 5000];
    path.push(0);
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, &path));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn interp_max_length_is_accepted() {
    let mut path = vec![b'a'; 4096];
    path.push(0);
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, &path))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.interp().map(str::len), Some(4096));
}

#[test]
fn interp_out_of_bounds_is_rejected() {
    let mut spec = Spec::raw(PT_INTERP, 0, &[b'x', 0]);
    spec.offset = Some(5000);
    spec.filesz = Some(0x1000);
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(spec);
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn duplicate_interp_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_INTERP, 0, b"/a\0"))
        .add(Spec::raw(PT_INTERP, 0, b"/b\0"));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn phdr_vaddr_from_pt_phdr() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_PHDR, BASE + 0x40, &[]))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.phdr_vaddr(), Some(BASE + 0x40));
}

#[test]
fn phdr_vaddr_computed_from_containing_segment() {
    let mut spec = Spec::load(BASE, &[], 0x1000, PF_R | PF_X);
    spec.offset = Some(0);
    spec.filesz = Some(0x1000);
    let builder = Builder::new(EM_X86_64, ET_EXEC).entry(BASE).add(spec);
    let bytes = builder.build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.phdr_vaddr(), Some(BASE + 0x40));
}

#[test]
fn phdr_vaddr_is_none_without_pt_phdr_or_containing_segment() {
    let bytes = basic(EM_X86_64).build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.phdr_vaddr(), None);
}

#[test]
fn duplicate_pt_phdr_is_rejected() {
    let builder = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load(BASE, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::raw(PT_PHDR, BASE + 0x40, &[]))
        .add(Spec::raw(PT_PHDR, BASE + 0x80, &[]));
    assert_eq!(parse_err(&builder, Machine::X86_64), ElfError::BadPhdr);
}

#[test]
fn truncation_at_every_byte_offset_is_rejected() {
    let bytes = data_binary(0x1234);
    assert!(Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).is_ok());
    for end in 0..bytes.len() {
        let result = Elf::parse(&bytes[..end], Machine::X86_64, 0..USER_MAX);
        assert!(result.is_err(), "truncated at {end} bytes parsed as Ok");
    }
}

#[test]
fn random_mutations_never_panic() {
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            let mut value = self.0;
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            self.0 = value;
            value
        }
    }

    let valid = data_binary(0x1234);
    let range_start = BASE;
    let range_end = BASE + 0x1_0000;
    let mut rng = XorShift(0x9e37_79b9_7f4a_7c15);
    for _ in 0..10_000 {
        let mut bytes = valid.clone();
        let mutations = (rng.next() % 4 + 1) as usize;
        for _ in 0..mutations {
            let index = (rng.next() as usize) % bytes.len();
            bytes[index] ^= (rng.next() as u8) | 1;
        }
        if let Ok(elf) = Elf::parse(&bytes, Machine::X86_64, range_start..range_end) {
            for mode in [LoadMode::Strict, LoadMode::Copy] {
                if let Ok(steps) = plan_load(&elf, PAGE, mode) {
                    let mut previous_end = range_start;
                    let mut step_count = 0usize;
                    for step in steps {
                        assert_eq!(step.vaddr % PAGE, 0);
                        assert!(step.len > 0);
                        assert_eq!(step.len % PAGE, 0);
                        assert!(step.vaddr >= range_start, "step below user range");
                        assert!(step.vaddr >= previous_end, "steps overlap");
                        let end = step
                            .vaddr
                            .checked_add(step.len)
                            .expect("step end must not overflow");
                        assert!(end <= range_end, "step outside user range");
                        previous_end = end;
                        step_count += 1;
                    }
                    if step_count == 0 {
                        assert!(elf.segments().all(|segment| segment.memsz == 0));
                    }
                }
            }
        }
    }
}

#[test]
fn plan_load_copy_exact_steps_for_unaligned_tail() {
    let bytes = data_binary(0x1234);
    let steps = plan_steps(&bytes, LoadMode::Copy);
    for step in &steps {
        println!(
            "vaddr={:#x} len={:#x} prot={} kind={:?}",
            step.vaddr, step.len, step.prot, step.kind
        );
    }
    assert_eq!(
        steps,
        vec![
            MapStep {
                vaddr: BASE,
                len: PAGE,
                prot: 1 | 4,
                kind: StepKind::File { offset: 0x1000 },
            },
            MapStep {
                vaddr: BASE + 0x1000,
                len: PAGE,
                prot: 1 | 4,
                kind: StepKind::File { offset: 0x2000 },
            },
            MapStep {
                vaddr: BASE + 0x2000,
                len: PAGE,
                prot: 1,
                kind: StepKind::File { offset: 0x3000 },
            },
            MapStep {
                vaddr: BASE + 0x3000,
                len: PAGE,
                prot: 1 | 2,
                kind: StepKind::File { offset: 0x4000 },
            },
            MapStep {
                vaddr: BASE + 0x4000,
                len: PAGE,
                prot: 1 | 2,
                kind: StepKind::CopyTail {
                    offset: 0x5000,
                    len: 0x234,
                },
            },
            MapStep {
                vaddr: BASE + 0x5000,
                len: PAGE,
                prot: 1 | 2,
                kind: StepKind::Zero,
            },
        ]
    );
}

#[test]
fn plan_load_strict_rejects_unaligned_tail() {
    let bytes = data_binary(0x1234);
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    match plan_load(&elf, PAGE, LoadMode::Strict) {
        Err(error) => assert_eq!(error, ElfError::FileTailNotPageAligned),
        Ok(_) => panic!("strict planning of an unaligned tail must fail"),
    }
}

#[test]
fn plan_load_strict_aligned_exact_steps() {
    let bytes = data_binary(0x2000);
    let expected = vec![
        MapStep {
            vaddr: BASE,
            len: PAGE,
            prot: 1 | 4,
            kind: StepKind::File { offset: 0x1000 },
        },
        MapStep {
            vaddr: BASE + 0x1000,
            len: PAGE,
            prot: 1 | 4,
            kind: StepKind::File { offset: 0x2000 },
        },
        MapStep {
            vaddr: BASE + 0x2000,
            len: PAGE,
            prot: 1,
            kind: StepKind::File { offset: 0x3000 },
        },
        MapStep {
            vaddr: BASE + 0x3000,
            len: PAGE,
            prot: 1 | 2,
            kind: StepKind::File { offset: 0x4000 },
        },
        MapStep {
            vaddr: BASE + 0x4000,
            len: PAGE,
            prot: 1 | 2,
            kind: StepKind::File { offset: 0x5000 },
        },
        MapStep {
            vaddr: BASE + 0x5000,
            len: PAGE,
            prot: 1 | 2,
            kind: StepKind::Zero,
        },
    ];
    assert_eq!(plan_steps(&bytes, LoadMode::Strict), expected);
    assert_eq!(plan_steps(&bytes, LoadMode::Copy), expected);
}

#[test]
fn plan_load_handles_page_offset_vaddr() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE + 3)
        .add(Spec::load_at(
            BASE + 3,
            0x1003,
            &vec![0xDD; 0x2FFD],
            0x2FFD,
            PF_R | PF_X,
        ))
        .build();
    let steps = plan_steps(&bytes, LoadMode::Strict);
    assert_eq!(
        steps,
        vec![
            MapStep {
                vaddr: BASE,
                len: PAGE,
                prot: 1 | 4,
                kind: StepKind::File { offset: 0x1000 },
            },
            MapStep {
                vaddr: BASE + 0x1000,
                len: PAGE,
                prot: 1 | 4,
                kind: StepKind::File { offset: 0x2000 },
            },
            MapStep {
                vaddr: BASE + 0x2000,
                len: PAGE,
                prot: 1 | 4,
                kind: StepKind::File { offset: 0x3000 },
            },
        ]
    );
}

#[test]
fn plan_load_bss_only_segment_is_zeroed() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(
            BASE,
            0x1000,
            &vec![0xAA; 0x1000],
            0x1000,
            PF_R | PF_X,
        ))
        .add(Spec::load_at(BASE + 0x10003, 0x3, &[], 0x2000, PF_R | PF_W))
        .build();
    let steps = plan_steps(&bytes, LoadMode::Strict);
    assert_eq!(
        steps,
        vec![
            MapStep {
                vaddr: BASE,
                len: PAGE,
                prot: 1 | 4,
                kind: StepKind::File { offset: 0x1000 },
            },
            MapStep {
                vaddr: BASE + 0x10000,
                len: PAGE,
                prot: 1 | 2,
                kind: StepKind::Zero,
            },
            MapStep {
                vaddr: BASE + 0x11000,
                len: PAGE,
                prot: 1 | 2,
                kind: StepKind::Zero,
            },
            MapStep {
                vaddr: BASE + 0x12000,
                len: PAGE,
                prot: 1 | 2,
                kind: StepKind::Zero,
            },
        ]
    );
}

#[test]
fn plan_load_rejects_zero_page() {
    let bytes = data_binary(0x1234);
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    match plan_load(&elf, 0, LoadMode::Copy) {
        Err(error) => assert_eq!(error, ElfError::BadPhdr),
        Ok(_) => panic!("page size zero must be rejected"),
    }
}

#[test]
fn plan_load_rejects_overlap_at_a_larger_page() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(BASE, 0x1000, &[0xAA; 16], 16, PF_R | PF_X))
        .add(Spec::load_at(BASE + 0x1000, 0x2000, &[0xBB; 16], 16, PF_R))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    match plan_load(&elf, 8192, LoadMode::Copy) {
        Err(error) => assert_eq!(error, ElfError::SegmentOverlap),
        Ok(_) => panic!("segments sharing a larger page must be rejected"),
    }
}

#[test]
fn segments_are_sorted_by_vaddr() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE + 0x2000)
        .add(Spec::load_at(
            BASE + 0x2000,
            0x1000,
            &[0xAA; 16],
            16,
            PF_R | PF_X,
        ))
        .add(Spec::load_at(BASE, 0x2000, &[0xBB; 16], 16, PF_R))
        .add(Spec::load_at(BASE + 0x1000, 0x3000, &[0xCC; 16], 16, PF_R))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    let vaddrs: Vec<_> = elf.segments().map(|segment| segment.vaddr).collect();
    assert_eq!(vaddrs, vec![BASE, BASE + 0x1000, BASE + 0x2000]);
}

#[test]
fn segment_data_matches_file_bytes() {
    let bytes = data_binary(0x1234);
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    let segments: Vec<_> = elf.segments().collect();
    assert_eq!(elf.segment_data(&segments[0]), &vec![0xAAu8; 0x2000][..]);
    assert_eq!(elf.segment_data(&segments[1]), &vec![0xBBu8; 0x1000][..]);
    assert_eq!(elf.segment_data(&segments[2]), &vec![0xCCu8; 0x1234][..]);
}

#[test]
fn zero_memsz_segment_has_no_steps() {
    let bytes = Builder::new(EM_X86_64, ET_EXEC)
        .entry(BASE)
        .add(Spec::load_at(
            BASE,
            0x1000,
            &vec![0xAA; 0x1000],
            0x1000,
            PF_R | PF_X,
        ))
        .add(Spec::load_at(BASE + 0x10_0000, 0x0, &[], 0, PF_R))
        .build();
    let elf = Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).unwrap();
    assert_eq!(elf.segments().count(), 2);
    let steps = plan_steps(&bytes, LoadMode::Copy);
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].vaddr, BASE);
}

#[test]
fn real_world_bin_ls_parses_and_plans() {
    let Ok(bytes) = std::fs::read("/bin/ls") else {
        return;
    };
    if bytes.get(18..20) != Some(&EM_X86_64.to_le_bytes()[..]) {
        return;
    }
    if bytes.get(16..18) != Some(&ET_DYN.to_le_bytes()[..]) {
        return;
    }
    let elf =
        Elf::parse(&bytes, Machine::X86_64, 0..USER_MAX).expect("/bin/ls must parse as x86-64 ELF");
    let steps = plan_load(&elf, PAGE, LoadMode::Copy).expect("/bin/ls must be loadable");
    let mut previous_end = 0u64;
    let mut count = 0usize;
    for step in steps {
        assert_eq!(step.vaddr % PAGE, 0);
        assert_eq!(step.len % PAGE, 0);
        assert!(step.vaddr >= previous_end);
        previous_end = step.vaddr.checked_add(step.len).unwrap();
        count += 1;
    }
    assert!(count > 0);
}
