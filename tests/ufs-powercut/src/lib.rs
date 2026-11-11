// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-side power-cut harness.
//!
//! Until K55 provides a guest disk server, this runs the workload on the host
//! through `ufs-core`, fails every write after a chosen count, remounts the
//! image, runs the K52 allocation check and walks the tree read-only. K57 is
//! not merged, so the `repaired` column is `k57-absent`; the QEMU half lives in
//! `qemu-cut.sh` and needs K55.

#![forbid(unsafe_code)]

use std::cell::Cell;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;

use ufs_core::{BlockIo, IoError, MountOptions, Ufs, UfsError};

/// One cut's result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CutRow {
    /// Cut number (successful writes allowed).
    pub cut: u64,
    /// Workload, read-only walk and allocation-check findings.
    pub findings: String,
    /// `k57-absent` until K57 provides a repair mode.
    pub repaired: String,
    /// Whether remount, read-only walk and allocation check all succeeded.
    pub clean: bool,
}

impl CutRow {
    /// Render one TSV row.
    pub fn tsv(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.cut,
            self.findings.replace(['\t', '\n'], " "),
            self.repaired,
            if self.clean { "yes" } else { "no" }
        )
    }
}

/// Summary of a range of cuts.
pub struct CutSummary {
    /// Per-cut rows.
    pub rows: Vec<CutRow>,
    /// Written TSV path.
    pub report: PathBuf,
}

impl CutSummary {
    /// One-line summary for the report and test output.
    pub fn summary_line(&self) -> String {
        let clean = self.rows.iter().filter(|row| row.clean).count();
        let repaired = self
            .rows
            .iter()
            .filter(|row| row.repaired != "k57-absent")
            .count();
        format!(
            "ufs-powercut: cuts={} clean={} repaired={repaired} unrepairable={} k57=absent",
            self.rows.len(),
            clean,
            self.rows.len() - clean
        )
    }
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }
}

/// A RAM disk that refuses every write after `budget` successful writes.
pub struct PowerCut {
    /// Raw image bytes.
    pub data: Vec<u8>,
    budget: Rc<Cell<u64>>,
    writes: Rc<Cell<u64>>,
}

/// A handle to change the write budget after mounting.
#[derive(Clone)]
pub struct CutHandle {
    budget: Rc<Cell<u64>>,
    writes: Rc<Cell<u64>>,
}

impl CutHandle {
    /// Allow at most this many additional successful writes.
    pub fn set(&self, budget: u64) {
        self.budget.set(budget);
    }

    /// Number of writes that succeeded so far.
    pub fn writes(&self) -> u64 {
        self.writes.get()
    }
}

impl PowerCut {
    /// Wrap an image and return the budget handle.
    pub fn new(data: Vec<u8>) -> (Self, CutHandle) {
        let budget = Rc::new(Cell::new(u64::MAX));
        let writes = Rc::new(Cell::new(0u64));
        (
            Self {
                data,
                budget: budget.clone(),
                writes: writes.clone(),
            },
            CutHandle { budget, writes },
        )
    }
}

impl BlockIo for PowerCut {
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
        if self.writes.get() >= self.budget.get() {
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

/// A clean RAM disk used for remounting a cut image.
struct VecDisk {
    data: Vec<u8>,
}

impl BlockIo for VecDisk {
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
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = self.data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

fn digest(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x1000_0000_01b3);
    }
    hash
}

fn workload(ufs: &mut Ufs<PowerCut>) -> Result<(), UfsError> {
    let root = ufs.root();
    let dir = ufs.mkdir(root, b"powercut", 0o755, 0, 0)?;
    let file = ufs.create(dir, b"file", 0o644, 0, 0)?;
    let mut data = vec![0u8; 600];
    let mut rng = Rng(0x1234_5678_9abc_def0);
    for byte in &mut data {
        *byte = (rng.next() & 0xff) as u8;
    }
    ufs.write(file, 0, &data)?;
    ufs.link(dir, b"hard", file)?;
    ufs.symlink(dir, b"link", b"file", 0, 0)?;
    ufs.mkdir(dir, b"sub", 0o755, 0, 0)?;
    ufs.rename(dir, b"file", root, b"moved")?;
    ufs.unlink(root, b"moved")?;
    ufs.unlink(dir, b"hard")?;
    ufs.unlink(dir, b"link")?;
    ufs.rename(dir, b"sub", root, b"sub2")?;
    ufs.rmdir(root, b"sub2")?;
    ufs.rmdir(root, b"powercut")
}

fn read_only_walk(data: &[u8]) -> Result<(), String> {
    let mut ufs = Ufs::mount(
        VecDisk {
            data: data.to_vec(),
        },
        MountOptions { read_only: true },
    )
    .map_err(|error| format!("readonly mount: {error}"))?;
    let root = ufs.root();
    walk(&mut ufs, root)
}

fn walk(ufs: &mut Ufs<VecDisk>, dir: ufs_core::Ino) -> Result<(), String> {
    let mut children = Vec::new();
    ufs.readdir(dir, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            children.push(entry.ino);
        }
        true
    })
    .map_err(|error| format!("readdir: {error}"))?;
    for ino in children {
        let attr = ufs
            .getattr(ino)
            .map_err(|error| format!("getattr: {error}"))?;
        if attr.kind == ufs_core::FileType::Regular {
            let size = usize::try_from(attr.size).map_err(|_| "file too large".to_string())?;
            let mut buf = vec![0u8; size];
            ufs.read(ino, 0, &mut buf)
                .map_err(|error| format!("read: {error}"))?;
            let _ = digest(&buf);
        }
        if attr.kind == ufs_core::FileType::Directory {
            walk(ufs, ino)?;
        }
    }
    Ok(())
}

fn fsck_binary() -> Option<PathBuf> {
    std::env::var_os("UFS_FSCK_FFS")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let dir = PathBuf::from(std::env::var_os("UFS_ORACLE_DIR")?);
            [dir.join("bin/fsck_ffs"), dir.join("fsck_ffs")]
                .into_iter()
                .find(|path| path.is_file())
        })
}

fn fsck_status(image: &Path) -> String {
    let Some(binary) = fsck_binary() else {
        return "k57-absent/oracle-absent".to_string();
    };
    match Command::new(binary).arg("-n").arg(image).output() {
        Ok(output) if output.status.success() => "fsck-ok".to_string(),
        Ok(output) => format!("fsck-exit-{:?}", output.status.code()),
        Err(error) => format!("fsck-error-{error}"),
    }
}

fn summary_path() -> Result<PathBuf, String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("build/ufs-powercut");
    fs::create_dir_all(&dir).map_err(|error| format!("create {}: {error}", dir.display()))?;
    Ok(dir.join("report.tsv"))
}

/// Run one cut against a fresh copy of `base`.
pub fn run_cut(cut: u64, base: &[u8]) -> Result<CutRow, String> {
    let (disk, handle) = PowerCut::new(base.to_vec());
    let mut ufs = Ufs::mount(disk, MountOptions { read_only: false })
        .map_err(|error| format!("cut {cut}: mount: {error}"))?;
    handle.set(cut);
    let operation = workload(&mut ufs);
    let disk = ufs.into_inner();
    let data = disk.data;

    let read_only = read_only_walk(&data)
        .map(|()| "readonly-ok".to_string())
        .unwrap_or_else(|error| format!("readonly-{error}"));

    let mut remounted = Ufs::mount(
        VecDisk { data: data.clone() },
        MountOptions { read_only: false },
    )
    .map_err(|error| format!("cut {cut}: remount: {error}"))?;
    let allocation = remounted
        .verify_allocation()
        .map(|()| "alloc-ok".to_string())
        .unwrap_or_else(|error| format!("alloc-{error}"));

    let oracle = if fsck_binary().is_some() {
        let image_path = std::env::temp_dir().join(format!("ufs-powercut-{cut}.img"));
        fs::write(&image_path, &data).map_err(|error| format!("write cut image: {error}"))?;
        let status = fsck_status(&image_path);
        let _ = fs::remove_file(&image_path);
        status
    } else {
        "k57-absent/oracle-absent".to_string()
    };

    let clean = read_only == "readonly-ok"
        && allocation == "alloc-ok"
        && (oracle == "k57-absent/oracle-absent" || oracle == "fsck-ok");
    let findings = format!(
        "op={} {read_only} {allocation} {oracle}",
        match operation {
            Ok(()) => "ok".to_string(),
            Err(error) => format!("{error:?}"),
        }
    );
    Ok(CutRow {
        cut,
        findings,
        repaired: "k57-absent".to_string(),
        clean,
    })
}

/// Run `cuts` cuts (0..cuts) and write `build/ufs-powercut/report.tsv`.
pub fn run(cuts: u64) -> Result<CutSummary, String> {
    let base = load_golden()?;
    let mut rows = Vec::new();
    for cut in 0..cuts {
        rows.push(run_cut(cut, &base)?);
    }
    let report = summary_path()?;
    let mut file = fs::File::create(&report)
        .map_err(|error| format!("create {}: {error}", report.display()))?;
    writeln!(file, "cut\tfindings\trepaired\tclean")
        .map_err(|error| format!("write {}: {error}", report.display()))?;
    for row in &rows {
        writeln!(file, "{}", row.tsv())
            .map_err(|error| format!("write {}: {error}", report.display()))?;
    }
    Ok(CutSummary { rows, report })
}

/// Load the golden image for a standalone cut.
pub fn load_golden_for_test() -> Result<Vec<u8>, String> {
    load_golden()
}

fn load_golden() -> Result<Vec<u8>, String> {
    if let Some(raw) = std::env::var_os("UFS_ORACLE_IMAGE") {
        return fs::read(&raw).map_err(|error| format!("read {}: {error}", raw.display()));
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let vectors = std::env::var_os("UFS_VECTORS_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("tests/ufs-vectors"));
    let path = vectors.join("golden/ufs2-b4096.img.gz");
    let output = Command::new("gzip")
        .arg("-dc")
        .arg(&path)
        .output()
        .map_err(|error| format!("gzip {}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "gzip {} failed: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}
