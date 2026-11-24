// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CONTRACTS C9 conformance: one negative case per strict ELF64 rule, the `Strict`/`Copy`
//! load plans, and a page-rounding property model.
//!
//! The byte builder here is independent of the crate's unit-test builder: it writes only the
//! fields C9 names, so a parser regression cannot be hidden by a shared helper.

use raven_elf::{Elf, ElfError, LoadMode, Machine, MapStep, StepKind, plan_load};

const PAGE: u64 = 4096;
const BASE: u64 = 0x0040_0000;
const USER_START: u64 = 0x1000;
const USER_END: u64 = 0x7fff_ffff_f000;

const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

#[derive(Clone, Copy, Debug)]
struct Seg {
    vaddr: u64,
    memsz: u64,
    filesz: u64,
    flags: u32,
}

impl Seg {
    const fn load(vaddr: u64, filesz: u64, memsz: u64, flags: u32) -> Self {
        Self {
            vaddr,
            memsz,
            filesz,
            flags,
        }
    }
}

/// Build a little-endian ELF64 with one `PT_LOAD` per `Seg`.
///
/// Each segment's file data gets its own offset congruent to `vaddr` modulo the page size, so
/// only the property under test can reject the image.
fn build(segs: &[Seg], entry: u64) -> Vec<u8> {
    let phnum = u16::try_from(segs.len()).expect("test segment count");
    let table_end = 64usize + usize::from(phnum) * 56;
    let mut offsets = Vec::with_capacity(segs.len());
    let mut file_len = table_end.max(0x2000);
    for (index, segment) in segs.iter().enumerate() {
        let offset = 0x2000 + (index as u64) * 0x2000 + (segment.vaddr % PAGE);
        offsets.push(offset);
        let end = usize::try_from(offset + segment.filesz).expect("test file size");
        file_len = file_len.max(end);
    }
    let mut file = vec![0u8; file_len];
    file[0..4].copy_from_slice(b"\x7fELF");
    file[4] = 2; // ELFCLASS64
    file[5] = 1; // ELFDATA2LSB
    file[6] = 1; // EV_CURRENT
    file[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    file[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    file[24..32].copy_from_slice(&entry.to_le_bytes());
    file[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    file[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    file[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    file[56..58].copy_from_slice(&phnum.to_le_bytes());
    for (index, (segment, offset)) in segs.iter().zip(offsets).enumerate() {
        let base = 64 + index * 56;
        file[base..base + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
        file[base + 4..base + 8].copy_from_slice(&segment.flags.to_le_bytes());
        file[base + 8..base + 16].copy_from_slice(&offset.to_le_bytes());
        file[base + 16..base + 24].copy_from_slice(&segment.vaddr.to_le_bytes());
        file[base + 24..base + 32].copy_from_slice(&segment.vaddr.to_le_bytes());
        file[base + 32..base + 40].copy_from_slice(&segment.filesz.to_le_bytes());
        file[base + 40..base + 48].copy_from_slice(&segment.memsz.to_le_bytes());
        file[base + 48..base + 56].copy_from_slice(&PAGE.to_le_bytes());
        if segment.filesz != 0 {
            let start = usize::try_from(offset).expect("test offset");
            let end = start + usize::try_from(segment.filesz).expect("test filesz");
            file[start..end].fill(0xa5);
        }
    }
    file
}

/// Parse with the default user range.
fn parse(segs: &[Seg], entry: u64, range: core::ops::Range<u64>) -> Result<(), ElfError> {
    let file = build(segs, entry);
    Elf::parse(&file, Machine::X86_64, range).map(|_| ())
}

fn expect(segs: &[Seg], entry: u64, range: core::ops::Range<u64>, error: ElfError) {
    assert_eq!(parse(segs, entry, range), Err(error), "segments={segs:?}");
}

/// One `PT_LOAD` with raw header fields for the `checked_add` overflow cases.
fn build_raw(offset: u64, vaddr: u64, filesz: u64, memsz: u64, entry: u64) -> Vec<u8> {
    let mut file = vec![0u8; 0x1000];
    file[0..4].copy_from_slice(b"\x7fELF");
    file[4] = 2;
    file[5] = 1;
    file[6] = 1;
    file[16..18].copy_from_slice(&2u16.to_le_bytes());
    file[18..20].copy_from_slice(&62u16.to_le_bytes());
    file[24..32].copy_from_slice(&entry.to_le_bytes());
    file[32..40].copy_from_slice(&64u64.to_le_bytes());
    file[52..54].copy_from_slice(&64u16.to_le_bytes());
    file[54..56].copy_from_slice(&56u16.to_le_bytes());
    file[56..58].copy_from_slice(&1u16.to_le_bytes());
    file[64..68].copy_from_slice(&1u32.to_le_bytes());
    file[68..72].copy_from_slice(&(PF_R | PF_X).to_le_bytes());
    file[72..80].copy_from_slice(&offset.to_le_bytes());
    file[80..88].copy_from_slice(&vaddr.to_le_bytes());
    file[88..96].copy_from_slice(&vaddr.to_le_bytes());
    file[96..104].copy_from_slice(&filesz.to_le_bytes());
    file[104..112].copy_from_slice(&memsz.to_le_bytes());
    file[112..120].copy_from_slice(&PAGE.to_le_bytes());
    file
}

#[test]
fn c9_max_sixteen_load_segments() {
    let mut sixteen = Vec::new();
    for index in 0..16u64 {
        let flags = if index == 0 { PF_R | PF_X } else { PF_R };
        sixteen.push(Seg::load(BASE + index * PAGE, 0, PAGE, flags));
    }
    assert_eq!(
        parse(&sixteen, BASE, USER_START..USER_END),
        Ok(()),
        "16 PT_LOAD segments are the contract limit and must parse"
    );

    let mut seventeen = sixteen.clone();
    seventeen.push(Seg::load(BASE + 16 * PAGE, 0, PAGE, PF_R));
    expect(
        &seventeen,
        BASE,
        USER_START..USER_END,
        ElfError::TooManySegments,
    );
}

#[test]
fn c9_page_rounded_overlap_is_rejected() {
    // Byte ranges are adjacent (0x400000..0x400800 and 0x400800..0x401000) but their page
    // spans overlap at 0x400000..0x401000. C9 rejects after page rounding.
    let overlap = [
        Seg::load(BASE, 0, 0x800, PF_R | PF_X),
        Seg::load(BASE + 0x800, 0, 0x800, PF_R | PF_W),
    ];
    expect(
        &overlap,
        BASE,
        USER_START..USER_END,
        ElfError::SegmentOverlap,
    );

    let adjacent = [
        Seg::load(BASE, 0, PAGE, PF_R | PF_X),
        Seg::load(BASE + PAGE, 0, PAGE, PF_R | PF_W),
    ];
    assert_eq!(parse(&adjacent, BASE, USER_START..USER_END), Ok(()));
}

#[test]
fn c9_wx_is_rejected() {
    expect(
        &[Seg::load(BASE, 0, PAGE, PF_R | PF_W | PF_X)],
        BASE,
        USER_START..USER_END,
        ElfError::WritableAndExecutable,
    );

    let split = [
        Seg::load(BASE, 0, PAGE, PF_R | PF_X),
        Seg::load(BASE + PAGE, 0, PAGE, PF_R | PF_W),
    ];
    assert_eq!(parse(&split, BASE, USER_START..USER_END), Ok(()));
}

#[test]
fn c9_file_offset_checked_add_overflow_is_rejected() {
    // p_offset + p_filesz overflows u64. The header/table are valid and the real image length is
    // u64::MAX, so only the checked addition can reject this file.
    let file = build_raw(u64::MAX - 8, BASE, 16, 16, BASE);
    assert_eq!(
        Elf::parse_with_len(&file, u64::MAX, Machine::X86_64, USER_START..USER_END).err(),
        Some(ElfError::SegmentOutOfBounds)
    );
}

#[test]
fn c9_vaddr_checked_add_overflow_is_rejected() {
    // p_vaddr + p_memsz overflows u64; the user range itself is unbounded above.
    let file = build_raw(0x1000, u64::MAX - 8, 0, 16, u64::MAX - 8);
    assert_eq!(
        Elf::parse_with_len(&file, u64::MAX, Machine::X86_64, 0..u64::MAX).err(),
        Some(ElfError::SegmentOutsideUserRange)
    );
}

#[test]
fn c9_user_range_is_enforced() {
    let range = BASE..BASE + 0x2000;
    expect(
        &[Seg::load(BASE - PAGE, 0, PAGE, PF_R | PF_X)],
        BASE,
        range.clone(),
        ElfError::SegmentOutsideUserRange,
    );
    expect(
        &[Seg::load(BASE + 0x2000, 0, PAGE, PF_R | PF_X)],
        BASE,
        range.clone(),
        ElfError::SegmentOutsideUserRange,
    );
    expect(
        &[Seg::load(BASE + 0x1800, 0, PAGE, PF_R | PF_X)],
        BASE,
        range.clone(),
        ElfError::SegmentOutsideUserRange,
    );
    assert_eq!(
        parse(
            &[Seg::load(BASE, 0, PAGE, PF_R | PF_X)],
            BASE,
            range.clone()
        ),
        Ok(())
    );
}

#[test]
fn c9_entry_must_be_executable() {
    expect(
        &[Seg::load(BASE, 0, PAGE, PF_R | PF_W)],
        BASE,
        USER_START..USER_END,
        ElfError::BadPhdr,
    );
    expect(
        &[Seg::load(BASE, 0, PAGE, PF_R | PF_X)],
        BASE + PAGE,
        USER_START..USER_END,
        ElfError::BadPhdr,
    );
}

#[test]
fn c9_strict_and_copy_plans() {
    // A 0x1234-byte file part in a 0x3000-byte segment: one whole page, one copied tail page and
    // one zero page. Strict cannot fill the tail without writing, so it must reject.
    let image = build(&[Seg::load(BASE, 0x1234, 0x3000, PF_R | PF_X)], BASE);
    let elf = Elf::parse(&image, Machine::X86_64, USER_START..USER_END).expect("valid image");
    let copy: Vec<MapStep> = plan_load(&elf, PAGE, LoadMode::Copy)
        .expect("copy plan")
        .collect();
    assert_eq!(copy.len(), 3);
    assert_eq!(copy[0].vaddr, BASE);
    assert_eq!(copy[0].len, PAGE);
    assert!(matches!(copy[0].kind, StepKind::File { .. }));
    assert_eq!(copy[1].vaddr, BASE + PAGE);
    assert!(matches!(
        copy[1].kind,
        StepKind::CopyTail { len: 0x234, .. }
    ));
    assert_eq!(copy[2].vaddr, BASE + 0x2000);
    assert!(matches!(copy[2].kind, StepKind::Zero));
    assert_eq!(
        plan_load(&elf, PAGE, LoadMode::Strict).map(|_| ()),
        Err(ElfError::FileTailNotPageAligned)
    );

    // A page-aligned file part needs no copy: Strict accepts it and emits only File steps.
    let aligned = build(&[Seg::load(BASE, 0x2000, 0x2000, PF_R | PF_X)], BASE);
    let elf = Elf::parse(&aligned, Machine::X86_64, USER_START..USER_END).expect("valid image");
    let steps: Vec<MapStep> = plan_load(&elf, PAGE, LoadMode::Strict)
        .expect("strict plan")
        .collect();
    assert_eq!(steps.len(), 2);
    assert!(
        steps
            .iter()
            .all(|step| matches!(step.kind, StepKind::File { .. }))
    );
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, max: u64) -> u64 {
        self.next() % max
    }
}

fn floor_page(value: u64) -> u64 {
    value & !(PAGE - 1)
}

fn ceil_page(value: u64) -> u64 {
    floor_page(value.saturating_add(PAGE - 1))
}

/// Independent C9 model for the random images: accept exactly when every rule holds.
fn model_accepts(segments: &[Seg], entry: u64) -> bool {
    for segment in segments {
        if segment.filesz > segment.memsz {
            return false;
        }
        let Some(end) = segment.vaddr.checked_add(segment.memsz) else {
            return false;
        };
        if segment.vaddr < USER_START || end > USER_END {
            return false;
        }
        if segment.flags & PF_W != 0 && segment.flags & PF_X != 0 {
            return false;
        }
    }
    let mut spans: Vec<(u64, u64)> = segments
        .iter()
        .filter(|segment| segment.memsz != 0)
        .map(|segment| {
            (
                floor_page(segment.vaddr),
                ceil_page(segment.vaddr + segment.memsz),
            )
        })
        .collect();
    spans.sort_unstable();
    let mut previous_end = 0u64;
    for (start, end) in spans {
        if start < previous_end {
            return false;
        }
        previous_end = end;
    }
    segments.iter().any(|segment| {
        segment.flags & PF_X != 0
            && entry >= segment.vaddr
            && entry < segment.vaddr.saturating_add(segment.memsz)
    })
}

#[test]
fn c9_page_rounding_property_model() {
    let mut rng = Rng(0x5eed_c9c9_1234_5678);
    let mut accepted = 0usize;
    for _ in 0..5_000 {
        let count = 1 + rng.below(6) as usize;
        let mut segments = Vec::with_capacity(count);
        for _ in 0..count {
            let vaddr = (0x400 + rng.below(16)) * PAGE + rng.below(PAGE);
            let memsz = match rng.below(5) {
                0 => 0,
                1 => 1 + rng.below(PAGE),
                2 => PAGE + rng.below(PAGE),
                3 => 2 * PAGE + rng.below(PAGE),
                _ => 3 * PAGE + rng.below(PAGE),
            };
            let filesz = if memsz == 0 { 0 } else { rng.below(memsz + 1) };
            let flags = match rng.below(6) {
                0 => PF_R,
                1 => PF_R | PF_X,
                2 => PF_R | PF_W,
                3 => PF_W,
                4 => PF_X,
                _ => PF_R | PF_W | PF_X,
            };
            segments.push(Seg {
                vaddr,
                memsz,
                filesz,
                flags,
            });
        }
        // Half the time point the entry at a random segment; otherwise use 0. The model then
        // decides whether that entry is executable.
        let entry = if rng.below(2) == 0 {
            0
        } else {
            let index = rng.below(count as u64) as usize;
            segments[index].vaddr
        };
        let file = build(&segments, entry);
        let parsed = Elf::parse(&file, Machine::X86_64, USER_START..USER_END);
        let expected = model_accepts(&segments, entry);
        assert_eq!(
            parsed.is_ok(),
            expected,
            "case segments={segments:?} entry={entry:#x}"
        );
        if let Ok(elf) = parsed {
            accepted += 1;
            let copy: Vec<MapStep> = plan_load(&elf, PAGE, LoadMode::Copy)
                .expect("accepted image plans")
                .collect();
            let mut previous_end = 0u64;
            for step in &copy {
                assert_eq!(step.vaddr % PAGE, 0, "step vaddr page-aligned");
                assert!(
                    step.len != 0 && step.len % PAGE == 0,
                    "step len page-aligned"
                );
                assert!(step.vaddr >= previous_end, "steps sorted and disjoint");
                previous_end = step.vaddr + step.len;
            }
            let needs_tail = segments
                .iter()
                .any(|segment| segment.filesz != 0 && (segment.vaddr + segment.filesz) % PAGE != 0);
            let strict = plan_load(&elf, PAGE, LoadMode::Strict);
            if needs_tail {
                assert_eq!(strict.map(|_| ()), Err(ElfError::FileTailNotPageAligned));
            } else {
                let strict_steps: Vec<MapStep> = strict.expect("aligned strict plan").collect();
                assert!(
                    strict_steps
                        .iter()
                        .all(|step| !matches!(step.kind, StepKind::CopyTail { .. }))
                );
            }
        }
    }
    // The generator must exercise both sides of the model; otherwise the property is vacuous.
    assert!(accepted > 100, "accepted cases: {accepted}");
}
