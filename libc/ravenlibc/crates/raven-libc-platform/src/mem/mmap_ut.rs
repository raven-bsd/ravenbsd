// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Memory glue tests.

use raven_abi::errno::Errno;
use raven_libc_abi::mman;

use crate::backend::FakeSys;
use crate::mem::{brk, getpagesize, madvise, mlock, mmap, mprotect, msync, munmap};
use crate::platform::Platform;

const PAGE: usize = 4096;

#[test]
fn page_size_is_4096() {
    assert_eq!(getpagesize(), 4096);
}

#[test]
fn anonymous_private_is_zero_filled() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let address = mmap(
        &platform,
        0,
        PAGE,
        3,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("mmap");
    assert_eq!(
        platform.backend().mapped(address).expect("mapped"),
        alloc::vec![0u8; PAGE]
    );
}

#[test]
fn anonymous_shared_goes_through_memobj_create() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let address = mmap(
        &platform,
        0,
        PAGE,
        3,
        mman::MAP_SHARED | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("mmap");
    assert!(platform.backend().mapped(address).is_some());
}

#[test]
fn file_mapping_is_enodev_until_pg_36() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", raven_libc_abi::fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(
        mmap(&platform, 0, PAGE, 1, mman::MAP_PRIVATE, fd, 0),
        Err(Errno::ENODEV)
    );
}

#[test]
fn zero_length_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        mmap(
            &platform,
            0,
            0,
            1,
            mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
            -1,
            0
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn misaligned_offset_is_einval() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        mmap(
            &platform,
            0,
            PAGE,
            1,
            mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
            -1,
            1
        ),
        Err(Errno::EINVAL)
    );
}

#[test]
fn write_exec_is_eacces() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(
        mmap(
            &platform,
            0,
            PAGE,
            3 | 4,
            mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
            -1,
            0
        ),
        Err(Errno::EACCES)
    );
}

#[test]
fn fixed_mapping_replaces_the_range() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let first = mmap(
        &platform,
        0x5000_0000,
        PAGE,
        1,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS | mman::MAP_FIXED,
        -1,
        0,
    )
    .expect("first");
    assert_eq!(first, 0x5000_0000);
    let second = mmap(
        &platform,
        0x5000_0000,
        PAGE,
        3,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS | mman::MAP_FIXED,
        -1,
        0,
    )
    .expect("second");
    assert_eq!(second, 0x5000_0000);
}

#[test]
fn munmap_removes_the_mapping() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let address = mmap(
        &platform,
        0,
        PAGE,
        1,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("mmap");
    munmap(&platform, address, PAGE).expect("munmap");
    assert!(platform.backend().mapped(address).is_none());
}

#[test]
fn mprotect_changes_the_bits() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let address = mmap(
        &platform,
        0,
        PAGE,
        1,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("mmap");
    mprotect(&platform, address, PAGE, 3).expect("mprotect");
    assert!(platform.backend().mapped(address).is_some());
}

#[test]
fn madvise_normal_succeeds_and_dontneed_is_tbd() {
    let fake = FakeSys::new();
    let platform = Platform::new(fake);
    let address = mmap(
        &platform,
        0,
        PAGE,
        1,
        mman::MAP_PRIVATE | mman::MAP_ANONYMOUS,
        -1,
        0,
    )
    .expect("mmap");
    assert_eq!(madvise(&platform, address, PAGE, mman::MADV_NORMAL), Ok(()));
    assert_eq!(
        madvise(&platform, address, PAGE, mman::MADV_DONTNEED),
        Err(Errno::ENOSYS)
    );
}

#[test]
fn mlock_is_an_accepted_no_op() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(mlock(&platform, 0x1000, PAGE), Ok(()));
}

#[test]
fn msync_and_brk_are_enosys() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(msync(&platform, 0x1000, PAGE, 0), Err(Errno::ENOSYS));
    assert_eq!(brk(&platform, 0x1000), Err(Errno::ENOSYS));
}
