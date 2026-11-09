// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! mmap/munmap/mprotect/mremap/brk edge cases.

mod common;

use common::MockHost;
use linux_shim::Mm;
use raven_abi::errno::Errno;
use raven_linux_abi::flags::mmap::{
    MAP_ANONYMOUS, MAP_FIXED, MAP_FIXED_NOREPLACE, MAP_PRIVATE, MAP_SHARED, PROT_EXEC, PROT_NONE,
    PROT_READ, PROT_WRITE,
};

const PAGE: usize = 4096;
const MMAP_BASE: usize = 0x4000_0000;
const MMAP_END: usize = 0x4001_0000;
const BRK_START: usize = 0x1000_0000;

fn mm() -> Mm {
    Mm::new(MMAP_BASE, MMAP_END, BRK_START)
}

fn host() -> MockHost {
    MockHost::new()
}

fn map(
    mm: &mut Mm,
    host: &mut MockHost,
    addr: usize,
    len: usize,
    prot: u32,
) -> Result<usize, Errno> {
    mm.mmap(host, addr, len, prot, MAP_ANONYMOUS | MAP_PRIVATE, -1, 0)
}

fn fixed_map(
    mm: &mut Mm,
    host: &mut MockHost,
    addr: usize,
    len: usize,
    prot: u32,
) -> Result<usize, Errno> {
    mm.mmap(
        host,
        addr,
        len,
        prot,
        MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED,
        -1,
        0,
    )
}

#[test]
fn mmap_rejects_zero_length() {
    assert_eq!(
        map(&mut mm(), &mut host(), 0, 0, PROT_READ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mmap_rejects_misaligned_hint() {
    assert_eq!(
        map(&mut mm(), &mut host(), MMAP_BASE + 1, PAGE, PROT_READ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mmap_rounds_length_up_to_a_page() {
    let mut mm = mm();
    let mut host = host();
    let addr = map(&mut mm, &mut host, MMAP_BASE, PAGE + 1, PROT_READ).unwrap();
    assert_eq!(addr % PAGE, 0);
    assert_eq!(host.maps.get(&addr), Some(&(2 * PAGE, PROT_READ)));
}

#[test]
fn mmap_rejects_write_and_execute() {
    assert_eq!(
        map(&mut mm(), &mut host(), 0, PAGE, PROT_WRITE | PROT_EXEC),
        Err(Errno::EACCES)
    );
}

#[test]
fn mmap_rejects_file_backed_mappings() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mmap(&mut host, 0, PAGE, PROT_READ, MAP_PRIVATE, 0, 0),
        Err(Errno::ENODEV)
    );
}

#[test]
fn mmap_rejects_shared_anonymous_mappings() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mmap(
            &mut host,
            0,
            PAGE,
            PROT_READ,
            MAP_ANONYMOUS | MAP_SHARED,
            -1,
            0
        ),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn mmap_fixed_needs_an_address() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mmap(
            &mut host,
            0,
            PAGE,
            PROT_READ,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED,
            -1,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mmap_fixed_maps_the_exact_address() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    assert_eq!(
        fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap(),
        address
    );
    assert!(host.mapped(address, PAGE));
}

#[test]
fn mmap_fixed_replaces_an_existing_mapping() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    assert_eq!(
        fixed_map(&mut mm, &mut host, address, PAGE, PROT_WRITE).unwrap(),
        address
    );
    assert_eq!(host.maps.get(&address), Some(&(PAGE, PROT_WRITE)));
}

#[test]
fn mmap_fixed_noreplace_rejects_overlap() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mmap(
            &mut host,
            address,
            PAGE,
            PROT_READ,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED_NOREPLACE,
            -1,
            0
        ),
        Err(Errno::EEXIST)
    );
}

#[test]
fn mmap_fixed_noreplace_maps_a_free_address() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    assert_eq!(
        mm.mmap(
            &mut host,
            address,
            PAGE,
            PROT_READ,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED_NOREPLACE,
            -1,
            0
        )
        .unwrap(),
        address
    );
}

#[test]
fn mmap_uses_a_free_hint() {
    let mut mm = mm();
    let mut host = host();
    let hint = 0x4000_8000;
    assert_eq!(
        map(&mut mm, &mut host, hint, PAGE, PROT_READ).unwrap(),
        hint
    );
}

#[test]
fn mmap_falls_back_when_the_hint_overlaps() {
    let mut mm = mm();
    let mut host = host();
    let hint = 0x4000_8000;
    fixed_map(&mut mm, &mut host, hint, PAGE, PROT_READ).unwrap();
    let address = map(&mut mm, &mut host, hint, PAGE, PROT_READ).unwrap();
    assert_ne!(address, hint);
    assert_eq!(address, MMAP_END - PAGE);
}

#[test]
fn mmap_places_without_a_hint_top_down() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        map(&mut mm, &mut host, 0, PAGE, PROT_READ).unwrap(),
        MMAP_END - PAGE
    );
    assert_eq!(
        map(&mut mm, &mut host, 0, PAGE, PROT_READ).unwrap(),
        MMAP_END - 2 * PAGE
    );
}

#[test]
fn mmap_reports_exhaustion() {
    let mut mm = Mm::new(0x10000, 0x12000, 0x1000);
    let mut host = host();
    map(&mut mm, &mut host, 0, PAGE, PROT_READ).unwrap();
    map(&mut mm, &mut host, 0, PAGE, PROT_READ).unwrap();
    assert_eq!(
        map(&mut mm, &mut host, 0, PAGE, PROT_READ),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn mmap_rejects_both_fixed_flags() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mmap(
            &mut host,
            0x4000_8000,
            PAGE,
            PROT_READ,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED | MAP_FIXED_NOREPLACE,
            -1,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mmap_accepts_prot_none_and_a_nonzero_anon_offset() {
    let mut mm = mm();
    let mut host = host();
    assert!(
        mm.mmap(
            &mut host,
            0,
            PAGE,
            PROT_NONE,
            MAP_ANONYMOUS | MAP_PRIVATE,
            -1,
            PAGE
        )
        .is_ok()
    );
}

#[test]
fn munmap_rejects_zero_length() {
    assert_eq!(mm().munmap(&mut host(), MMAP_BASE, 0), Err(Errno::EINVAL));
}

#[test]
fn munmap_rejects_misaligned_address() {
    assert_eq!(
        mm().munmap(&mut host(), MMAP_BASE + 1, PAGE),
        Err(Errno::EINVAL)
    );
}

#[test]
fn munmap_releases_the_area_for_reuse() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    mm.munmap(&mut host, address, PAGE).unwrap();
    assert!(!host.mapped(address, PAGE));
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
}

#[test]
fn munmap_ignores_holes() {
    let mut mm = mm();
    let mut host = host();
    assert!(mm.munmap(&mut host, MMAP_BASE, 2 * PAGE).is_ok());
}

#[test]
fn munmap_splits_a_mapping() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, 3 * PAGE, PROT_READ).unwrap();
    mm.munmap(&mut host, address + PAGE, PAGE).unwrap();
    assert!(host.mapped(address, PAGE));
    assert!(!host.mapped(address + PAGE, PAGE));
    assert!(host.mapped(address + 2 * PAGE, PAGE));
    fixed_map(&mut mm, &mut host, address + PAGE, PAGE, PROT_READ).unwrap();
}

#[test]
fn mprotect_rejects_zero_and_misaligned_length() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mprotect(&mut host, MMAP_BASE, 0, PROT_READ),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mm.mprotect(&mut host, MMAP_BASE + 1, PAGE, PROT_READ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mprotect_rejects_holes() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, 0x4000_8000, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mprotect(&mut host, 0x4000_8000, 2 * PAGE, PROT_READ),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn mprotect_rejects_write_and_execute() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, 0x4000_8000, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mprotect(&mut host, 0x4000_8000, PAGE, PROT_WRITE | PROT_EXEC),
        Err(Errno::EACCES)
    );
}

#[test]
fn mprotect_updates_the_host_and_merges() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, 0x4000_8000, PAGE, PROT_READ).unwrap();
    fixed_map(&mut mm, &mut host, 0x4000_9000, PAGE, PROT_READ).unwrap();
    mm.mprotect(&mut host, 0x4000_8000, 2 * PAGE, PROT_READ | PROT_WRITE)
        .unwrap();
    assert_eq!(
        host.maps.get(&0x4000_8000),
        Some(&(PAGE, PROT_READ | PROT_WRITE))
    );
    assert_eq!(
        host.maps.get(&0x4000_9000),
        Some(&(PAGE, PROT_READ | PROT_WRITE))
    );
}

#[test]
fn mprotect_splits_when_the_range_is_partial() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, 0x4000_8000, 2 * PAGE, PROT_READ).unwrap();
    mm.mprotect(&mut host, 0x4000_8000, PAGE, PROT_READ | PROT_WRITE)
        .unwrap();
    assert_eq!(
        host.maps.get(&0x4000_8000),
        Some(&(PAGE, PROT_READ | PROT_WRITE))
    );
    assert_eq!(host.maps.get(&0x4000_9000), Some(&(PAGE, PROT_READ)));
}

#[test]
fn mremap_shrinks_in_place() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, 4 * PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mremap(&mut host, address, 4 * PAGE, 2 * PAGE, 0, 0)
            .unwrap(),
        address
    );
    assert_eq!(host.maps.get(&address), Some(&(2 * PAGE, PROT_READ)));
}

#[test]
fn mremap_grows_in_place_when_free() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mremap(&mut host, address, PAGE, 2 * PAGE, 0, 0).unwrap(),
        address
    );
    assert!(host.mapped(address, 2 * PAGE));
}

#[test]
fn mremap_grow_needs_maymove_when_blocked() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    fixed_map(&mut mm, &mut host, address + PAGE, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mremap(&mut host, address, PAGE, 2 * PAGE, 0, 0),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn mremap_maymove_moves_the_mapping() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    fixed_map(&mut mm, &mut host, address + PAGE, PAGE, PROT_READ).unwrap();
    let moved = mm.mremap(&mut host, address, PAGE, 2 * PAGE, 1, 0).unwrap();
    assert_ne!(moved, address);
    assert!(!host.mapped(address, PAGE));
    assert!(host.mapped(moved, 2 * PAGE));
}

#[test]
fn mremap_fixed_moves_to_the_requested_address() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    let target = 0x4000_c000;
    fixed_map(&mut mm, &mut host, address, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mremap(&mut host, address, PAGE, 2 * PAGE, 3, target)
            .unwrap(),
        target
    );
    assert!(!host.mapped(address, PAGE));
    assert!(host.mapped(target, 2 * PAGE));
}

#[test]
fn mremap_rejects_unsupported_flags_and_misalignment() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, 0x4000_8000, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mremap(&mut host, 0x4000_8000, PAGE, 2 * PAGE, 4, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        mm.mremap(&mut host, 0x4000_8001, PAGE, 2 * PAGE, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mremap_without_a_mapping_is_enomem() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mremap(&mut host, 0x4000_8000, PAGE, 2 * PAGE, 0, 0),
        Err(Errno::ENOMEM)
    );
}

#[test]
fn brk_reports_the_initial_break_on_zero() {
    let mut mm = mm();
    assert_eq!(mm.brk(&mut host(), 0), BRK_START);
}

#[test]
fn brk_grows_and_shrinks() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(mm.brk(&mut host, BRK_START + PAGE), BRK_START + PAGE);
    assert!(host.mapped(BRK_START, PAGE));
    assert_eq!(mm.brk(&mut host, BRK_START), BRK_START);
    assert!(!host.mapped(BRK_START, PAGE));
}

#[test]
fn brk_rejects_below_the_start_and_above_the_mmap_base() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(mm.brk(&mut host, BRK_START - 1), BRK_START);
    assert_eq!(mm.brk(&mut host, MMAP_BASE + PAGE), BRK_START);
}

#[test]
fn brk_does_not_overwrite_an_existing_mapping() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, BRK_START, PAGE, PROT_READ).unwrap();
    assert_eq!(mm.brk(&mut host, BRK_START + PAGE), BRK_START);
}

#[test]
fn mmap_overflowing_length_is_einval() {
    assert_eq!(
        map(&mut mm(), &mut host(), 0, usize::MAX, PROT_READ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mmap_fixed_noreplace_needs_an_address() {
    let mut mm = mm();
    let mut host = host();
    assert_eq!(
        mm.mmap(
            &mut host,
            0,
            PAGE,
            PROT_READ,
            MAP_ANONYMOUS | MAP_PRIVATE | MAP_FIXED_NOREPLACE,
            -1,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn mremap_fixed_requires_maymove() {
    let mut mm = mm();
    let mut host = host();
    fixed_map(&mut mm, &mut host, 0x4000_8000, PAGE, PROT_READ).unwrap();
    assert_eq!(
        mm.mremap(&mut host, 0x4000_8000, PAGE, 2 * PAGE, 2, 0x4000_c000),
        Err(Errno::EINVAL)
    );
}

#[test]
fn munmap_rounds_the_length_up() {
    let mut mm = mm();
    let mut host = host();
    let address = 0x4000_8000;
    fixed_map(&mut mm, &mut host, address, 2 * PAGE, PROT_READ).unwrap();
    mm.munmap(&mut host, address, PAGE + 1).unwrap();
    assert!(!host.mapped(address, 2 * PAGE));
}
