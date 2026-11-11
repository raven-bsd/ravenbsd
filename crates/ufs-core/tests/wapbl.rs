// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! WAPBL journal tests: layout, replay and crash cuts.

mod common;

use std::cell::Cell;
use std::rc::Rc;

use common::VecDisk;
use ufs_core::mkfs::{MkfsOptions, format, format_journaled};
use ufs_core::{BlockIo, Ino, IoError, MountOptions, Ufs, UfsError};

const MIB: u64 = 1024 * 1024;
const STAMP: i64 = 1_767_225_600;

fn options(size: u64) -> MkfsOptions<'static> {
    MkfsOptions {
        size,
        version: 2,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "k58",
        timestamp: STAMP,
    }
}

fn journaled() -> Ufs<VecDisk> {
    let total = 17 * MIB;
    let disk = VecDisk::new(vec![0u8; total as usize]);
    format_journaled(disk, &options(total), MIB).expect("format journaled")
}

fn read_all<D: BlockIo>(ufs: &mut Ufs<D>, ino: Ino, size: u64) -> Vec<u8> {
    let mut data = vec![0u8; size as usize];
    let mut done = 0;
    while done < data.len() {
        let n = ufs.read(ino, done as u64, &mut data[done..]).expect("read");
        assert!(n > 0);
        done += n;
    }
    data
}

/// A block device that fails every write after a shared limit of successful writes.
struct CutDisk {
    data: Vec<u8>,
    writes: Rc<Cell<usize>>,
    limit: Rc<Cell<usize>>,
}

impl CutDisk {
    fn new(data: Vec<u8>) -> Self {
        Self {
            data,
            writes: Rc::new(Cell::new(0)),
            limit: Rc::new(Cell::new(usize::MAX)),
        }
    }

    fn writes_handle(&self) -> Rc<Cell<usize>> {
        Rc::clone(&self.writes)
    }

    fn limit_handle(&self) -> Rc<Cell<usize>> {
        Rc::clone(&self.limit)
    }

    fn into_data(self) -> Vec<u8> {
        self.data
    }
}

impl BlockIo for CutDisk {
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let src = self.data.get(start..end).ok_or(IoError)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        if self.writes.get() >= self.limit.get() {
            return Err(IoError);
        }
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = self.data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

fn base_journaled_image() -> Vec<u8> {
    let mut ufs = journaled();
    let file = ufs
        .create(ufs.root(), b"stable", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"stable").expect("write");
    ufs.sync().expect("sync");
    ufs.unmount().expect("unmount").data
}

fn verify_replayed(image: &[u8]) -> (bool, Vec<u8>) {
    let mut ufs = Ufs::mount(
        CutDisk::new(image.to_vec()),
        MountOptions { read_only: false },
    )
    .expect("remount");
    let stable = ufs.lookup(ufs.root(), b"stable").expect("stable");
    assert_eq!(read_all(&mut ufs, stable, 6), b"stable");
    let present = match ufs.lookup(ufs.root(), b"crash") {
        Ok(file) => {
            let attr = ufs.getattr(file).expect("attr");
            assert_eq!(attr.size, 18);
            assert_eq!(read_all(&mut ufs, file, attr.size), b"journaled payload\n");
            true
        }
        Err(UfsError::NotFound) => false,
        Err(error) => panic!("lookup: {error:?}"),
    };
    ufs.verify_allocation().expect("allocation");
    ufs.sync().expect("sync after replay");
    let data = ufs.unmount().expect("unmount").data;
    (present, data)
}

fn transaction_writes(image: &[u8]) -> usize {
    let disk = CutDisk::new(image.to_vec());
    let writes = disk.writes_handle();
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("mount");
    let file = ufs
        .create(ufs.root(), b"crash", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"journaled payload\n").expect("write");
    let before = writes.get();
    ufs.sync().expect("sync");
    let after = writes.get();
    ufs.unmount().expect("unmount");
    after - before
}

fn run_with_cut(image: &[u8], cut: usize) -> Vec<u8> {
    let disk = CutDisk::new(image.to_vec());
    let counter = disk.writes_handle();
    let limit = disk.limit_handle();
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("mount");
    let file = ufs
        .create(ufs.root(), b"crash", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"journaled payload\n").expect("write");
    let before = counter.get();
    limit.set(before + cut);
    let _ = ufs.sync();
    ufs.into_inner().into_data()
}

fn journal_head_tail(image: &[u8]) -> (u64, u64) {
    let base = 8192usize;
    let start = u64::from_le_bytes(image[base + 872..base + 880].try_into().expect("start"));
    let block = u64::from_le_bytes(image[base + 888..base + 896].try_into().expect("block"));
    let start = (start * block) as usize;
    let mut best = (0u32, 0u64, 0u64);
    for slot in [0usize, 512] {
        let at = start + slot;
        let kind = u32::from_le_bytes(image[at..at + 4].try_into().expect("kind"));
        if kind != 0x5741_424c {
            continue;
        }
        let generation = u32::from_le_bytes(image[at + 12..at + 16].try_into().expect("gen"));
        let head = i64::from_le_bytes(image[at + 48..at + 56].try_into().expect("head"));
        let tail = i64::from_le_bytes(image[at + 56..at + 64].try_into().expect("tail"));
        if generation >= best.0 {
            best = (generation, head.max(0) as u64, tail.max(0) as u64);
        }
    }
    (best.1, best.2)
}

#[test]
fn every_prefix_of_a_committed_transaction_recovers() {
    let base = base_journaled_image();
    let writes = transaction_writes(&base);
    assert!(writes > 0);
    for cut in 0..=writes {
        let image = run_with_cut(&base, cut);
        let (present, _) = verify_replayed(&image);
        if cut == 0 {
            assert!(!present, "an uncommitted transaction must not appear");
        }
        if cut == writes {
            assert!(present, "a fully committed transaction must replay");
        }
    }
}

#[test]
fn formats_and_mounts_with_a_journal() {
    let mut ufs = journaled();
    assert!(ufs.is_journaled());
    let file = ufs
        .create(ufs.root(), b"hello", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"journaled payload\n").expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");

    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("mount ro");
    let file = ufs.lookup(ufs.root(), b"hello").expect("lookup");
    let attr = ufs.getattr(file).expect("attr");
    assert_eq!(attr.size, 18);
    assert_eq!(read_all(&mut ufs, file, attr.size), b"journaled payload\n");
}

#[test]
fn a_clean_journaled_image_mounts_read_only() {
    let mut ufs = journaled();
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("mount ro");
    assert!(!ufs.is_journaled());
}

#[test]
fn non_journaled_format_is_unchanged() {
    let size = 16 * MIB;
    let disk = VecDisk::new(vec![0u8; size as usize]);
    let ufs = format(disk, &options(size)).expect("format");
    assert!(!ufs.is_journaled());
}

#[test]
fn journal_area_is_outside_the_filesystem() {
    let disk = journaled().unmount().expect("unmount");
    assert_eq!(disk.data.len(), 17 * MIB as usize);
    // The superblock records an end-of-partition log at byte 16 MiB.
    let base = 8192usize;
    assert_eq!(disk.data[base + 864], 1);
    assert_eq!(disk.data[base + 865], 1);
    let start = u64::from_le_bytes(disk.data[base + 872..base + 880].try_into().expect("start"));
    let count = u64::from_le_bytes(disk.data[base + 880..base + 888].try_into().expect("count"));
    let block = u64::from_le_bytes(disk.data[base + 888..base + 896].try_into().expect("block"));
    assert_eq!(start, (16 * MIB) / 512);
    assert_eq!(count, MIB / 512);
    assert_eq!(block, 512);
}

#[test]
fn enable_and_disable_journal() {
    let size = 17 * MIB;
    let disk = VecDisk::new(vec![0u8; size as usize]);
    let mut ufs = format_journaled(disk, &options(size), MIB).expect("format");
    assert!(ufs.is_journaled());
    ufs.disable_journal().expect("disable");
    assert!(!ufs.is_journaled());
    ufs.enable_journal().expect("enable");
    assert!(ufs.is_journaled());
}

#[test]
fn replay_after_dropping_without_unmount() {
    let mut ufs = journaled();
    let file = ufs
        .create(ufs.root(), b"crash", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"before crash").expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.into_inner();
    // `into_inner` skipped unmount; the log is empty after sync, so this only
    // proves a clean image with a journal fields block still mounts.
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("remount");
    let file = ufs.lookup(ufs.root(), b"crash").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 12), b"before crash");
}

#[test]
fn read_only_mount_refuses_a_dirty_journal() {
    let base = base_journaled_image();
    let writes = transaction_writes(&base);
    for cut in 1..writes {
        let image = run_with_cut(&base, cut);
        let (head, tail) = journal_head_tail(&image);
        if head == 0 && tail == 0 {
            continue;
        }
        let result = Ufs::mount(CutDisk::new(image), MountOptions { read_only: true });
        assert!(
            matches!(
                result,
                Err(UfsError::Unsupported("dirty journal on read-only mount"))
            ),
            "dirty journal was not refused"
        );
        return;
    }
    panic!("no cut produced a dirty journal");
}

#[test]
fn fsync_commits_the_journal() {
    let mut ufs = journaled();
    let file = ufs
        .create(ufs.root(), b"sync", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"payload").expect("write");
    ufs.fsync(file).expect("fsync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let file = ufs.lookup(ufs.root(), b"sync").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 7), b"payload");
}

#[test]
fn journaled_write_survives_a_remount() {
    let mut ufs = journaled();
    for index in 0..20 {
        let name = format!("file-{index:02}");
        let file = ufs
            .create(ufs.root(), name.as_bytes(), 0o644, 0, 0)
            .expect("create");
        ufs.write(file, 0, &vec![index as u8; 4096]).expect("write");
        ufs.lookup(ufs.root(), name.as_bytes())
            .unwrap_or_else(|error| panic!("lookup after create {name}: {error:?}"));
    }
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("mount");
    for index in 0..20u8 {
        let name = format!("file-{index:02}");
        let file = ufs
            .lookup(ufs.root(), name.as_bytes())
            .unwrap_or_else(|error| panic!("lookup {name}: {error:?}"));
        let attr = ufs.getattr(file).expect("attr");
        assert_eq!(attr.size, 4096);
        assert_eq!(read_all(&mut ufs, file, attr.size), vec![index; 4096]);
    }
}

#[test]
fn journaled_ufs1_round_trip() {
    let total = 17 * MIB;
    let mut opts = options(total);
    opts.version = 1;
    let disk = VecDisk::new(vec![0u8; total as usize]);
    let mut ufs = format_journaled(disk, &opts, MIB).expect("format ufs1 journaled");
    assert!(ufs.is_journaled());
    let file = ufs
        .create(ufs.root(), b"ufs1", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"one").expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let file = ufs.lookup(ufs.root(), b"ufs1").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 3), b"one");
}

#[test]
fn journaled_8192_block_round_trip() {
    let total = 17 * MIB;
    let mut opts = options(total);
    opts.block_size = 8192;
    opts.fragment_size = 1024;
    let disk = VecDisk::new(vec![0u8; total as usize]);
    let mut ufs = format_journaled(disk, &opts, MIB).expect("format 8192 journaled");
    let file = ufs
        .create(ufs.root(), b"big-block", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, &vec![0x7e; 20_000]).expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let file = ufs.lookup(ufs.root(), b"big-block").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 20_000), vec![0x7e; 20_000]);
}

#[test]
fn format_journaled_validates_the_log_size() {
    let total = 17 * MIB;
    let disk = VecDisk::new(vec![0u8; total as usize]);
    assert!(matches!(
        format_journaled(disk, &options(total), 512 * 1024),
        Err(UfsError::Invalid)
    ));
    let disk = VecDisk::new(vec![0u8; total as usize]);
    assert!(matches!(
        format_journaled(disk, &options(total), total),
        Err(UfsError::Invalid)
    ));
}

#[test]
fn disable_journal_keeps_the_data() {
    let mut ufs = journaled();
    let file = ufs
        .create(ufs.root(), b"plain", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"after disable").expect("write");
    ufs.disable_journal().expect("disable");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let file = ufs.lookup(ufs.root(), b"plain").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 13), b"after disable");
}

#[test]
fn randomised_crash_cuts_recover() {
    let mut image = base_journaled_image();
    {
        let mut ufs = Ufs::mount(
            CutDisk::new(std::mem::take(&mut image)),
            MountOptions { read_only: false },
        )
        .expect("mount");
        for index in 0..20 {
            let name = format!("file-{index:02}");
            let file = ufs
                .create(ufs.root(), name.as_bytes(), 0o644, 0, 0)
                .expect("create");
            ufs.write(file, 0, &vec![index as u8; 4096]).expect("write");
        }
        ufs.sync().expect("sync");
        image = ufs.unmount().expect("unmount").data;
    }
    let mut state = 0x1234_5678_9abc_def0u64;
    for cycle in 0..300 {
        let disk = CutDisk::new(std::mem::take(&mut image));
        let counter = disk.writes_handle();
        let limit = disk.limit_handle();
        let mut ufs = Ufs::mount(disk, MountOptions { read_only: false }).expect("mount");
        for step in 0..16 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let index = ((cycle * 16 + step) % 20) as u8;
            let name = format!("file-{index:02}");
            let file = ufs.lookup(ufs.root(), name.as_bytes()).expect("lookup");
            let offset = ((cycle * 16 + step) % 200) as u64 * 512;
            ufs.write(file, offset, &vec![index; 512]).expect("write");
        }
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let cut = (state as usize) % 40;
        let before = counter.get();
        limit.set(before + cut);
        let _ = ufs.sync();
        image = ufs.into_inner().into_data();
        let (_, normalized) = verify_replayed(&image);
        image = normalized;
    }
    let mut ufs =
        Ufs::mount(CutDisk::new(image), MountOptions { read_only: false }).expect("final mount");
    for index in 0..20 {
        let name = format!("file-{index:02}");
        let file = ufs
            .lookup(ufs.root(), name.as_bytes())
            .expect("final lookup");
        let attr = ufs.getattr(file).expect("attr");
        assert!(attr.size >= 4096, "file {index} lost its size");
    }
    ufs.verify_allocation().expect("final allocation");
}

#[test]
#[ignore = "B63 has no NetBSD-written WAPBL golden image"]
fn netbsd_journaled_golden_layout() {
    // Kept as a named placeholder so the missing oracle is visible in the
    // test list.  When B63 gains a journaled vector, load it here and compare
    // the parsed header and record offsets.
}

#[test]
fn oracle_interop_when_present() {
    let Some(binary) = std::env::var_os("UFS_FSCK_FFS")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let dir = std::path::PathBuf::from(std::env::var_os("UFS_ORACLE_DIR")?);
            [dir.join("bin/fsck_ffs"), dir.join("fsck_ffs")]
                .into_iter()
                .find(|path| path.is_file())
        })
    else {
        println!("ORACLE ABSENT: fsck_ffs not installed; skipped");
        return;
    };
    let mut ufs = journaled();
    let file = ufs
        .create(ufs.root(), b"oracle", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"oracle").expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let dir = std::env::temp_dir().join(format!("ufs-wapbl-oracle-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("image.img");
    std::fs::write(&path, &disk.data).expect("write image");
    let output = std::process::Command::new(binary)
        .arg("-n")
        .arg(&path)
        .output()
        .expect("fsck_ffs");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "fsck_ffs disagreed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn journaled_dowapbl_flag_is_set() {
    let ufs = journaled();
    let disk = ufs.unmount().expect("unmount");
    let flags = u32::from_le_bytes(
        disk.data[8192 + 1312..8192 + 1316]
            .try_into()
            .expect("flags"),
    );
    assert_ne!(flags & 0x100, 0);
}

#[test]
fn journaled_zero_size_uses_the_device() {
    let total = 17 * MIB;
    let mut opts = options(total);
    opts.size = 0;
    let disk = VecDisk::new(vec![0u8; total as usize]);
    let mut ufs = format_journaled(disk, &opts, MIB).expect("format");
    let file = ufs
        .create(ufs.root(), b"zero", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"ok").expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let file = ufs.lookup(ufs.root(), b"zero").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 2), b"ok");
}

#[test]
fn two_committed_transactions_both_survive() {
    let mut ufs = journaled();
    let first = ufs
        .create(ufs.root(), b"first", 0o644, 0, 0)
        .expect("create first");
    ufs.write(first, 0, b"one").expect("write first");
    ufs.sync().expect("sync first");
    let second = ufs
        .create(ufs.root(), b"second", 0o644, 0, 0)
        .expect("create second");
    ufs.write(second, 0, b"two").expect("write second");
    ufs.sync().expect("sync second");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let cases: [(&[u8], &[u8]); 2] = [(b"first", b"one"), (b"second", b"two")];
    for (name, value) in cases {
        let file = ufs.lookup(ufs.root(), name).expect("lookup");
        assert_eq!(read_all(&mut ufs, file, 3), value);
    }
}

#[test]
fn journaled_sync_passes_verify_allocation() {
    let mut ufs = journaled();
    for index in 0..5 {
        let name = format!("verify-{index}");
        let file = ufs
            .create(ufs.root(), name.as_bytes(), 0o644, 0, 0)
            .expect("create");
        ufs.write(file, 0, &vec![index as u8; 2048]).expect("write");
    }
    ufs.sync().expect("sync");
    ufs.verify_allocation().expect("allocation");
    let _ = ufs.unmount().expect("unmount");
}

#[test]
fn journaled_64m_round_trip() {
    let total = 65 * MIB;
    let disk = VecDisk::new(vec![0u8; total as usize]);
    let mut ufs = format_journaled(disk, &options(total), MIB).expect("format 64m");
    let file = ufs
        .create(ufs.root(), b"large", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, &vec![0x3c; 100_000]).expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: true }).expect("ro");
    let file = ufs.lookup(ufs.root(), b"large").expect("lookup");
    assert_eq!(read_all(&mut ufs, file, 100_000), vec![0x3c; 100_000]);
}

#[test]
fn replaying_a_cut_image_twice_is_stable() {
    let base = base_journaled_image();
    let writes = transaction_writes(&base);
    let image = run_with_cut(&base, writes / 2);
    let (_, first) = verify_replayed(&image);
    let (_, second) = verify_replayed(&first);
    assert_eq!(journal_head_tail(&second), (0, 0));
}

#[test]
fn missing_journal_space_is_reported() {
    let size = 16 * MIB;
    let disk = VecDisk::new(vec![0u8; size as usize]);
    let mut ufs = format(disk, &options(size)).expect("format");
    assert!(matches!(
        ufs.enable_journal(),
        Err(UfsError::Unsupported("no journal space in superblock"))
    ));
}
