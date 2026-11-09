// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Conversion of the UEFI memory map into sorted, page-aligned `MemoryRegion`s.
//!
//! Every UEFI memory type is mapped through [`CONVERSION`] exactly once; the kernel image and the
//! initfs copy are overlaid as `KernelAndModules`. Low memory below 1 MiB is always `Reserved`
//! (firmware data, real-mode IVT/BDA and legacy holes must not become kernel pages), overlaps are
//! resolved by [`priority`] with `Reserved` above ordinary kinds, and adjacent equal regions are
//! merged.
//!
//! The loader runs the conversion after `ExitBootServices`, where allocation is forbidden. All
//! scratch buffers therefore come from [`MemMapBuffers`], which the caller fills with generous
//! capacities before exiting.

use alloc::vec::Vec;
use core::fmt;

use raven_bootinfo::{MemoryKind, MemoryRegion, PhysRange};
use uefi::mem::memory_map::{MemoryDescriptor, MemoryType};

const PAGE_SIZE: u64 = 4096;
/// Firmware-described memory below this address is never handed to the kernel as usable.
pub const LOW_MEMORY_END: u64 = 0x10_0000;

/// Conversion table for every UEFI memory type this loader understands.
///
/// Types not listed here are `Reserved`, as are `PERSISTENT_MEMORY` and the platform-specific
/// types (PAL, unaccepted memory) that we do not support yet.
pub const CONVERSION: &[(u32, MemoryKind)] = &[
    (MemoryType::CONVENTIONAL.0, MemoryKind::Usable),
    (MemoryType::LOADER_CODE.0, MemoryKind::LoaderReclaimable),
    (MemoryType::LOADER_DATA.0, MemoryKind::LoaderReclaimable),
    (
        MemoryType::BOOT_SERVICES_CODE.0,
        MemoryKind::LoaderReclaimable,
    ),
    (
        MemoryType::BOOT_SERVICES_DATA.0,
        MemoryKind::LoaderReclaimable,
    ),
    (MemoryType::RUNTIME_SERVICES_CODE.0, MemoryKind::Reserved),
    (MemoryType::RUNTIME_SERVICES_DATA.0, MemoryKind::Reserved),
    (MemoryType::ACPI_RECLAIM.0, MemoryKind::AcpiReclaimable),
    (MemoryType::ACPI_NON_VOLATILE.0, MemoryKind::AcpiNvs),
    (MemoryType::MMIO.0, MemoryKind::Mmio),
    (MemoryType::MMIO_PORT_SPACE.0, MemoryKind::Mmio),
    (MemoryType::UNUSABLE.0, MemoryKind::BadMemory),
    (MemoryType::PERSISTENT_MEMORY.0, MemoryKind::Reserved),
    (MemoryType::RESERVED.0, MemoryKind::Reserved),
];

/// Why a UEFI memory map could not be converted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemMapError {
    /// A descriptor's page count overflows the address space.
    Overflow,
    /// A scratch or output buffer was too small for the converted map.
    Capacity,
}

impl fmt::Display for MemMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MemMapError::Overflow => {
                f.write_str("UEFI memory descriptor overflows the address space")
            }
            MemMapError::Capacity => f.write_str("converted memory map does not fit its buffer"),
        }
    }
}

/// Preallocated scratch space for [`convert_into`].
pub struct MemMapBuffers {
    spans: Vec<(u64, u64, MemoryKind)>,
    tracked: Vec<(u64, u64)>,
    boundaries: Vec<u64>,
    entries: Vec<(u64, u64, MemoryKind)>,
}

impl MemMapBuffers {
    /// Allocate scratch for a map with at most `max_entries` final entries.
    pub fn new(max_entries: usize) -> Self {
        let boundary_capacity = max_entries.saturating_mul(2).saturating_add(8);
        Self {
            spans: Vec::with_capacity(max_entries.saturating_add(4)),
            tracked: Vec::with_capacity(8),
            boundaries: Vec::with_capacity(boundary_capacity),
            entries: Vec::with_capacity(boundary_capacity),
        }
    }
}

/// Map one UEFI memory type; unknown types are `Reserved`.
pub fn convert_type(ty: MemoryType) -> MemoryKind {
    CONVERSION
        .iter()
        .find_map(|&(value, kind)| (value == ty.0).then_some(kind))
        .unwrap_or(MemoryKind::Reserved)
}

/// Overlap priority: larger wins. Tracked kernel/initfs ranges are handled separately.
fn priority(kind: MemoryKind) -> u8 {
    match kind {
        MemoryKind::Reserved => 6,
        MemoryKind::BadMemory => 5,
        MemoryKind::Mmio => 4,
        MemoryKind::AcpiNvs | MemoryKind::AcpiReclaimable => 3,
        MemoryKind::LoaderReclaimable => 2,
        MemoryKind::Usable => 1,
        MemoryKind::KernelAndModules => 7,
    }
}

fn page_floor(value: u64) -> u64 {
    value & !(PAGE_SIZE - 1)
}

fn page_ceil(value: u64) -> Option<u64> {
    value.checked_add(PAGE_SIZE - 1).map(page_floor)
}

fn push_span(
    spans: &mut Vec<(u64, u64, MemoryKind)>,
    span: (u64, u64, MemoryKind),
) -> Result<(), MemMapError> {
    if spans.len() == spans.capacity() {
        return Err(MemMapError::Capacity);
    }
    spans.push(span);
    Ok(())
}

/// Allocating convenience wrapper used by tests; the loader uses [`convert_into`].
pub fn convert(
    descriptors: &[MemoryDescriptor],
    tracked: &[PhysRange],
) -> Result<Vec<MemoryRegion>, MemMapError> {
    let max_entries = descriptors
        .len()
        .checked_mul(2)
        .and_then(|value| value.checked_add(16))
        .ok_or(MemMapError::Overflow)?;
    let mut buffers = MemMapBuffers::new(max_entries);
    let mut out = Vec::with_capacity(max_entries.saturating_mul(4).saturating_add(16));
    convert_into(descriptors, tracked, &mut out, &mut buffers)?;
    Ok(out)
}

/// Convert `descriptors` into `out`, overlaying `tracked` as `KernelAndModules` and reserving low
/// memory. Uses `buffers` for scratch, so it never allocates when they have enough capacity.
pub fn convert_into(
    descriptors: &[MemoryDescriptor],
    tracked: &[PhysRange],
    out: &mut Vec<MemoryRegion>,
    buffers: &mut MemMapBuffers,
) -> Result<(), MemMapError> {
    buffers.spans.clear();
    buffers.tracked.clear();
    buffers.boundaries.clear();
    buffers.entries.clear();

    for descriptor in descriptors {
        let length = descriptor
            .page_count
            .checked_mul(PAGE_SIZE)
            .ok_or(MemMapError::Overflow)?;
        if length == 0 {
            continue;
        }
        let start = page_floor(descriptor.phys_start);
        let end = page_ceil(
            descriptor
                .phys_start
                .checked_add(length)
                .ok_or(MemMapError::Overflow)?,
        )
        .ok_or(MemMapError::Overflow)?;
        if end <= start {
            continue;
        }
        push_span(
            &mut buffers.spans,
            (start, end, convert_type(descriptor.ty)),
        )?;
        if buffers.boundaries.len() + 2 > buffers.boundaries.capacity() {
            return Err(MemMapError::Capacity);
        }
        buffers.boundaries.push(start);
        buffers.boundaries.push(end);
    }

    for range in tracked {
        if range.len == 0 {
            continue;
        }
        let start = page_floor(range.base);
        let end = page_ceil(
            range
                .base
                .checked_add(range.len)
                .ok_or(MemMapError::Overflow)?,
        )
        .ok_or(MemMapError::Overflow)?;
        if end <= start {
            continue;
        }
        if buffers.tracked.len() == buffers.tracked.capacity() {
            return Err(MemMapError::Capacity);
        }
        buffers.tracked.push((start, end));
        if buffers.boundaries.len() + 2 > buffers.boundaries.capacity() {
            return Err(MemMapError::Capacity);
        }
        buffers.boundaries.push(start);
        buffers.boundaries.push(end);
    }

    if buffers.boundaries.len() + 2 > buffers.boundaries.capacity() {
        return Err(MemMapError::Capacity);
    }
    buffers.boundaries.push(0);
    buffers.boundaries.push(LOW_MEMORY_END);
    buffers.boundaries.sort_unstable();
    buffers.boundaries.dedup();

    for pair in buffers.boundaries.windows(2) {
        let (start, end) = (pair[0], pair[1]);
        if start >= end {
            continue;
        }

        let in_tracked = buffers
            .tracked
            .iter()
            .any(|&(tracked_start, tracked_end)| tracked_start <= start && tracked_end >= end);
        let covered = buffers
            .spans
            .iter()
            .any(|&(span_start, span_end, _)| span_start <= start && span_end >= end);
        let kind = if in_tracked {
            MemoryKind::KernelAndModules
        } else if start < LOW_MEMORY_END && !covered {
            continue;
        } else if start < LOW_MEMORY_END {
            MemoryKind::Reserved
        } else {
            let mut winner: Option<MemoryKind> = None;
            for &(span_start, span_end, span_kind) in &buffers.spans {
                if span_start <= start
                    && span_end >= end
                    && winner.is_none_or(|current| priority(span_kind) > priority(current))
                {
                    winner = Some(span_kind);
                }
            }
            match winner {
                Some(kind) => kind,
                None => continue,
            }
        };

        if let Some(last) = buffers.entries.last_mut()
            && last.2 == kind
            && last.1 == start
        {
            last.1 = end;
        } else {
            if buffers.entries.len() == buffers.entries.capacity() {
                return Err(MemMapError::Capacity);
            }
            buffers.entries.push((start, end, kind));
        }
    }

    if out.capacity() < buffers.entries.len() {
        return Err(MemMapError::Capacity);
    }
    out.clear();
    for &(base, end, kind) in &buffers.entries {
        out.push(MemoryRegion {
            base,
            len: end - base,
            kind: kind as u32,
            reserved: 0,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uefi::mem::memory_map::MemoryAttribute;

    fn descriptor(ty: MemoryType, base: u64, pages: u64) -> MemoryDescriptor {
        MemoryDescriptor {
            ty,
            padding: 0,
            phys_start: base,
            virt_start: 0,
            page_count: pages,
            att: MemoryAttribute::empty(),
        }
    }

    fn convert_one(ty: MemoryType) -> MemoryKind {
        let mapped = convert(&[descriptor(ty, 0x20_0000, 1)], &[]).unwrap();
        assert_eq!(mapped.len(), 1);
        mapped[0].kind().unwrap()
    }

    #[test]
    fn every_conversion_row_is_covered() {
        for &(value, expected) in CONVERSION {
            assert_eq!(convert_type(MemoryType(value)), expected, "type {value}");
        }
        assert_eq!(convert_type(MemoryType(0x7000_0001)), MemoryKind::Reserved);
    }

    #[test]
    fn conventional_is_usable() {
        assert_eq!(convert_one(MemoryType::CONVENTIONAL), MemoryKind::Usable);
    }

    #[test]
    fn loader_and_boot_services_are_reclaimable() {
        for ty in [
            MemoryType::LOADER_CODE,
            MemoryType::LOADER_DATA,
            MemoryType::BOOT_SERVICES_CODE,
            MemoryType::BOOT_SERVICES_DATA,
        ] {
            assert_eq!(convert_one(ty), MemoryKind::LoaderReclaimable);
        }
    }

    #[test]
    fn runtime_services_are_reserved() {
        for ty in [
            MemoryType::RUNTIME_SERVICES_CODE,
            MemoryType::RUNTIME_SERVICES_DATA,
        ] {
            assert_eq!(convert_one(ty), MemoryKind::Reserved);
        }
    }

    #[test]
    fn acpi_kinds_are_mapped() {
        assert_eq!(
            convert_one(MemoryType::ACPI_RECLAIM),
            MemoryKind::AcpiReclaimable
        );
        assert_eq!(
            convert_one(MemoryType::ACPI_NON_VOLATILE),
            MemoryKind::AcpiNvs
        );
    }

    #[test]
    fn mmio_kinds_are_mapped() {
        assert_eq!(convert_one(MemoryType::MMIO), MemoryKind::Mmio);
        assert_eq!(convert_one(MemoryType::MMIO_PORT_SPACE), MemoryKind::Mmio);
    }

    #[test]
    fn unusable_is_bad_memory() {
        assert_eq!(convert_one(MemoryType::UNUSABLE), MemoryKind::BadMemory);
    }

    #[test]
    fn persistent_is_reserved() {
        assert_eq!(
            convert_one(MemoryType::PERSISTENT_MEMORY),
            MemoryKind::Reserved
        );
    }

    #[test]
    fn adjacent_regions_of_the_same_kind_merge() {
        let mapped = convert(
            &[
                descriptor(MemoryType::CONVENTIONAL, 0x20_0000, 1),
                descriptor(MemoryType::CONVENTIONAL, 0x20_1000, 1),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].base, 0x20_0000);
        assert_eq!(mapped[0].len, 0x2000);
    }

    #[test]
    fn different_kinds_do_not_merge() {
        let mapped = convert(
            &[
                descriptor(MemoryType::CONVENTIONAL, 0x20_0000, 1),
                descriptor(MemoryType::ACPI_RECLAIM, 0x20_1000, 1),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(mapped.len(), 2);
    }

    #[test]
    fn unsorted_input_is_sorted() {
        let mapped = convert(
            &[
                descriptor(MemoryType::CONVENTIONAL, 0x40_0000, 1),
                descriptor(MemoryType::CONVENTIONAL, 0x20_0000, 1),
            ],
            &[],
        )
        .unwrap();
        assert_eq!(mapped[0].base, 0x20_0000);
        assert_eq!(mapped[1].base, 0x40_0000);
    }

    #[test]
    fn overlap_resolution_prefers_reserved() {
        let mapped = convert(
            &[
                descriptor(MemoryType::CONVENTIONAL, 0x20_0000, 2),
                descriptor(MemoryType::RESERVED, 0x20_1000, 1),
            ],
            &[],
        )
        .unwrap();
        let reserved = mapped
            .iter()
            .find(|region| region.kind() == Some(MemoryKind::Reserved))
            .unwrap();
        assert_eq!(reserved.base, 0x20_1000);
        assert_eq!(reserved.len, 0x1000);
    }

    #[test]
    fn low_memory_is_reserved() {
        let mapped = convert(&[descriptor(MemoryType::CONVENTIONAL, 0, 0x200)], &[]).unwrap();
        assert_eq!(mapped.len(), 2);
        assert_eq!(mapped[0].kind(), Some(MemoryKind::Reserved));
        assert_eq!(mapped[0].base, 0);
        assert_eq!(mapped[0].len, LOW_MEMORY_END);
        assert_eq!(mapped[1].kind(), Some(MemoryKind::Usable));
        assert_eq!(mapped[1].base, LOW_MEMORY_END);
        assert_eq!(mapped[1].len, 0x10_0000);
    }

    #[test]
    fn tracked_ranges_are_kernel_and_modules() {
        let kernel = PhysRange {
            base: 0x20_0000,
            len: 0x2000,
        };
        let mapped = convert(
            &[descriptor(MemoryType::LOADER_DATA, 0x20_0000, 2)],
            &[kernel],
        )
        .unwrap();
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind(), Some(MemoryKind::KernelAndModules));
        assert_eq!(mapped[0].base, 0x20_0000);
        assert_eq!(mapped[0].len, 0x2000);
    }

    #[test]
    fn tracked_partial_page_protects_the_whole_page() {
        let tracked = PhysRange {
            base: 0x20_0000,
            len: 4097,
        };
        let mapped = convert(
            &[descriptor(MemoryType::LOADER_DATA, 0x20_0000, 2)],
            &[tracked],
        )
        .unwrap();
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind(), Some(MemoryKind::KernelAndModules));
        assert_eq!(mapped[0].base, 0x20_0000);
        assert_eq!(mapped[0].len, 0x2000);
    }

    #[test]
    fn tracked_unaligned_range_is_page_rounded() {
        let tracked = PhysRange {
            base: 0x20_0123,
            len: 1,
        };
        let mapped = convert(
            &[descriptor(MemoryType::LOADER_DATA, 0x20_0000, 1)],
            &[tracked],
        )
        .unwrap();
        assert_eq!(mapped.len(), 1);
        assert_eq!(mapped[0].kind(), Some(MemoryKind::KernelAndModules));
        assert_eq!(mapped[0].base, 0x20_0000);
        assert_eq!(mapped[0].len, 0x1000);
    }

    #[test]
    fn tracked_range_overflow_is_rejected() {
        let tracked = PhysRange {
            base: u64::MAX - 0x1000,
            len: 0x2000,
        };
        assert_eq!(convert(&[], &[tracked]), Err(MemMapError::Overflow));
    }

    #[test]
    fn page_rounding_and_zero_length() {
        let odd = descriptor(MemoryType::CONVENTIONAL, 0x20_0123, 2);
        let mapped = convert(
            &[odd, descriptor(MemoryType::CONVENTIONAL, 0x30_0000, 0)],
            &[],
        )
        .unwrap();
        assert_eq!(mapped[0].base, 0x20_0000);
        assert_eq!(mapped[0].len, 0x3000);
        assert_eq!(mapped.len(), 1);
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(convert(&[], &[]).unwrap().is_empty());
    }

    #[test]
    fn page_count_overflow_is_rejected() {
        let mut bad = descriptor(MemoryType::CONVENTIONAL, 0, 1);
        bad.page_count = u64::MAX;
        assert_eq!(convert(&[bad], &[]), Err(MemMapError::Overflow));
    }

    #[test]
    fn capacity_is_checked_for_preallocated_output() {
        let mut out = Vec::with_capacity(0);
        let mut buffers = MemMapBuffers::new(4);
        let result = convert_into(
            &[
                descriptor(MemoryType::CONVENTIONAL, 0x20_0000, 1),
                descriptor(MemoryType::ACPI_RECLAIM, 0x30_0000, 1),
            ],
            &[],
            &mut out,
            &mut buffers,
        );
        assert_eq!(result, Err(MemMapError::Capacity));
    }
}
