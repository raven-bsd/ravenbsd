// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the aarch64 descriptor encoding and the encoding-generic mapper.
//!
//! The physical-memory arena is the same pattern as `tests/mapper.rs`; every descriptor bit is
//! asserted explicitly against the ARMv8-A 4 KiB-granule format documented in
//! `src/arch/aarch64.rs`.

use std::cell::UnsafeCell;
use std::collections::BTreeSet;

use raven_rmm::arch::aarch64::{
    AF, AP_ALL_RO, AP_ALL_RW, AP_EL1_RO, AP_EL1_RW, ATTR_DEVICE, ATTR_DEVICE_NGNRE,
    ATTR_NON_CACHEABLE, ATTR_NORMAL, ENTRY_TABLE_OR_PAGE, ENTRY_VALID, NG, PHYS_ADDR_MASK, PXN,
    SH_INNER, UXN, is_canonical,
};
use raven_rmm::page::{CACHE_DISABLE, GLOBAL, NO_EXECUTE, NON_CACHEABLE, PRESENT, USER, WRITABLE};
use raven_rmm::{
    Aarch64Encoding, Encoding, FrameAllocator, MapError, Mapper, PageFlags, PageSize, PhysAddr,
    PhysMemory, TlbFlusher, VirtAddr,
};

const FRAME: u64 = 4096;
const ARENA_FRAMES: u64 = 4096;

struct FakePhys {
    memory: UnsafeCell<Box<[u8]>>,
}

impl FakePhys {
    fn new(frames: u64) -> Self {
        Self {
            memory: UnsafeCell::new(vec![0u8; (frames * FRAME) as usize].into_boxed_slice()),
        }
    }
}

impl PhysMemory for FakePhys {
    fn phys_to_virt(&self, phys: PhysAddr) -> *mut u8 {
        assert!(phys.0.is_multiple_of(FRAME));
        assert!(phys.0 / FRAME < ARENA_FRAMES);
        // SAFETY: the arena is a stable boxed slice and UnsafeCell permits writes through a
        // shared reference; the physical address was bounds checked above.
        unsafe { (*self.memory.get()).as_mut_ptr().add(phys.0 as usize) }
    }
}

struct FakeAlloc {
    next: u64,
    limit: u64,
    allocated: BTreeSet<u64>,
}

impl FakeAlloc {
    fn new(limit: u64) -> Self {
        Self {
            next: 1,
            limit,
            allocated: BTreeSet::new(),
        }
    }
}

impl FrameAllocator for FakeAlloc {
    fn allocate(&mut self) -> Option<PhysAddr> {
        if self.next >= self.limit {
            return None;
        }
        let frame = self.next;
        self.next += 1;
        assert!(self.allocated.insert(frame));
        Some(PhysAddr(frame * FRAME))
    }

    fn free(&mut self, frame: PhysAddr) {
        assert!(frame.0.is_multiple_of(FRAME));
        assert!(self.allocated.remove(&(frame.0 / FRAME)));
    }
}

#[derive(Default)]
struct FakeTlb {
    flushed: Vec<u64>,
}

impl TlbFlusher for FakeTlb {
    fn flush(&mut self, virt: VirtAddr) {
        self.flushed.push(virt.data());
    }

    fn flush_all(&mut self) {}
}

struct Harness {
    phys: FakePhys,
    alloc: FakeAlloc,
    tlb: FakeTlb,
    root: PhysAddr,
}

impl Harness {
    fn new(limit: u64) -> Self {
        let phys = FakePhys::new(ARENA_FRAMES);
        let mut alloc = FakeAlloc::new(limit);
        let root = alloc.allocate().expect("root frame");
        Self {
            phys,
            alloc,
            tlb: FakeTlb::default(),
            root,
        }
    }

    fn with<R>(
        &mut self,
        f: impl FnOnce(&mut Mapper<'_, FakeAlloc, FakePhys, FakeTlb, Aarch64Encoding>) -> R,
    ) -> R {
        let mut mapper =
            Mapper::with_encoding(self.root, &mut self.alloc, &self.phys, &mut self.tlb)
                .expect("valid root");
        f(&mut mapper)
    }
}

fn virt(address: u64) -> VirtAddr {
    VirtAddr::new(address).expect("canonical test address")
}

fn phys(frame: u64) -> PhysAddr {
    PhysAddr(frame * FRAME)
}

/// The exact descriptor for a kernel 4 KiB RW page at `phys`.
///
/// Kernel data is execute-never at both privilege levels (UXN and PXN).
fn kernel_rw_page(phys: PhysAddr) -> u64 {
    phys.0 | ENTRY_VALID | ENTRY_TABLE_OR_PAGE | AF | SH_INNER | ATTR_NORMAL | AP_EL1_RW | UXN | PXN
}

// --- Descriptor bits ---------------------------------------------------------

#[test]
fn kernel_rw_page_bits() {
    let descriptor = Aarch64Encoding::leaf(phys(20), PageFlags::kernel_rw(), PageSize::Size4K);
    assert_eq!(descriptor, kernel_rw_page(phys(20)));
    assert_eq!(descriptor & 0b111 << 2, ATTR_NORMAL);
    assert_eq!((descriptor >> 6) & 0b11, 0b00);
    assert_eq!(descriptor & UXN, UXN);
    assert_eq!(
        descriptor & PXN,
        PXN,
        "kernel RW data is execute-never at both levels"
    );
    assert_eq!(descriptor & NG, 0, "kernel pages are global");
}

#[test]
fn kernel_rx_page_bits() {
    let descriptor = Aarch64Encoding::leaf(phys(21), PageFlags::kernel_rx(), PageSize::Size4K);
    assert_eq!(
        descriptor,
        phys(21).0
            | ENTRY_VALID
            | ENTRY_TABLE_OR_PAGE
            | AF
            | SH_INNER
            | ATTR_NORMAL
            | AP_EL1_RO
            | UXN
    );
    assert_eq!(descriptor & PXN, 0, "kernel text executes at EL1");
}

#[test]
fn kernel_ro_page_bits() {
    let flags = PageFlags::kernel_ro();
    let descriptor = Aarch64Encoding::leaf(phys(22), flags, PageSize::Size4K);
    assert_eq!(
        descriptor,
        phys(22).0
            | ENTRY_VALID
            | ENTRY_TABLE_OR_PAGE
            | AF
            | SH_INNER
            | ATTR_NORMAL
            | AP_EL1_RO
            | UXN
            | PXN
    );
}

#[test]
fn user_rw_page_bits() {
    let descriptor = Aarch64Encoding::leaf(phys(23), PageFlags::user_rw(), PageSize::Size4K);
    assert_eq!(
        descriptor,
        phys(23).0
            | ENTRY_VALID
            | ENTRY_TABLE_OR_PAGE
            | AF
            | SH_INNER
            | ATTR_NORMAL
            | AP_ALL_RW
            | PXN
            | UXN
            | NG
    );
    assert_eq!(descriptor & NG, NG, "user pages are not global");
}

#[test]
fn user_rx_page_bits() {
    let descriptor = Aarch64Encoding::leaf(phys(24), PageFlags::user_rx(), PageSize::Size4K);
    assert_eq!(
        descriptor,
        phys(24).0
            | ENTRY_VALID
            | ENTRY_TABLE_OR_PAGE
            | AF
            | SH_INNER
            | ATTR_NORMAL
            | AP_ALL_RO
            | PXN
            | NG
    );
    assert_eq!(descriptor & UXN, 0, "user code executes at EL0");
}

#[test]
fn user_ro_page_bits() {
    let descriptor = Aarch64Encoding::leaf(phys(25), PageFlags::user_ro(), PageSize::Size4K);
    assert_eq!(
        descriptor,
        phys(25).0
            | ENTRY_VALID
            | ENTRY_TABLE_OR_PAGE
            | AF
            | SH_INNER
            | ATTR_NORMAL
            | AP_ALL_RO
            | PXN
            | UXN
            | NG
    );
}

#[test]
fn kernel_writable_execute_combinations() {
    // Every write/execute combination for kernel pages: W!X enforced by the mapper, and the
    // encoder always sets the XN bit for the privilege level that cannot execute.
    for (writable, executable, expected_ap, expected_pxn, expected_uxn) in [
        (false, false, AP_EL1_RO, PXN, UXN),
        (false, true, AP_EL1_RO, 0, UXN),
        (true, false, AP_EL1_RW, PXN, UXN),
    ] {
        let mut flags = PageFlags::kernel_rx();
        flags.remove(PageFlags::from_bits(PRESENT | WRITABLE | NO_EXECUTE));
        if writable {
            flags.insert(PageFlags::from_bits(WRITABLE));
        }
        if !executable {
            flags.insert(PageFlags::from_bits(NO_EXECUTE));
        }
        let descriptor = Aarch64Encoding::leaf(phys(26), flags, PageSize::Size4K);
        assert_eq!(
            (descriptor >> 6) & 0b11,
            (expected_ap >> 6) & 0b11,
            "writable={writable} executable={executable}"
        );
        assert_eq!(descriptor & PXN, expected_pxn);
        assert_eq!(descriptor & UXN, expected_uxn);
    }
}

#[test]
fn mmio_selects_device_ngnre() {
    let descriptor = Aarch64Encoding::leaf(phys(27), PageFlags::mmio(), PageSize::Size4K);
    assert_eq!(descriptor & 0b111 << 2, ATTR_DEVICE_NGNRE);
    // mmio is RW and execute-never at EL1.
    assert_eq!((descriptor >> 6) & 0b11, 0b00);
    assert_eq!(descriptor & PXN, PXN);
    assert_eq!(descriptor & UXN, UXN);
}

#[test]
fn cache_disable_alone_selects_device_ngnrne() {
    let flags = PageFlags::from_bits(PRESENT | WRITABLE | GLOBAL | NO_EXECUTE | CACHE_DISABLE);
    let descriptor = Aarch64Encoding::leaf(phys(28), flags, PageSize::Size4K);
    assert_eq!(descriptor & 0b111 << 2, ATTR_DEVICE);
}

#[test]
fn non_cacheable_selects_attr2() {
    let flags = PageFlags::from_bits(PRESENT | WRITABLE | NO_EXECUTE | NON_CACHEABLE);
    let descriptor = Aarch64Encoding::leaf(phys(29), flags, PageSize::Size4K);
    assert_eq!(descriptor & 0b111 << 2, ATTR_NON_CACHEABLE);
}

#[test]
fn block_descriptor_clears_the_page_bit() {
    let page = Aarch64Encoding::leaf(phys(30), PageFlags::kernel_rw(), PageSize::Size4K);
    let block = Aarch64Encoding::leaf(phys(30), PageFlags::kernel_rw(), PageSize::Size2M);
    assert_eq!(page & ENTRY_TABLE_OR_PAGE, ENTRY_TABLE_OR_PAGE);
    assert_eq!(block & ENTRY_TABLE_OR_PAGE, 0);
}

#[test]
fn table_descriptor_bits() {
    for user in [false, true] {
        let descriptor = Aarch64Encoding::table(phys(31), user);
        assert_eq!(
            descriptor,
            phys(31).0 | ENTRY_VALID | ENTRY_TABLE_OR_PAGE,
            "user={user}"
        );
    }
}

#[test]
fn address_mask_round_trip() {
    // The highest encodable physical address and a mid-range one survive encode/decode.
    for address in [0u64, 0x1234_5000, PHYS_ADDR_MASK & !0xFFF] {
        let descriptor =
            Aarch64Encoding::leaf(PhysAddr(address), PageFlags::kernel_rw(), PageSize::Size4K);
        let (decoded, flags, size) = Aarch64Encoding::decode(descriptor, 0);
        assert_eq!(decoded.0, address);
        assert_eq!(size, PageSize::Size4K);
        assert!(flags.is_writable());
        assert!(!flags.is_executable());
    }
}

#[test]
fn decode_restores_user_flags() {
    let descriptor = Aarch64Encoding::leaf(phys(32), PageFlags::user_rw(), PageSize::Size4K);
    let (_address, flags, _size) = Aarch64Encoding::decode(descriptor, 0);
    assert!(flags.is_user());
    assert!(flags.is_writable());
    assert!(!flags.is_executable());
    assert!(!flags.is_global());

    let descriptor = Aarch64Encoding::leaf(phys(33), PageFlags::user_rx(), PageSize::Size4K);
    let (_address, flags, _size) = Aarch64Encoding::decode(descriptor, 0);
    assert!(flags.is_user());
    assert!(!flags.is_writable());
    assert!(flags.is_executable());
}

#[test]
fn decode_block_size_follows_the_level() {
    let block = Aarch64Encoding::leaf(phys(34), PageFlags::kernel_rw(), PageSize::Size2M);
    let (_address, _flags, size) = Aarch64Encoding::decode(block, 1);
    assert_eq!(size, PageSize::Size2M);
    // The same raw bits at L0 would be a 4 KiB page (bit 1 is a must-be-one there).
    let (_address, _flags, size) = Aarch64Encoding::decode(block | ENTRY_TABLE_OR_PAGE, 0);
    assert_eq!(size, PageSize::Size4K);
}

#[test]
fn is_table_distinguishes_blocks() {
    let table = Aarch64Encoding::table(phys(35), false);
    let block = Aarch64Encoding::leaf(phys(35), PageFlags::kernel_rw(), PageSize::Size2M);
    assert!(Aarch64Encoding::is_table(table, 1));
    assert!(!Aarch64Encoding::is_table(block, 1));
    // L3 entries are always leaves.
    assert!(!Aarch64Encoding::is_table(table, 0));
}

#[test]
fn canonical_ttbr0_ttbr1_ranges() {
    // Full 48-bit lower half (TTBR0) and full upper half (TTBR1).
    for address in [
        0u64,
        0x0000_7FFF_FFFF_FFFF,
        0x0000_8000_0000_0000,
        0x0000_FFFF_FFFF_FFFF,
        0xFFFF_0000_0000_0000,
        0xFFFF_FFFF_FFFF_FFFF,
    ] {
        assert!(is_canonical(address), "{address:#x}");
        assert!(Aarch64Encoding::is_canonical(address), "{address:#x}");
    }
    for address in [0x8000_0000_0000_0000, 0x7FFF_0000_0000_0000] {
        assert!(!Aarch64Encoding::is_canonical(address), "{address:#x}");
    }
}

// --- Mapper over Aarch64Encoding ----------------------------------------------

#[test]
fn map_and_translate_ttbr1_page() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0xFFFF_FFFF_8020_0000);
        assert_eq!(
            mapper.map(address, phys(20), PageFlags::kernel_rx(), PageSize::Size4K),
            Ok(())
        );
        let (physical, flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(20));
        assert_eq!(size, PageSize::Size4K);
        assert!(flags.is_executable());
        assert!(!flags.is_writable());
    });
}

#[test]
fn map_and_translate_2m_block() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0xFFFF_8000_4000_0000);
        assert_eq!(
            mapper.map(address, phys(512), PageFlags::kernel_rw(), PageSize::Size2M),
            Ok(())
        );
        let (physical, flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(512));
        assert_eq!(size, PageSize::Size2M);
        assert!(flags.is_writable());
        assert!(!flags.is_executable());
    });
}

#[test]
fn map_range_prefers_blocks() {
    let mut harness = Harness::new(512);
    harness.with(|mapper| {
        // 4 MiB + 4 KiB starting at a 2 MiB boundary: two blocks plus one page.
        let address = virt(0xFFFF_8000_4000_0000);
        assert_eq!(
            mapper.map_range(
                address,
                phys(512),
                4 * 1024 * 1024 + 4096,
                PageFlags::kernel_rw()
            ),
            Ok(())
        );
        let (_, _, head) = mapper.translate(address).expect("mapped");
        assert_eq!(head, PageSize::Size2M);
        let (_, _, tail) = mapper
            .translate(virt(address.data() + 4 * 1024 * 1024))
            .expect("tail");
        assert_eq!(tail, PageSize::Size4K);
    });
}

#[test]
fn writable_executable_is_rejected() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let writable_executable = PageFlags::from_bits(PRESENT | WRITABLE | USER);
        assert_eq!(
            mapper.map(
                virt(0x0000_0000_5000_0000),
                phys(20),
                writable_executable,
                PageSize::Size4K
            ),
            Err(MapError::WriteExecute)
        );
        assert_eq!(
            mapper.map_range(
                virt(0x0000_0000_5000_0000),
                phys(20),
                4096,
                writable_executable
            ),
            Err(MapError::WriteExecute)
        );
        // The W^X check runs before the mapping lookup, as on x86-64.
        assert_eq!(
            mapper.protect(virt(0x0000_0000_5000_0000), writable_executable),
            Err(MapError::WriteExecute)
        );
    });
}

#[test]
fn protect_preserves_address_and_shape() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0xFFFF_8000_4000_0000);
        mapper
            .map(address, phys(512), PageFlags::kernel_rw(), PageSize::Size2M)
            .unwrap();
        assert_eq!(mapper.protect(address, PageFlags::kernel_ro()), Ok(()));
        let (physical, flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(512), "protect must not move the mapping");
        assert_eq!(size, PageSize::Size2M, "protect must keep the block shape");
        assert!(!flags.is_writable());
        assert!(!flags.is_executable());
    });
}

#[test]
fn unmap_then_remap() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0xFFFF_FFFF_8020_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(mapper.unmap(address), Ok(phys(20)));
        assert_eq!(mapper.translate(address), None);
        assert_eq!(
            mapper.map(address, phys(21), PageFlags::kernel_ro(), PageSize::Size4K),
            Ok(())
        );
        let (remapped, _flags, _size) = mapper.translate(address).expect("remapped");
        assert_eq!(remapped, phys(21));
    });
}

#[test]
fn page_over_block_conflicts() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let block = virt(0xFFFF_8000_4000_0000);
        mapper
            .map(block, phys(512), PageFlags::kernel_rw(), PageSize::Size2M)
            .unwrap();
        assert_eq!(
            mapper.map(
                virt(block.data() + 0x2000),
                phys(30),
                PageFlags::kernel_rw(),
                PageSize::Size4K
            ),
            Err(MapError::HugePageConflict)
        );
    });
}

#[test]
fn unmap_of_unmapped_is_rejected() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(
            mapper.unmap(virt(0x0000_0000_6000_0000)),
            Err(MapError::NotMapped)
        );
    });
}

#[test]
fn physical_address_above_48_bits_is_rejected() {
    let mut harness = Harness::new(4096 * 2);
    harness.with(|mapper| {
        let too_high = PhysAddr(1 << 48);
        assert_eq!(
            mapper.map(
                virt(0x0000_0000_5000_0000),
                too_high,
                PageFlags::kernel_rw(),
                PageSize::Size4K
            ),
            Err(MapError::NotCanonical)
        );
    });
}

#[test]
fn double_map_is_rejected() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_5000_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(
            mapper.map(address, phys(21), PageFlags::kernel_rw(), PageSize::Size4K),
            Err(MapError::AlreadyMapped)
        );
    });
}
