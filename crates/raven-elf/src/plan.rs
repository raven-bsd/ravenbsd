// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Page-level load planning.
//!
//! See the crate documentation for the algorithm and a worked example.

use crate::{Elf, ElfError, MAX_SEGMENTS};

/// How [`plan_load`] handles a file part that does not end on a page boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadMode {
    /// Reject a partial last file page with [`ElfError::FileTailNotPageAligned`].
    Strict,
    /// Emit a [`StepKind::CopyTail`] step for the partial last file page.
    Copy,
}

/// What a single [`MapStep`] maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    /// Map whole pages of file data beginning at `offset`.
    File {
        /// File offset of the first byte of the step's first page.
        offset: u64,
    },
    /// Map pages that must be zero-filled.
    Zero,
    /// Map one page: copy `len` bytes from `offset`, zero-fill the rest of the page.
    CopyTail {
        /// File offset of the first byte to copy.
        offset: u64,
        /// Number of bytes to copy; always less than the page size.
        len: u64,
    },
}

/// One page-aligned mapping step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapStep {
    /// Page-aligned virtual address of the first byte.
    pub vaddr: u64,
    /// Length in bytes; a positive multiple of the page size.
    pub len: u64,
    /// RavenBSD protection bits: `1 = read`, `2 = write`, `4 = execute`.
    pub prot: u8,
    /// Where the page contents come from.
    pub kind: StepKind,
}

#[derive(Clone, Copy)]
struct SegPlan {
    prot: u8,
    filesz: u64,
    start: u64,
    file_end: u64,
    file_whole_end: u64,
    zero_start: u64,
    end: u64,
    file_offset_base: u64,
}

impl SegPlan {
    const EMPTY: Self = Self {
        prot: 0,
        filesz: 0,
        start: 0,
        file_end: 0,
        file_whole_end: 0,
        zero_start: 0,
        end: 0,
        file_offset_base: 0,
    };

    fn has_file_tail(&self) -> bool {
        self.filesz > 0 && self.file_whole_end < self.file_end
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Start,
    File,
    CopyTail,
    Zero,
    Done,
}

struct Plan<'a> {
    /// Keeps the borrow of the ELF (and therefore of its bytes) for the iterator's lifetime.
    _elf: &'a Elf<'a>,
    page: u64,
    plans: [SegPlan; MAX_SEGMENTS],
    count: usize,
    segment_index: usize,
    phase: Phase,
    cursor: u64,
}

impl Iterator for Plan<'_> {
    type Item = MapStep;

    fn next(&mut self) -> Option<MapStep> {
        loop {
            if self.segment_index >= self.count {
                return None;
            }
            let plan = *self.plans.get(self.segment_index)?;
            match self.phase {
                Phase::Start => {
                    self.cursor = plan.start;
                    self.phase = Phase::File;
                }
                Phase::File => {
                    if self.cursor < plan.file_whole_end {
                        let delta = self.cursor.checked_sub(plan.start)?;
                        let offset = plan.file_offset_base.checked_add(delta)?;
                        let step = MapStep {
                            vaddr: self.cursor,
                            len: self.page,
                            prot: plan.prot,
                            kind: StepKind::File { offset },
                        };
                        self.cursor = self.cursor.checked_add(self.page)?;
                        return Some(step);
                    }
                    self.phase = if plan.has_file_tail() {
                        Phase::CopyTail
                    } else {
                        Phase::Zero
                    };
                    self.cursor = plan.zero_start;
                }
                Phase::CopyTail => {
                    self.phase = Phase::Zero;
                    if plan.has_file_tail() {
                        let offset = plan
                            .file_offset_base
                            .checked_add(plan.file_whole_end.checked_sub(plan.start)?)?;
                        let len = plan.file_end.checked_sub(plan.file_whole_end)?;
                        return Some(MapStep {
                            vaddr: plan.file_whole_end,
                            len: self.page,
                            prot: plan.prot,
                            kind: StepKind::CopyTail { offset, len },
                        });
                    }
                }
                Phase::Zero => {
                    if self.cursor < plan.end {
                        let step = MapStep {
                            vaddr: self.cursor,
                            len: self.page,
                            prot: plan.prot,
                            kind: StepKind::Zero,
                        };
                        self.cursor = self.cursor.checked_add(self.page)?;
                        return Some(step);
                    }
                    self.phase = Phase::Done;
                }
                Phase::Done => {
                    self.segment_index += 1;
                    self.phase = Phase::Start;
                }
            }
        }
    }
}

/// Plan the page mappings needed to load `elf`.
///
/// Every `PT_LOAD` segment is split into whole file pages (`File`), at most one partial file page
/// (`CopyTail` in [`LoadMode::Copy`]) and zero-filled tail pages (`Zero`). Steps are page-aligned,
/// non-overlapping and sorted by increasing address. `page` must be non-zero; validation and segment
/// ordering are repeated at that page size so the returned steps remain non-overlapping even when
/// the caller uses a page size other than 4096.
///
/// In [`LoadMode::Strict`] a segment with a non-empty file part whose end is not page-aligned makes
/// the whole plan fail with [`ElfError::FileTailNotPageAligned`]; nothing is yielded in that case.
pub fn plan_load<'a>(
    elf: &'a Elf<'a>,
    page: u64,
    mode: LoadMode,
) -> Result<impl Iterator<Item = MapStep> + 'a, ElfError> {
    if page == 0 {
        return Err(ElfError::BadPhdr);
    }
    let user_range = elf.user_range();
    let mut plans = [SegPlan::EMPTY; MAX_SEGMENTS];
    let mut count = 0usize;
    let mut previous_end: Option<u64> = None;

    for segment in elf.segments() {
        if segment.memsz == 0 {
            continue;
        }
        let start = page_floor(segment.vaddr, page);
        let file_end = segment
            .vaddr
            .checked_add(segment.filesz)
            .ok_or(ElfError::SegmentOutsideUserRange)?;
        let mem_end = segment
            .vaddr
            .checked_add(segment.memsz)
            .ok_or(ElfError::SegmentOutsideUserRange)?;
        let end = page_ceil(mem_end, page).ok_or(ElfError::SegmentOutsideUserRange)?;
        let file_whole_end = page_floor(file_end, page);
        let zero_start = if segment.filesz == 0 {
            start
        } else {
            page_ceil(file_end, page).ok_or(ElfError::SegmentOutsideUserRange)?
        };
        if mode == LoadMode::Strict && segment.filesz > 0 && zero_start != file_end {
            return Err(ElfError::FileTailNotPageAligned);
        }
        if start < user_range.start || end > user_range.end {
            return Err(ElfError::SegmentOutsideUserRange);
        }
        let file_offset_base = if segment.filesz == 0 {
            segment.offset
        } else {
            let delta = segment.vaddr.checked_sub(start).ok_or(ElfError::BadPhdr)?;
            segment.offset.checked_sub(delta).ok_or(ElfError::BadPhdr)?
        };
        if let Some(previous) = previous_end
            && start < previous
        {
            return Err(ElfError::SegmentOverlap);
        }
        previous_end = Some(end);
        let Some(slot) = plans.get_mut(count) else {
            return Err(ElfError::TooManySegments);
        };
        *slot = SegPlan {
            prot: segment.prot,
            filesz: segment.filesz,
            start,
            file_end,
            file_whole_end,
            zero_start,
            end,
            file_offset_base,
        };
        count += 1;
    }

    Ok(Plan {
        _elf: elf,
        page,
        plans,
        count,
        segment_index: 0,
        phase: Phase::Start,
        cursor: 0,
    })
}

fn page_floor(value: u64, page: u64) -> u64 {
    value - (value % page)
}

fn page_ceil(value: u64, page: u64) -> Option<u64> {
    let remainder = value % page;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(page - remainder)
    }
}
