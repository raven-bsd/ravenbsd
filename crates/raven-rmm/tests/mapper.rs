// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the mapper using a `Vec`-backed physical-memory arena.

use std::cell::UnsafeCell;
use std::collections::{BTreeMap, BTreeSet};

use raven_rmm::page::{PRESENT, USER, WRITABLE};
use raven_rmm::{
    FrameAllocator, MapError, Mapper, PageFlags, PageSize, PageTable, PhysAddr, PhysMemory,
    TlbFlusher, VirtAddr, page_align_down, page_align_up,
};

const FRAME: u64 = 4096;
const ARENA_FRAMES: u64 = 4096;
const ROOT_FRAME: u64 = 1;

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
    free: Vec<u64>,
    allocated: BTreeSet<u64>,
}

impl FakeAlloc {
    fn new(limit: u64) -> Self {
        Self {
            next: ROOT_FRAME,
            limit,
            free: Vec::new(),
            allocated: BTreeSet::new(),
        }
    }

    fn freed_count(&self) -> usize {
        self.free.len()
    }
}

impl FrameAllocator for FakeAlloc {
    fn allocate(&mut self) -> Option<PhysAddr> {
        let frame = match self.free.pop() {
            Some(frame) => frame,
            None if self.next < self.limit => {
                let frame = self.next;
                self.next += 1;
                frame
            }
            None => return None,
        };
        assert!(
            self.allocated.insert(frame),
            "frame {frame} allocated twice"
        );
        Some(PhysAddr(frame * FRAME))
    }

    fn free(&mut self, frame: PhysAddr) {
        assert!(frame.0.is_multiple_of(FRAME));
        let number = frame.0 / FRAME;
        assert!(
            self.allocated.remove(&number),
            "freeing unallocated frame {number}"
        );
        self.free.push(number);
    }
}

#[derive(Default)]
struct FakeTlb {
    flushed: Vec<u64>,
    all: u32,
}

impl TlbFlusher for FakeTlb {
    fn flush(&mut self, virt: VirtAddr) {
        self.flushed.push(virt.data());
    }

    fn flush_all(&mut self) {
        self.all += 1;
    }
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

    fn with<R>(&mut self, f: impl FnOnce(&mut Mapper<'_, FakeAlloc, FakePhys, FakeTlb>) -> R) -> R {
        let mut mapper =
            Mapper::new(self.root, &mut self.alloc, &self.phys, &mut self.tlb).expect("valid root");
        f(&mut mapper)
    }

    fn table(&self) -> PageTable {
        PageTable::new(self.root, 3)
    }
}

fn virt(address: u64) -> VirtAddr {
    VirtAddr::new(address).expect("canonical test address")
}

fn phys(frame: u64) -> PhysAddr {
    PhysAddr(frame * FRAME)
}

#[test]
fn map_and_translate_4k() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_5000_0000);
        assert_eq!(
            mapper.map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K),
            Ok(())
        );
        let (physical, flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(20));
        assert_eq!(size, PageSize::Size4K);
        assert!(flags.is_writable());
        assert!(!flags.is_executable());
    });
}

#[test]
fn translate_unmapped_is_none() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(mapper.translate(virt(0x0000_0000_6000_0000)), None);
    });
}

#[test]
fn map_2m_aligned() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_0000);
        assert_eq!(
            mapper.map(address, phys(512), PageFlags::kernel_rw(), PageSize::Size2M),
            Ok(())
        );
        let (physical, _flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(512));
        assert_eq!(size, PageSize::Size2M);
    });
}

#[test]
fn map_2m_misaligned_is_rejected() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(
            mapper.map(
                virt(0x0000_0000_4000_1000),
                phys(512),
                PageFlags::kernel_rw(),
                PageSize::Size2M
            ),
            Err(MapError::Misaligned)
        );
    });
}

#[test]
fn map_4k_under_2m_conflicts() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let huge = virt(0x0000_0000_4000_0000);
        mapper
            .map(huge, phys(512), PageFlags::kernel_rw(), PageSize::Size2M)
            .unwrap();
        assert_eq!(
            mapper.map(
                virt(huge.data() + 0x2000),
                phys(30),
                PageFlags::kernel_rw(),
                PageSize::Size4K
            ),
            Err(MapError::HugePageConflict)
        );
    });
}

#[test]
fn map_2m_over_4k_conflicts() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(
            mapper.map(address, phys(512), PageFlags::kernel_rw(), PageSize::Size2M),
            Err(MapError::HugePageConflict)
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

#[test]
fn unmap_unmapped_is_rejected() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(
            mapper.unmap(virt(0x0000_0000_7000_0000)),
            Err(MapError::NotMapped)
        );
    });
}

#[test]
fn unmap_4k_returns_physical_base() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_5000_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(mapper.unmap(address), Ok(phys(20)));
        assert_eq!(mapper.translate(address), None);
    });
}

#[test]
fn unmap_2m_returns_physical_base() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_0000);
        mapper
            .map(address, phys(512), PageFlags::kernel_rw(), PageSize::Size2M)
            .unwrap();
        assert_eq!(mapper.unmap(address), Ok(phys(512)));
        assert_eq!(mapper.translate(address), None);
    });
}

#[test]
fn unmap_frees_empty_intermediate_tables() {
    let mut harness = Harness::new(64);
    let address = virt(0x0000_0000_5000_0000);
    harness.with(|mapper| {
        let before = mapper.allocator_mut().freed_count();
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(mapper.allocator_mut().freed_count(), before);
        mapper.unmap(address).unwrap();
        assert_eq!(mapper.allocator_mut().freed_count(), before + 3);
    });
}

#[test]
fn protect_updates_flags() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_5000_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(mapper.protect(address, PageFlags::kernel_ro()), Ok(()));
        let (physical, flags, _) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(20));
        assert!(!flags.is_writable());
        assert!(!flags.is_executable());
    });
}

#[test]
fn protect_rejects_write_execute() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_5000_0000);
        mapper
            .map(address, phys(20), PageFlags::kernel_rw(), PageSize::Size4K)
            .unwrap();
        assert_eq!(
            mapper.protect(address, PageFlags::from_bits(PRESENT | WRITABLE)),
            Err(MapError::WriteExecute)
        );
    });
}

#[test]
fn protect_keeps_huge_bit_and_physical_address() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_0000);
        mapper
            .map(address, phys(512), PageFlags::kernel_rw(), PageSize::Size2M)
            .unwrap();
        mapper.protect(address, PageFlags::kernel_ro()).unwrap();
        let (physical, _flags, size) = mapper.translate(address).expect("mapped");
        assert_eq!(physical, phys(512));
        assert_eq!(size, PageSize::Size2M);
    });
}

#[test]
fn write_execute_map_is_rejected() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(
            mapper.map(
                virt(0x0000_0000_5000_0000),
                phys(20),
                PageFlags::from_bits(PRESENT | WRITABLE),
                PageSize::Size4K
            ),
            Err(MapError::WriteExecute)
        );
    });
}

#[test]
fn map_range_uses_2m_pages() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_0000);
        mapper
            .map_range(address, phys(512), 4 * 1024 * 1024, PageFlags::kernel_rw())
            .unwrap();
        assert_eq!(mapper.translate(address).unwrap().2, PageSize::Size2M);
        assert_eq!(
            mapper
                .translate(virt(address.data() + 2 * 1024 * 1024))
                .unwrap()
                .2,
            PageSize::Size2M
        );
    });
}

#[test]
fn map_range_uses_4k_edges() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_1000);
        mapper
            .map_range(address, phys(512), 0x4000, PageFlags::kernel_rw())
            .unwrap();
        assert_eq!(mapper.translate(address).unwrap().2, PageSize::Size4K);
        assert_eq!(
            mapper.translate(virt(address.data() + 0x2000)).unwrap().2,
            PageSize::Size4K
        );
    });
}

#[test]
fn map_range_zero_length_is_ok() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(
            mapper.map_range(
                virt(0x0000_0000_5000_0000),
                phys(20),
                0,
                PageFlags::kernel_rw()
            ),
            Ok(())
        );
    });
}

#[test]
fn map_range_crosses_a_2m_boundary() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        let address = virt(0x0000_0000_4000_0000);
        mapper
            .map_range(address, phys(512), 3 * 1024 * 1024, PageFlags::kernel_rw())
            .unwrap();
        assert_eq!(mapper.translate(address).unwrap().2, PageSize::Size2M);
        assert_eq!(
            mapper
                .translate(virt(address.data() + 2 * 1024 * 1024))
                .unwrap()
                .2,
            PageSize::Size4K
        );
    });
}

#[test]
fn user_mapping_sets_user_on_every_level() {
    let mut harness = Harness::new(64);
    let address = virt(0x0000_0000_5000_0000);
    harness.with(|mapper| {
        mapper
            .map(address, phys(20), PageFlags::user_rw(), PageSize::Size4K)
            .unwrap();
    });
    let root = harness.table();
    let pml4 = root.entry(&harness.phys, root.index_of(address)).unwrap();
    assert_ne!(pml4.raw() & USER, 0);
    let pdp = PageTable::new(pml4.address(), 2);
    let pdp_entry = pdp.entry(&harness.phys, pdp.index_of(address)).unwrap();
    assert_ne!(pdp_entry.raw() & USER, 0);
    let pd = PageTable::new(pdp_entry.address(), 1);
    let pd_entry = pd.entry(&harness.phys, pd.index_of(address)).unwrap();
    assert_ne!(pd_entry.raw() & USER, 0);
}

#[test]
fn translate_supports_manual_1g_entry() {
    let mut harness = Harness::new(64);
    let address = virt(0x0000_4000_0000_0000);
    let pdp_frame = harness.alloc.allocate().unwrap();
    let root = harness.table();
    let i3 = root.index_of(address);
    root.set_entry(
        &harness.phys,
        i3,
        raven_rmm::Entry::new(pdp_frame, PageFlags::from_bits(PRESENT | WRITABLE)),
    );
    let pdp = PageTable::new(pdp_frame, 2);
    let i2 = pdp.index_of(address);
    pdp.set_entry(
        &harness.phys,
        i2,
        raven_rmm::Entry::new(
            phys(1024),
            PageFlags::from_bits(PRESENT | WRITABLE | raven_rmm::page::HUGE),
        ),
    );
    harness.with(|mapper| {
        let (physical, _flags, size) = mapper.translate(address).unwrap();
        assert_eq!(physical, phys(1024));
        assert_eq!(size, PageSize::Size1G);
    });
}

#[test]
fn new_address_space_copies_the_upper_half() {
    let mut harness = Harness::new(64);
    let root_frame = harness.root;
    let kernel_address = virt(0xFFFF_8000_0000_0000);
    let new_root = harness.with(|mapper| {
        mapper
            .map(
                kernel_address,
                phys(20),
                PageFlags::kernel_rw(),
                PageSize::Size4K,
            )
            .unwrap();
        mapper.new_address_space(root_frame).unwrap()
    });
    let source = PageTable::new(root_frame, 3);
    let destination = PageTable::new(new_root, 3);
    for index in 0..256 {
        assert_eq!(destination.entry(&harness.phys, index).unwrap().raw(), 0);
    }
    for index in 256..512 {
        assert_eq!(
            destination.entry(&harness.phys, index).unwrap().raw(),
            source.entry(&harness.phys, index).unwrap().raw()
        );
    }
}

#[test]
fn new_address_space_reports_out_of_frames() {
    let mut harness = Harness::new(2);
    let root_frame = harness.root;
    harness.with(|mapper| {
        assert_eq!(
            mapper.new_address_space(root_frame),
            Err(MapError::OutOfFrames)
        );
    });
}

#[test]
fn map_reports_out_of_frames() {
    let mut harness = Harness::new(2);
    harness.with(|mapper| {
        assert_eq!(
            mapper.map(
                virt(0x0000_0000_5000_0000),
                phys(20),
                PageFlags::kernel_rw(),
                PageSize::Size4K
            ),
            Err(MapError::OutOfFrames)
        );
    });
}

#[test]
fn physical_width_is_checked() {
    let mut harness = Harness::new(64);
    harness.with(|mapper| {
        assert_eq!(
            mapper.map(
                virt(0x0000_0000_5000_0000),
                PhysAddr(1 << 52),
                PageFlags::kernel_rw(),
                PageSize::Size4K
            ),
            Err(MapError::NotCanonical)
        );
    });
}

#[test]
fn virt_addr_rejects_non_canonical() {
    assert!(VirtAddr::new(0x0000_8000_0000_0000).is_none());
    assert!(VirtAddr::new(0xFFFF_7FFF_FFFF_FFFF).is_none());
    assert!(VirtAddr::new(0xFFFF_8000_0000_0000).is_some());
}

#[test]
fn page_alignment_helpers() {
    assert_eq!(page_align_down(0x1234), 0x1000);
    assert_eq!(page_align_up(0x1234), Some(0x2000));
    assert_eq!(page_align_up(u64::MAX), None);
}

#[test]
fn page_flag_helpers() {
    assert!(PageFlags::kernel_rx().is_executable());
    assert!(!PageFlags::kernel_rx().is_writable());
    assert!(PageFlags::kernel_rw().is_writable());
    assert!(!PageFlags::kernel_rw().is_executable());
    assert!(PageFlags::user_rw().is_user());
    assert!(!PageFlags::kernel_rw().is_user());
    assert!(PageFlags::kernel_rw().is_global());
    assert!(!PageFlags::user_rw().is_global());
}

#[test]
fn mmio_flags_are_uncached_and_non_executable() {
    let flags = PageFlags::mmio();
    assert!(flags.is_present());
    assert!(flags.is_writable());
    assert!(!flags.is_executable());
    assert_ne!(flags.bits() & raven_rmm::page::CACHE_DISABLE, 0);
    assert_ne!(flags.bits() & raven_rmm::page::WRITE_THROUGH, 0);
}

#[test]
fn mapper_rejects_misaligned_root() {
    let mut harness = Harness::new(64);
    let mut tlb = FakeTlb::default();
    assert_eq!(
        Mapper::new(
            PhysAddr(harness.root.0 + 1),
            &mut harness.alloc,
            &harness.phys,
            &mut tlb
        )
        .err(),
        Some(MapError::Misaligned)
    );
}

#[test]
fn random_map_unmap_matches_model() {
    let mut harness = Harness::new(1024);
    let base = 0x0000_0000_5000_0000u64;
    let slots = 512u64;
    let mut model: BTreeMap<u64, PhysAddr> = BTreeMap::new();
    let mut state = 0x1234_5678_9ABC_DEF0u64;
    let mut next_frame = 100u64;

    harness.with(|mapper| {
        for _ in 0..2000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let slot = state % slots;
            let address = virt(base + slot * FRAME);
            if state & 1 == 0 {
                let frame = next_frame;
                next_frame += 1;
                let expected = model.get(&address.data()).copied();
                let result = mapper.map(
                    address,
                    phys(frame),
                    PageFlags::kernel_rw(),
                    PageSize::Size4K,
                );
                match expected {
                    None => {
                        assert_eq!(result, Ok(()));
                        model.insert(address.data(), phys(frame));
                    }
                    Some(_) => assert_eq!(result, Err(MapError::AlreadyMapped)),
                }
            } else {
                let expected = model.remove(&address.data());
                match expected {
                    Some(physical) => assert_eq!(mapper.unmap(address), Ok(physical)),
                    None => assert_eq!(mapper.unmap(address), Err(MapError::NotMapped)),
                }
            }
            assert_eq!(
                mapper.translate(address).map(|value| value.0),
                model.get(&address.data()).copied()
            );
        }
    });
}
