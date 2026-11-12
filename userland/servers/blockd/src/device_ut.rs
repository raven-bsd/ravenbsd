// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RAM-disk tests.

use alloc::vec;
use alloc::vec::Vec;

use super::*;

fn disk(sectors: usize) -> RamDisk {
    RamDisk::new(vec![0u8; sectors * SECTOR_SIZE as usize], false).expect("disk")
}

#[test]
fn new_requires_whole_nonempty_sectors() {
    assert_eq!(RamDisk::new(Vec::new(), false).err(), Some(Errno::EINVAL));
    assert_eq!(
        RamDisk::new(vec![0u8; 511], false).err(),
        Some(Errno::EINVAL)
    );
    assert_eq!(disk(2).sectors(), 2);
}

#[test]
fn info_reports_geometry_and_read_only_state() {
    let disk = disk(4);
    let info = disk.info();
    assert_eq!(info.sector_size, SECTOR_SIZE);
    assert_eq!(info.read_only, 0);
    assert_eq!(info.sectors, 4);
    assert_eq!(info.reserved, 0);

    let read_only = RamDisk::new(vec![0u8; 512], true).expect("disk");
    assert_eq!(read_only.info().read_only, 1);
}

#[test]
fn sectors_round_trip_and_bounds_are_checked() {
    let mut disk = disk(4);
    let mut pattern = [0u8; 1024];
    for (index, byte) in pattern.iter_mut().enumerate() {
        *byte = index as u8;
    }
    disk.write_sectors(1, &pattern).expect("write");
    let mut read = [0u8; 1024];
    disk.read_sectors(1, &mut read).expect("read");
    assert_eq!(read, pattern);

    assert_eq!(disk.write_sectors(3, &pattern).err(), Some(Errno::EINVAL));
    assert_eq!(
        disk.read_sectors(4, &mut read[..512]).err(),
        Some(Errno::EINVAL)
    );
    assert_eq!(disk.write_sectors(0, &[0u8; 3]).err(), Some(Errno::EINVAL));
    assert_eq!(
        disk.read_sectors(u64::MAX, &mut read[..512]).err(),
        Some(Errno::EINVAL)
    );
}

#[test]
fn read_only_disk_rejects_writes() {
    let mut disk = RamDisk::new(vec![0u8; 512], true).expect("disk");
    assert_eq!(disk.write_sectors(0, &[1u8; 512]).err(), Some(Errno::EROFS));
    let mut read = [0u8; 512];
    disk.read_sectors(0, &mut read).expect("read");
    assert_eq!(read, [0u8; 512]);
}

#[test]
fn flush_succeeds() {
    let mut disk = disk(1);
    disk.flush().expect("flush");
    disk.flush().expect("flush");
}
