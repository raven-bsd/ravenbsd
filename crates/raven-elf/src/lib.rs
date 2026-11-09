// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Strict ELF64 parser and page-level load planner.
//!
//! Three consumers load ELF images without trusting them: the UEFI loader (the kernel), the kernel
//! (`/init`) and `raven_rt::spawn` (user programs). This crate is the single validator they share. It
//! is `no_std`, allocation-free, never panics on arbitrary input and contains no `unsafe` code.
//!
//! # Accepted input
//!
//! Only 64-bit little-endian ELF files are accepted. `e_machine` must name the [`Machine`] the caller
//! expects, and `e_type` must be `ET_EXEC` or `ET_DYN`. For `ET_DYN` the segment `vaddr` values are
//! used as given; relocation is the loader's business. Only `PT_LOAD` segments are exposed. Every
//! `PT_LOAD` must satisfy, in this order:
//!
//! 1. `p_filesz <= p_memsz` and `p_align` is 0, 1 or a power of two.
//! 2. `p_offset + p_filesz` is representable and inside the file.
//! 3. `p_vaddr + p_memsz` is representable and inside the caller's `user_range`.
//! 4. `p_vaddr % 4096 == p_offset % 4096` (the file and memory page offsets agree).
//! 5. The segment is not both writable and executable.
//! 6. After rounding to 4096-byte pages, segments do not overlap (checked after sorting by `vaddr`).
//!
//! The entry point must lie inside an executable `PT_LOAD` segment. A `PT_INTERP` string must be
//! NUL-terminated inside its program header's file range, non-empty, UTF-8 and at most 4096 bytes.
//!
//! # Load planning
//!
//! [`plan_load`] turns each validated segment into a sequence of page-sized [`MapStep`]s. A segment
//! is split into three parts:
//!
//! * whole file-backed pages, emitted as [`StepKind::File`] with the file offset of the page;
//! * at most one partial file page, emitted as [`StepKind::CopyTail`] in [`LoadMode::Copy`] mode (the
//!   loader copies `len` bytes and zero-fills the rest of the page, because the page must still be
//!   mapped as a whole). [`LoadMode::Strict`] rejects it with
//!   [`ElfError::FileTailNotPageAligned`] instead, for callers that cannot write into the target;
//! * zero-filled pages covering the rest of `p_memsz`, emitted as [`StepKind::Zero`].
//!
//! `MapStep::vaddr` and `MapStep::len` are page-aligned, steps are sorted by increasing address and
//! do not overlap.
//!
//! ## Worked example
//!
//! With a 4096-byte page and these three segments (file offsets chosen by a linker):
//!
//! ```text
//! segment   vaddr       filesz    memsz     prot    file offset
//! text      0x0040_0000 0x2000    0x2000    R|X     0x1000
//! rodata    0x0040_2000 0x1000    0x1000    R       0x3000
//! data      0x0040_3000 0x1234    0x3000    R|W     0x4000
//! ```
//!
//! `plan_load(elf, 4096, LoadMode::Copy)` yields:
//!
//! ```text
//! vaddr       len   prot  step
//! 0x0040_0000 0x1000 R|X  File { offset: 0x1000 }
//! 0x0040_1000 0x1000 R|X  File { offset: 0x2000 }
//! 0x0040_2000 0x1000 R    File { offset: 0x3000 }
//! 0x0040_3000 0x1000 R|W  File { offset: 0x4000 }
//! 0x0040_4000 0x1000 R|W  CopyTail { offset: 0x5000, len: 0x234 }
//! 0x0040_5000 0x1000 R|W  Zero
//! ```
//!
//! The `data` segment's file part ends at `0x0040_3000 + 0x1234 = 0x0040_4234`, in the middle of the
//! page at `0x0040_4000`: `CopyTail` copies the remaining `0x234` bytes from file offset `0x4000 +
//! 0x1000 = 0x5000` and the loader zero-fills the rest of that page. `LoadMode::Strict` returns
//! [`ElfError::FileTailNotPageAligned`] for the same file because the partial page cannot be filled
//! from file data alone.
//!
//! Sizes of the public structures: [`Segment`] is five `u64`/`u8` values and [`MapStep`] is a
//! `#[repr(Rust)]` value type; neither is shared across a binary boundary. The crate performs no
//! I/O and no allocation.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

use core::ops::Range;

mod plan;

pub use plan::{LoadMode, MapStep, StepKind, plan_load};

const EHDR_SIZE: usize = 64;
const PHDR_SIZE: u16 = 56;
const MAX_PHNUM: u16 = 64;
const MAX_SEGMENTS: usize = 16;
const MAX_INTERP: usize = 4096;

const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PT_PHDR: u32 = 6;

const PF_X: u32 = 1;
const PF_W: u32 = 2;
const PF_R: u32 = 4;

const EM_X86_64: u16 = 62;
const EM_AARCH64: u16 = 183;
const EM_RISCV: u16 = 243;

const ET_EXEC: u16 = 2;
const ET_DYN: u16 = 3;

/// Read permission in [`Segment::prot`].
const PROT_R: u8 = 1;
/// Write permission in [`Segment::prot`].
const PROT_W: u8 = 2;
/// Execute permission in [`Segment::prot`].
const PROT_X: u8 = 4;

/// Page size used by validation and by the load planner's default callers.
const PAGE_SIZE: u64 = 4096;

/// The architecture the caller expects and the ELF header must declare.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Machine {
    /// x86-64 (`EM_X86_64 = 62`).
    X86_64,
    /// AArch64 (`EM_AARCH64 = 183`).
    Aarch64,
    /// RISC-V 64 (`EM_RISCV = 243`).
    Riscv64,
}

/// Why an ELF file was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElfError {
    /// The input is shorter than the 64-byte ELF header.
    TooShort,
    /// The `\x7fELF` magic number is missing.
    BadMagic,
    /// `EI_CLASS` is not `ELFCLASS64`.
    NotElf64,
    /// `EI_DATA` is not `ELFDATA2LSB`.
    NotLittleEndian,
    /// `e_machine` does not match the requested [`Machine`].
    BadMachine,
    /// `e_type` is not `ET_EXEC` or `ET_DYN`.
    BadType,
    /// A program header or ELF header field is malformed.
    BadPhdr,
    /// A segment's file range is not fully inside the file.
    SegmentOutOfBounds,
    /// Two segments overlap after rounding their address ranges to 4096-byte pages.
    SegmentOverlap,
    /// A segment's memory range is outside the caller's user address range.
    SegmentOutsideUserRange,
    /// A segment is both writable and executable.
    WritableAndExecutable,
    /// More than 16 `PT_LOAD` segments (or more than 64 program headers) were declared.
    TooManySegments,
    /// A file part does not end on a page boundary and `LoadMode::Strict` was requested.
    FileTailNotPageAligned,
}

impl core::fmt::Display for ElfError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::TooShort => "file shorter than the ELF header",
            Self::BadMagic => "missing ELF magic",
            Self::NotElf64 => "not a 64-bit ELF",
            Self::NotLittleEndian => "not a little-endian ELF",
            Self::BadMachine => "ELF machine does not match",
            Self::BadType => "ELF type is not ET_EXEC or ET_DYN",
            Self::BadPhdr => "malformed ELF header or program header",
            Self::SegmentOutOfBounds => "segment file range outside the file",
            Self::SegmentOverlap => "segments overlap after page rounding",
            Self::SegmentOutsideUserRange => "segment memory range outside the user range",
            Self::WritableAndExecutable => "segment is writable and executable",
            Self::TooManySegments => "too many program headers or load segments",
            Self::FileTailNotPageAligned => "file part is not page aligned in strict mode",
        })
    }
}

/// A validated `PT_LOAD` segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Virtual address of the first byte of the segment.
    pub vaddr: u64,
    /// Size of the memory image in bytes; includes the zero-filled tail.
    pub memsz: u64,
    /// Number of bytes backed by file data at [`Segment::offset`].
    pub filesz: u64,
    /// File offset of the first byte of the segment.
    pub offset: u64,
    /// RavenBSD protection bits: `1 = read`, `2 = write`, `4 = execute`.
    pub prot: u8,
}

const EMPTY_SEGMENT: Segment = Segment {
    vaddr: 0,
    memsz: 0,
    filesz: 0,
    offset: 0,
    prot: 0,
};

/// A fully validated ELF64 file borrowing the original byte slice.
#[derive(Clone, Copy, Debug)]
pub struct Elf<'a> {
    data: &'a [u8],
    entry: u64,
    phnum: u16,
    phdr_vaddr: Option<u64>,
    interp: Option<&'a str>,
    segments: [Segment; MAX_SEGMENTS],
    segment_count: usize,
    user_start: u64,
    user_end: u64,
}

impl<'a> Elf<'a> {
    /// Parse and validate an ELF64 file.
    ///
    /// `machine` is the architecture the caller is loading for; `user_range` is the half-open
    /// address range the image may occupy. Every validation rule listed in the crate documentation is
    /// applied here, so success means [`segments`](Self::segments) is safe to plan and map.
    pub fn parse(
        data: &'a [u8],
        machine: Machine,
        user_range: Range<u64>,
    ) -> Result<Elf<'a>, ElfError> {
        if data.len() < EHDR_SIZE {
            return Err(ElfError::TooShort);
        }
        let magic = data.get(0..4).ok_or(ElfError::TooShort)?;
        if !magic.starts_with(b"\x7fELF") {
            return Err(ElfError::BadMagic);
        }
        let class = *data.get(4).ok_or(ElfError::TooShort)?;
        if class != 2 {
            return Err(ElfError::NotElf64);
        }
        let encoding = *data.get(5).ok_or(ElfError::TooShort)?;
        if encoding != 1 {
            return Err(ElfError::NotLittleEndian);
        }

        let e_type = read_u16(data, 16).ok_or(ElfError::TooShort)?;
        let e_machine = read_u16(data, 18).ok_or(ElfError::TooShort)?;
        let entry = read_u64(data, 24).ok_or(ElfError::TooShort)?;
        let e_phoff = read_u64(data, 32).ok_or(ElfError::TooShort)?;
        let e_ehsize = read_u16(data, 52).ok_or(ElfError::TooShort)?;
        let e_phentsize = read_u16(data, 54).ok_or(ElfError::TooShort)?;
        let e_phnum = read_u16(data, 56).ok_or(ElfError::TooShort)?;

        if e_type != ET_EXEC && e_type != ET_DYN {
            return Err(ElfError::BadType);
        }
        let expected_machine = match machine {
            Machine::X86_64 => EM_X86_64,
            Machine::Aarch64 => EM_AARCH64,
            Machine::Riscv64 => EM_RISCV,
        };
        if e_machine != expected_machine {
            return Err(ElfError::BadMachine);
        }
        if e_ehsize as usize != EHDR_SIZE {
            return Err(ElfError::BadPhdr);
        }
        if e_phentsize != PHDR_SIZE {
            return Err(ElfError::BadPhdr);
        }
        if e_phnum == 0 {
            return Err(ElfError::BadPhdr);
        }
        if e_phnum > MAX_PHNUM {
            return Err(ElfError::TooManySegments);
        }
        let data_len = u64::try_from(data.len()).map_err(|_| ElfError::BadPhdr)?;
        let table_len = u64::from(e_phentsize)
            .checked_mul(u64::from(e_phnum))
            .ok_or(ElfError::BadPhdr)?;
        let table_end = e_phoff.checked_add(table_len).ok_or(ElfError::BadPhdr)?;
        if table_end > data_len {
            return Err(ElfError::BadPhdr);
        }

        let mut segments = [EMPTY_SEGMENT; MAX_SEGMENTS];
        let mut segment_count = 0usize;
        let mut interp: Option<&'a str> = None;
        let mut phdr_vaddr_field: Option<u64> = None;
        let mut saw_interp = false;
        let mut saw_phdr = false;

        let mut index: u16 = 0;
        while index < e_phnum {
            let base = u64::from(index)
                .checked_mul(u64::from(e_phentsize))
                .and_then(|delta| e_phoff.checked_add(delta))
                .and_then(|offset| usize::try_from(offset).ok())
                .ok_or(ElfError::BadPhdr)?;
            let p_type = read_u32(data, base).ok_or(ElfError::BadPhdr)?;

            if p_type == PT_LOAD {
                if segment_count >= MAX_SEGMENTS {
                    return Err(ElfError::TooManySegments);
                }
                let p_flags = read_u32(data, base.checked_add(4).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let p_offset = read_u64(data, base.checked_add(8).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let p_vaddr = read_u64(data, base.checked_add(16).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let p_filesz = read_u64(data, base.checked_add(32).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let p_memsz = read_u64(data, base.checked_add(40).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let p_align = read_u64(data, base.checked_add(48).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;

                if p_filesz > p_memsz {
                    return Err(ElfError::BadPhdr);
                }
                if p_align != 0 && p_align != 1 && !p_align.is_power_of_two() {
                    return Err(ElfError::BadPhdr);
                }
                let file_end = p_offset
                    .checked_add(p_filesz)
                    .ok_or(ElfError::SegmentOutOfBounds)?;
                if file_end > data_len {
                    return Err(ElfError::SegmentOutOfBounds);
                }
                let mem_end = p_vaddr
                    .checked_add(p_memsz)
                    .ok_or(ElfError::SegmentOutsideUserRange)?;
                if p_vaddr < user_range.start || mem_end > user_range.end {
                    return Err(ElfError::SegmentOutsideUserRange);
                }
                if p_vaddr % PAGE_SIZE != p_offset % PAGE_SIZE {
                    return Err(ElfError::BadPhdr);
                }
                if p_flags & PF_W != 0 && p_flags & PF_X != 0 {
                    return Err(ElfError::WritableAndExecutable);
                }
                let mut prot = 0u8;
                if p_flags & PF_R != 0 {
                    prot |= PROT_R;
                }
                if p_flags & PF_W != 0 {
                    prot |= PROT_W;
                }
                if p_flags & PF_X != 0 {
                    prot |= PROT_X;
                }
                let slot = segments
                    .get_mut(segment_count)
                    .ok_or(ElfError::TooManySegments)?;
                *slot = Segment {
                    vaddr: p_vaddr,
                    memsz: p_memsz,
                    filesz: p_filesz,
                    offset: p_offset,
                    prot,
                };
                segment_count += 1;
            } else if p_type == PT_INTERP {
                if saw_interp {
                    return Err(ElfError::BadPhdr);
                }
                saw_interp = true;
                let p_offset = read_u64(data, base.checked_add(8).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let p_filesz = read_u64(data, base.checked_add(32).ok_or(ElfError::BadPhdr)?)
                    .ok_or(ElfError::BadPhdr)?;
                let file_end = p_offset.checked_add(p_filesz).ok_or(ElfError::BadPhdr)?;
                if file_end > data_len {
                    return Err(ElfError::BadPhdr);
                }
                let start = usize::try_from(p_offset).map_err(|_| ElfError::BadPhdr)?;
                let len = usize::try_from(p_filesz).map_err(|_| ElfError::BadPhdr)?;
                let end = start.checked_add(len).ok_or(ElfError::BadPhdr)?;
                let bytes = data.get(start..end).ok_or(ElfError::BadPhdr)?;
                let nul = bytes
                    .iter()
                    .position(|&byte| byte == 0)
                    .ok_or(ElfError::BadPhdr)?;
                if nul == 0 || nul > MAX_INTERP {
                    return Err(ElfError::BadPhdr);
                }
                let path = bytes.get(..nul).ok_or(ElfError::BadPhdr)?;
                interp = Some(core::str::from_utf8(path).map_err(|_| ElfError::BadPhdr)?);
            } else if p_type == PT_PHDR {
                if saw_phdr {
                    return Err(ElfError::BadPhdr);
                }
                saw_phdr = true;
                phdr_vaddr_field = Some(
                    read_u64(data, base.checked_add(16).ok_or(ElfError::BadPhdr)?)
                        .ok_or(ElfError::BadPhdr)?,
                );
            }
            index += 1;
        }

        if let Some(sorted) = segments.get_mut(..segment_count) {
            sorted.sort_unstable_by_key(|segment| segment.vaddr);
        }

        let mut previous_end: Option<u64> = None;
        for segment in segments.iter().take(segment_count) {
            if segment.memsz == 0 {
                continue;
            }
            let start = page_floor(segment.vaddr);
            let mem_end = segment
                .vaddr
                .checked_add(segment.memsz)
                .ok_or(ElfError::SegmentOutsideUserRange)?;
            let end = page_ceil(mem_end).ok_or(ElfError::SegmentOutsideUserRange)?;
            if let Some(previous) = previous_end
                && start < previous
            {
                return Err(ElfError::SegmentOverlap);
            }
            previous_end = Some(end);
        }

        let mut entry_is_executable = false;
        for segment in segments.iter().take(segment_count) {
            if segment.prot & PROT_X == 0 {
                continue;
            }
            let Some(end) = segment.vaddr.checked_add(segment.memsz) else {
                continue;
            };
            if entry >= segment.vaddr && entry < end {
                entry_is_executable = true;
                break;
            }
        }
        if !entry_is_executable {
            return Err(ElfError::BadPhdr);
        }

        let phdr_vaddr = phdr_vaddr_field.or_else(|| {
            for segment in segments.iter().take(segment_count) {
                let Some(file_end) = segment.offset.checked_add(segment.filesz) else {
                    continue;
                };
                if e_phoff >= segment.offset && e_phoff < file_end {
                    return segment
                        .vaddr
                        .checked_add(e_phoff.checked_sub(segment.offset)?);
                }
            }
            None
        });

        Ok(Elf {
            data,
            entry,
            phnum: e_phnum,
            phdr_vaddr,
            interp,
            segments,
            segment_count,
            user_start: user_range.start,
            user_end: user_range.end,
        })
    }

    /// Returns the ELF entry point.
    pub fn entry(&self) -> u64 {
        self.entry
    }

    /// Iterates over the validated `PT_LOAD` segments in ascending `vaddr` order.
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.segments.iter().take(self.segment_count).copied()
    }

    /// Returns the file bytes backing `segment`.
    ///
    /// For a segment produced by [`parse`](Self::parse) the returned slice has exactly `filesz`
    /// bytes. A segment value that does not describe a valid file range in this ELF yields an empty
    /// slice instead of a panic.
    pub fn segment_data(&self, segment: &Segment) -> &'a [u8] {
        let Ok(start) = usize::try_from(segment.offset) else {
            return &[];
        };
        let Ok(len) = usize::try_from(segment.filesz) else {
            return &[];
        };
        let Some(end) = start.checked_add(len) else {
            return &[];
        };
        self.data.get(start..end).unwrap_or(&[])
    }

    /// Returns the `PT_INTERP` program interpreter path, if the file declares one.
    pub fn interp(&self) -> Option<&'a str> {
        self.interp
    }

    /// Returns the address of the program headers in memory.
    ///
    /// This is `PT_PHDR`'s `p_vaddr` when present. Otherwise it is computed from the load segment
    /// whose file range contains `e_phoff`, or `None` when no segment contains the table.
    pub fn phdr_vaddr(&self) -> Option<u64> {
        self.phdr_vaddr
    }

    /// Returns the number of program headers (`e_phnum`).
    pub fn phnum(&self) -> usize {
        usize::from(self.phnum)
    }

    pub(crate) fn user_range(&self) -> Range<u64> {
        self.user_start..self.user_end
    }
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let bytes: [u8; 2] = data.get(offset..end)?.try_into().ok()?;
    Some(u16::from_le_bytes(bytes))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let bytes: [u8; 4] = data.get(offset..end)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    let end = offset.checked_add(8)?;
    let bytes: [u8; 8] = data.get(offset..end)?.try_into().ok()?;
    Some(u64::from_le_bytes(bytes))
}

fn page_floor(value: u64) -> u64 {
    value - (value % PAGE_SIZE)
}

fn page_ceil(value: u64) -> Option<u64> {
    let remainder = value % PAGE_SIZE;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(PAGE_SIZE - remainder)
    }
}

#[cfg(test)]
mod tests;
