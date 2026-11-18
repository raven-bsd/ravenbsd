// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pure direct-boot helpers (`lib.rs`).

use raven_bootinfo::{MemoryKind, PhysRange};

use super::*;

fn payload(kernel: &[u8], initfs: &[u8]) -> std::vec::Vec<u8> {
    let mut data = std::vec::Vec::new();
    data.extend_from_slice(&PAYLOAD_MAGIC);
    data.extend_from_slice(&(kernel.len() as u64).to_le_bytes());
    data.extend_from_slice(kernel);
    data.extend_from_slice(initfs);
    data
}

#[test]
fn payload_splits_kernel_and_initfs() {
    let data = payload(b"kernel", b"initfs");
    let parsed = parse_payload(&data).unwrap();
    assert_eq!(parsed.kernel, b"kernel");
    assert_eq!(parsed.initfs, b"initfs");
}

#[test]
fn payload_rejects_bad_input() {
    assert_eq!(parse_payload(b"short"), Err(PayloadError::TooShort));
    let mut data = payload(b"k", b"i");
    data[0] = b'X';
    assert_eq!(parse_payload(&data), Err(PayloadError::BadMagic));
    let mut data = payload(b"k", b"i");
    data[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    assert_eq!(parse_payload(&data), Err(PayloadError::BadKernelLength));
}

#[test]
fn start_info_parses_all_fields() {
    let mut data = [0u8; 56];
    data[0..4].copy_from_slice(&HVM_START_MAGIC.to_le_bytes());
    data[4..8].copy_from_slice(&1u32.to_le_bytes());
    data[8..12].copy_from_slice(&7u32.to_le_bytes());
    data[12..16].copy_from_slice(&1u32.to_le_bytes());
    data[16..24].copy_from_slice(&0x1234u64.to_le_bytes());
    data[24..32].copy_from_slice(&0x2000u64.to_le_bytes());
    data[32..40].copy_from_slice(&0xe0000u64.to_le_bytes());
    data[40..48].copy_from_slice(&0x3000u64.to_le_bytes());
    data[48..52].copy_from_slice(&12u32.to_le_bytes());
    let info = parse_start_info(&data).unwrap();
    assert_eq!(info.magic, HVM_START_MAGIC);
    assert_eq!(info.version, 1);
    assert_eq!(info.flags, 7);
    assert_eq!(info.nr_modules, 1);
    assert_eq!(info.modlist_paddr, 0x1234);
    assert_eq!(info.cmdline_paddr, 0x2000);
    assert_eq!(info.rsdp_paddr, 0xe0000);
    assert_eq!(info.memmap_paddr, 0x3000);
    assert_eq!(info.memmap_entries, 12);
    assert!(parse_start_info(&data[..55]).is_none());
}

#[test]
fn memmap_entries_convert_types() {
    let mut data = [0u8; HVM_MEMMAP_ENTRY_SIZE * 3];
    for (index, raw_kind) in [1u32, 3, 5].into_iter().enumerate() {
        let offset = index * HVM_MEMMAP_ENTRY_SIZE;
        data[offset..offset + 8].copy_from_slice(&(0x1000u64 * index as u64).to_le_bytes());
        data[offset + 8..offset + 16].copy_from_slice(&0x1000u64.to_le_bytes());
        data[offset + 16..offset + 20].copy_from_slice(&raw_kind.to_le_bytes());
    }
    let mut out = [MemmapEntry {
        addr: 0,
        size: 0,
        kind: MemoryKind::Reserved,
    }; 3];
    let count = parse_memmap_into(&data, 3, &mut out).unwrap();
    assert_eq!(count, 3);
    assert_eq!(out[0].kind, MemoryKind::Usable);
    assert_eq!(out[1].kind, MemoryKind::AcpiReclaimable);
    assert_eq!(out[2].kind, MemoryKind::BadMemory);
    assert!(parse_memmap_into(&data, 4, &mut out).is_err());
    assert!(parse_memmap_into(&data[..10], 1, &mut out).is_err());
}

#[test]
fn module_reads_first_entry() {
    let mut data = [0u8; HVM_MODULE_SIZE * 2];
    data[0..8].copy_from_slice(&0x1f00_0000u64.to_le_bytes());
    data[8..16].copy_from_slice(&0x0090_0000u64.to_le_bytes());
    let module = parse_module(&data).unwrap();
    assert_eq!(module.paddr, 0x1f00_0000);
    assert_eq!(module.size, 0x0090_0000);
    assert!(parse_module(&data[..31]).is_none());
}

#[test]
fn memory_map_reserves_low_memory_and_overlays_tracked() {
    let entries = [
        MemmapEntry {
            addr: 0,
            size: 0x10_0000,
            kind: MemoryKind::Usable,
        },
        MemmapEntry {
            addr: 0x10_0000,
            size: 0x1f0_0000,
            kind: MemoryKind::Usable,
        },
        MemmapEntry {
            addr: 0x2000_0000,
            size: 0x200_0000,
            kind: MemoryKind::Reserved,
        },
    ];
    let tracked = [TrackedRange {
        base: 0x40_0000,
        len: 0x20_0000,
        kind: MemoryKind::KernelAndModules,
    }];
    let mut out = [MemoryRegion {
        base: 0,
        len: 0,
        kind: 0,
        reserved: 0,
    }; 16];
    let count = build_memory_map(&entries, &tracked, &mut out).unwrap();
    let map = &out[..count];
    assert_eq!(map[0].base, 0);
    assert_eq!(map[0].kind(), Some(MemoryKind::Reserved));
    let kernel = map
        .iter()
        .find(|region| region.base == 0x40_0000)
        .expect("kernel range must be present");
    assert_eq!(kernel.kind(), Some(MemoryKind::KernelAndModules));
    assert_eq!(kernel.len, 0x20_0000);
    let mut previous_end = 0;
    for region in map {
        assert_eq!(region.base % 4096, 0);
        assert_eq!(region.len % 4096, 0);
        assert!(region.base >= previous_end);
        previous_end = region.base + region.len;
    }
}

#[test]
fn free_range_avoids_loader_and_module() {
    let entries = [
        MemmapEntry {
            addr: 0,
            size: 0x10_0000,
            kind: MemoryKind::Reserved,
        },
        MemmapEntry {
            addr: 0x10_0000,
            size: 0x1f0_0000,
            kind: MemoryKind::Usable,
        },
    ];
    let avoid = [
        PhysRange {
            base: 0x10_0000,
            len: 0x10_0000,
        },
        PhysRange {
            base: 0x1f00_0000,
            len: 0x100_0000,
        },
    ];
    let found = find_free_range(&entries, 0x11_0000, &avoid).unwrap();
    assert!(found >= 0x20_0000);
    assert!(found + 0x11_0000 <= 0x1f00_0000);
    assert_eq!(found % 0x20_0000, 0);
}
