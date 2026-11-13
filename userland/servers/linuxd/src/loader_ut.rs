// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Loader tests with a recording `Sys` implementation.

use raven_abi::types::{Handle, map, prot};

use super::*;
use crate::image::{Config, plan_image};

#[derive(Debug, Default)]
struct MapCall {
    proc: u32,
    object: u32,
    offset: usize,
    address: usize,
    len: usize,
    flags: usize,
}

#[derive(Debug, Default)]
struct Mock {
    next_memory: u32,
    next_address: usize,
    proc: Option<Handle>,
    maps: Vec<MapCall>,
    protects: Vec<(usize, usize, u8)>,
    writes: Vec<(usize, usize)>,
    creations: Vec<usize>,
    closed: Vec<u32>,
}

impl Mock {
    fn new() -> Self {
        Self {
            next_memory: 100,
            next_address: 0x7000_0000,
            ..Self::default()
        }
    }
}

impl Sys for Mock {
    fn proc_create(&mut self) -> Result<Handle, raven_abi::errno::Errno> {
        let handle = Handle(7);
        self.proc = Some(handle);
        Ok(handle)
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, raven_abi::errno::Errno> {
        self.creations.push(len);
        self.next_memory += 1;
        Ok(Handle(self.next_memory))
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, raven_abi::errno::Errno> {
        self.maps.push(MapCall {
            proc: proc.0,
            object: memobj.0,
            offset,
            address: addr,
            len,
            flags,
        });
        if addr == 0 && proc == Handle::SELF_PROC {
            self.next_address += 0x1000;
            Ok(self.next_address - 0x1000)
        } else {
            Ok(addr)
        }
    }

    fn unmap(
        &mut self,
        _proc: Handle,
        _addr: usize,
        _len: usize,
    ) -> Result<(), raven_abi::errno::Errno> {
        Ok(())
    }

    fn protect(
        &mut self,
        _proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), raven_abi::errno::Errno> {
        self.protects.push((addr, len, protection));
        Ok(())
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), raven_abi::errno::Errno> {
        self.closed.push(handle.0);
        Ok(())
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        self.writes.push((addr, bytes.len()));
    }
}

fn image() -> Vec<u8> {
    let mut image = vec![0u8; 0x2000];
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    image[16..18].copy_from_slice(&2u16.to_le_bytes());
    image[18..20].copy_from_slice(&62u16.to_le_bytes());
    image[20..24].copy_from_slice(&1u32.to_le_bytes());
    image[24..32].copy_from_slice(&0x400080u64.to_le_bytes());
    image[32..40].copy_from_slice(&64u64.to_le_bytes());
    image[52..54].copy_from_slice(&64u16.to_le_bytes());
    image[54..56].copy_from_slice(&56u16.to_le_bytes());
    image[56..58].copy_from_slice(&2u16.to_le_bytes());
    let phdrs = [
        (5u32, 0u64, 0x40_0000u64, 0x1000u64, 0x1000u64),
        (6u32, 0x1000u64, 0x40_1000u64, 0x100u64, 0x300u64),
    ];
    for (index, (flags, offset, vaddr, filesz, memsz)) in phdrs.iter().enumerate() {
        let base = 64 + index * 56;
        image[base..base + 4].copy_from_slice(&1u32.to_le_bytes());
        image[base + 4..base + 8].copy_from_slice(&flags.to_le_bytes());
        image[base + 8..base + 16].copy_from_slice(&offset.to_le_bytes());
        image[base + 16..base + 24].copy_from_slice(&vaddr.to_le_bytes());
        image[base + 32..base + 40].copy_from_slice(&filesz.to_le_bytes());
        image[base + 40..base + 48].copy_from_slice(&memsz.to_le_bytes());
        image[base + 48..base + 56].copy_from_slice(&0x1000u64.to_le_bytes());
    }
    image
}

fn config() -> Config {
    Config {
        main_pie_base: 0,
        interp_base: 0,
        page: 4096,
        uid: 0,
        euid: 0,
        gid: 0,
        egid: 0,
        secure: false,
        hwcap: 0,
        platform: "x86_64",
        clktck: 100,
        sysinfo_ehdr: None,
        random: [7u8; 16],
    }
}

#[test]
fn the_child_gets_every_segment_and_the_arenas() {
    let image = image();
    let spec = plan_image(
        &image,
        &config(),
        &[b"hello".as_slice()],
        &[],
        b"/hello",
        0x0000_7FFF_FFFF_F000,
    )
    .expect("plan");
    let mut mock = Mock::new();
    let loaded = load(&spec, &image, &mut mock).expect("load");
    assert_eq!(loaded.proc, Handle(7));
    assert_eq!(loaded.entry, 0x40_0080);
    assert_eq!(loaded.brk, 0x40_2000);
    assert_eq!(loaded.brk_limit, 0x40_2000 + BRK_SIZE);
    assert_eq!(loaded.mmap, MMAP_BASE);

    let child_maps: Vec<&MapCall> = mock.maps.iter().filter(|call| call.proc == 7).collect();
    // text (1 page), data tail (1 page), stack, two arenas
    assert_eq!(child_maps.len(), 5);
    assert_eq!(child_maps[0].address, 0x40_0000);
    assert_eq!(child_maps[0].offset, 0);
    assert_eq!(child_maps[0].len, 0x1000);
    assert_eq!(child_maps[0].flags & prot::MASK, prot::READ | prot::WRITE);
    // The data tail is staged in its own page object, not mapped from the image object.
    assert_ne!(child_maps[1].object, child_maps[0].object);
    assert!(
        mock.protects
            .iter()
            .any(|(addr, _, prot)| *addr == 0x40_0000 && *prot == 5)
    );
    assert_eq!(child_maps[2].address as u64, loaded.sp - (loaded.sp % PAGE));
    assert_eq!(
        child_maps[2].flags & (map::FIXED << map::SHIFT),
        map::FIXED << map::SHIFT
    );
    assert!(
        mock.maps
            .iter()
            .any(|call| call.proc == 7 && call.address as u64 == BRK_BASE)
    );
    assert!(
        mock.maps
            .iter()
            .any(|call| call.proc == 7 && call.address as u64 == MMAP_BASE)
    );
}

#[test]
fn the_regions_record_what_the_trap_loop_may_copy_out_of() {
    let image = image();
    let spec = plan_image(
        &image,
        &config(),
        &[b"hello".as_slice()],
        &[],
        b"/hello",
        0x0000_7FFF_FFFF_F000,
    )
    .expect("plan");
    let mut mock = Mock::new();
    let loaded = load(&spec, &image, &mut mock).expect("load");
    assert_eq!(loaded.regions.len(), 5);
    assert!(
        loaded
            .regions
            .iter()
            .any(|region| region.start == 0x40_0000 && region.backing != Handle::NONE)
    );
    assert!(
        loaded
            .regions
            .iter()
            .any(|region| region.start == BRK_BASE && region.len == BRK_SIZE)
    );
    assert!(
        loaded
            .regions
            .iter()
            .any(|region| region.start == MMAP_BASE && region.len == MMAP_SIZE)
    );
    let stack = loaded
        .regions
        .iter()
        .find(|region| region.start <= loaded.sp && loaded.sp < region.start + region.len)
        .expect("the stack is inside a region");
    assert!(stack.backing != Handle::NONE);
}

#[test]
fn the_stack_and_image_are_written_through_the_scratch_mapping() {
    let image = image();
    let spec = plan_image(
        &image,
        &config(),
        &[b"hello".as_slice()],
        &[],
        b"/hello",
        0x0000_7FFF_FFFF_F000,
    )
    .expect("plan");
    let mut mock = Mock::new();
    let _ = load(&spec, &image, &mut mock).expect("load");
    // image bytes, data tail, stack bytes
    assert_eq!(mock.writes.len(), 3);
    assert_eq!(mock.writes[0].1, image.len());
    assert_eq!(mock.writes[1].1, 0x100);
    assert_eq!(mock.writes[2].1, spec.stack.bytes.len());
    assert!(
        mock.creations
            .iter()
            .any(|len| *len == spec.stack.bytes.len().div_ceil(PAGE as usize) * PAGE as usize)
    );
}
