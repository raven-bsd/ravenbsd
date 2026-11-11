// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic operation generator and the 20-seed differential run.

use std::fs;
use std::path::{Path, PathBuf};

use ufs_core::{MountOptions, SetAttr, Ufs};

use crate::compare;
use crate::image::{VecDisk, build_dir, load_golden};
use crate::model::{InodeModel, Kind, Model, join, split};
use crate::oracle;

/// Configuration for one differential run.
pub struct Config {
    /// Number of fixed seeds to run.
    pub seeds: u64,
    /// Operations per seed.
    pub operations: u32,
    /// Directory holding `golden/` and `expected/`.
    pub vectors: PathBuf,
    /// Ignored build output directory.
    pub build: PathBuf,
}

impl Config {
    /// Read `UFS_ORACLE_SEEDS` (default 20) and `UFS_ORACLE_OPS` (default 5000).
    pub fn from_env() -> Result<Self, String> {
        let seeds = std::env::var("UFS_ORACLE_SEEDS")
            .ok()
            .map(|value| value.parse::<u64>().map_err(|_| "bad UFS_ORACLE_SEEDS"))
            .transpose()?
            .unwrap_or(20);
        let operations = std::env::var("UFS_ORACLE_OPS")
            .ok()
            .map(|value| value.parse::<u32>().map_err(|_| "bad UFS_ORACLE_OPS"))
            .transpose()?
            .unwrap_or(5000);
        Ok(Self {
            seeds,
            operations,
            vectors: crate::image::vectors_dir(),
            build: build_dir("ufs-oracle-tests")?,
        })
    }
}

/// One seed's result.
pub struct SeedReport {
    /// Fixed seed value.
    pub seed: u64,
    /// Requested operations.
    pub operations: u32,
    /// Whole-tree plus allocation checks.
    pub checks: u32,
    /// `ok`, `absent` or an error string from `fsck_ffs -n`.
    pub fsck: String,
    /// `ok`, `absent` or an error string from the Python oracle reader.
    pub image_check: String,
}

impl SeedReport {
    /// True when every available oracle accepted the final image.
    pub fn ok(&self) -> bool {
        self.fsck == "absent"
            || self.fsck == "ok" && (self.image_check == "absent" || self.image_check == "ok")
    }

    /// One report line.
    pub fn line(&self) -> String {
        format!(
            "seed {:02}: ops={} checks={} fsck={} image={}",
            self.seed, self.operations, self.checks, self.fsck, self.image_check
        )
    }
}

/// Result of a complete seed run.
pub struct Summary {
    /// Per-seed reports.
    pub seeds: Vec<SeedReport>,
}

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed ^ 0x9e37_79b9_7f4a_7c15 | 1)
    }

    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() as usize) % limit
    }
}

fn model_inode(kind: Kind, mode: u16, uid: u32, gid: u32) -> InodeModel {
    InodeModel {
        kind,
        mode,
        uid,
        gid,
        data: Vec::new(),
        target: Vec::new(),
    }
}

fn pick(values: &[Vec<u8>], rng: &mut Rng) -> Option<Vec<u8>> {
    if values.is_empty() {
        None
    } else {
        Some(values[rng.below(values.len())].clone())
    }
}

fn apply_create(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    dirs: &[Vec<u8>],
    rng: &mut Rng,
    sequence: u32,
) -> Result<(), String> {
    let Some(dir) = pick(dirs, rng) else {
        return Ok(());
    };
    let name = format!("f{sequence}");
    let path = join(&dir, name.as_bytes());
    if model.paths.contains_key(&path) {
        return Ok(());
    }
    let mode = 0o600 | (rng.next() & 0o77) as u16;
    let uid = (rng.next() & 0xffff) as u32;
    let gid = (rng.next() & 0xffff) as u32;
    let parent = model.ino(&dir).ok_or("model dir missing")?;
    let ino = ufs
        .create(parent, name.as_bytes(), mode, uid, gid)
        .map_err(|error| format!("create {name}: {error}"))?;
    model.insert(path, ino, model_inode(Kind::File, mode, uid, gid));
    Ok(())
}

fn apply_mkdir(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    dirs: &[Vec<u8>],
    rng: &mut Rng,
    sequence: u32,
) -> Result<(), String> {
    let Some(dir) = pick(dirs, rng) else {
        return Ok(());
    };
    let name = format!("d{sequence}");
    let path = join(&dir, name.as_bytes());
    if model.paths.contains_key(&path) {
        return Ok(());
    }
    let mode = 0o700 | (rng.next() & 0o77) as u16;
    let uid = (rng.next() & 0xffff) as u32;
    let gid = (rng.next() & 0xffff) as u32;
    let parent = model.ino(&dir).ok_or("model dir missing")?;
    let ino = ufs
        .mkdir(parent, name.as_bytes(), mode, uid, gid)
        .map_err(|error| format!("mkdir {name}: {error}"))?;
    model.insert(path, ino, model_inode(Kind::Dir, mode, uid, gid));
    Ok(())
}

fn apply_symlink(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    dirs: &[Vec<u8>],
    rng: &mut Rng,
    sequence: u32,
) -> Result<(), String> {
    let Some(dir) = pick(dirs, rng) else {
        return Ok(());
    };
    let name = format!("s{sequence}");
    let path = join(&dir, name.as_bytes());
    if model.paths.contains_key(&path) {
        return Ok(());
    }
    let len = 1 + rng.below(200);
    let target: Vec<u8> = (0..len)
        .map(|index| b'a' + ((index + sequence as usize) % 26) as u8)
        .collect();
    let uid = (rng.next() & 0xffff) as u32;
    let gid = (rng.next() & 0xffff) as u32;
    let parent = model.ino(&dir).ok_or("model dir missing")?;
    let ino = ufs
        .symlink(parent, name.as_bytes(), &target, uid, gid)
        .map_err(|error| format!("symlink {name}: {error}"))?;
    let mut node = model_inode(Kind::Symlink, 0o777, uid, gid);
    node.target = target;
    model.insert(path, ino, node);
    Ok(())
}

fn apply_link(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    dirs: &[Vec<u8>],
    non_dirs: &[Vec<u8>],
    rng: &mut Rng,
    sequence: u32,
) -> Result<(), String> {
    let Some(source) = pick(non_dirs, rng) else {
        return Ok(());
    };
    let Some(dir) = pick(dirs, rng) else {
        return Ok(());
    };
    let name = format!("h{sequence}");
    let path = join(&dir, name.as_bytes());
    if model.paths.contains_key(&path) {
        return Ok(());
    }
    let target = model.ino(&source).ok_or("model source missing")?;
    let parent = model.ino(&dir).ok_or("model dir missing")?;
    ufs.link(parent, name.as_bytes(), target)
        .map_err(|error| format!("link {name}: {error}"))?;
    let inode = model
        .inode(target)
        .cloned()
        .ok_or("model target inode missing")?;
    model.insert(path, target, inode);
    Ok(())
}

fn apply_unlink(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    non_dirs: &[Vec<u8>],
    rng: &mut Rng,
) -> Result<(), String> {
    let Some(path) = pick(non_dirs, rng) else {
        return Ok(());
    };
    let (parent, name) = split(&path);
    let parent_ino = model.ino(parent).ok_or("model parent missing")?;
    ufs.unlink(parent_ino, name)
        .map_err(|error| format!("unlink {}: {error}", String::from_utf8_lossy(&path)))?;
    model.remove_path(&path);
    Ok(())
}

fn apply_rmdir(ufs: &mut Ufs<VecDisk>, model: &mut Model, rng: &mut Rng) -> Result<(), String> {
    let candidates: Vec<Vec<u8>> = model
        .dirs()
        .into_iter()
        .filter(|path| !path.is_empty() && model.children(path).is_empty())
        .collect();
    let Some(path) = pick(&candidates, rng) else {
        return Ok(());
    };
    let (parent, name) = split(&path);
    let parent_ino = model.ino(parent).ok_or("model parent missing")?;
    ufs.rmdir(parent_ino, name)
        .map_err(|error| format!("rmdir {}: {error}", String::from_utf8_lossy(&path)))?;
    model.remove_path(&path);
    Ok(())
}

fn apply_write(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    files: &[Vec<u8>],
    rng: &mut Rng,
) -> Result<(), String> {
    let Some(path) = pick(files, rng) else {
        return Ok(());
    };
    let len = 1 + rng.below(2000);
    let offset = rng.below(16_000);
    if offset + len > 20_000 {
        return Ok(());
    }
    let data: Vec<u8> = (0..len).map(|_| (rng.next() & 0xff) as u8).collect();
    let ino = model.ino(&path).ok_or("model file missing")?;
    ufs.write(ino, offset as u64, &data)
        .map_err(|error| format!("write {}: {error}", String::from_utf8_lossy(&path)))?;
    let node = model.node_mut(&path).ok_or("model file inode missing")?;
    if node.data.len() < offset {
        node.data.resize(offset, 0);
    }
    let end = offset + len;
    if node.data.len() < end {
        node.data.resize(end, 0);
    }
    node.data[offset..end].copy_from_slice(&data);
    Ok(())
}

fn apply_truncate(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    files: &[Vec<u8>],
    rng: &mut Rng,
) -> Result<(), String> {
    let Some(path) = pick(files, rng) else {
        return Ok(());
    };
    let size = rng.below(20_000);
    let ino = model.ino(&path).ok_or("model file missing")?;
    ufs.truncate(ino, size as u64)
        .map_err(|error| format!("truncate {}: {error}", String::from_utf8_lossy(&path)))?;
    let node = model.node_mut(&path).ok_or("model file inode missing")?;
    node.data.resize(size, 0);
    Ok(())
}

fn apply_setattr(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    paths: &[Vec<u8>],
    rng: &mut Rng,
) -> Result<(), String> {
    let Some(path) = pick(paths, rng) else {
        return Ok(());
    };
    let mode = (rng.next() & 0o777) as u16;
    let uid = (rng.next() & 0xffff) as u32;
    let gid = (rng.next() & 0xffff) as u32;
    let ino = model.ino(&path).ok_or("model path missing")?;
    ufs.setattr(
        ino,
        SetAttr {
            mode: Some(mode),
            uid: Some(uid),
            gid: Some(gid),
            ..SetAttr::default()
        },
    )
    .map_err(|error| format!("setattr {}: {error}", String::from_utf8_lossy(&path)))?;
    let node = model.node_mut(&path).ok_or("model inode missing")?;
    node.mode = mode;
    node.uid = uid;
    node.gid = gid;
    Ok(())
}

fn apply_rename(
    ufs: &mut Ufs<VecDisk>,
    model: &mut Model,
    dirs: &[Vec<u8>],
    rng: &mut Rng,
    sequence: u32,
) -> Result<(), String> {
    let paths: Vec<Vec<u8>> = model
        .paths
        .keys()
        .filter(|path| !path.is_empty())
        .cloned()
        .collect();
    let Some(source) = pick(&paths, rng) else {
        return Ok(());
    };
    let Some(dir) = pick(dirs, rng) else {
        return Ok(());
    };
    let name = format!("r{sequence}");
    let destination = join(&dir, name.as_bytes());
    if destination == source {
        return Ok(());
    }
    let source_node = model.node(&source).ok_or("model source missing")?.clone();
    if source_node.kind == Kind::Dir {
        let mut prefix = Vec::from(source.as_slice());
        prefix.push(b'/');
        if destination == source || destination.starts_with(&prefix) {
            return Ok(());
        }
    }
    if let Some(target) = model.node(&destination) {
        let target_ino = model.ino(&destination).ok_or("model target missing")?;
        let source_ino = model.ino(&source).ok_or("model source ino missing")?;
        if target_ino == source_ino {
            return Ok(());
        }
        if (target.kind == Kind::Dir) != (source_node.kind == Kind::Dir) {
            return Ok(());
        }
        if target.kind == Kind::Dir && !model.children(&destination).is_empty() {
            return Ok(());
        }
    }
    let (source_parent, source_name) = split(&source);
    let source_parent_ino = model.ino(source_parent).ok_or("model source parent")?;
    let target_parent_ino = model.ino(&dir).ok_or("model target parent")?;
    ufs.rename(
        source_parent_ino,
        source_name,
        target_parent_ino,
        name.as_bytes(),
    )
    .map_err(|error| format!("rename {}: {error}", String::from_utf8_lossy(&source)))?;
    if model.paths.contains_key(&destination) {
        model.remove_path(&destination);
    }
    model.move_subtree(&source, &destination);
    Ok(())
}

fn run_seed(
    seed: u64,
    ops: u32,
    base: &[u8],
    image_path: &Path,
    vectors: &Path,
) -> Result<SeedReport, String> {
    let mut ufs = Ufs::mount(
        VecDisk::new(base.to_vec()),
        MountOptions { read_only: false },
    )
    .map_err(|error| format!("mount seed {seed}: {error}"))?;
    let image_root = ufs.root();
    let root = ufs
        .mkdir(image_root, b"b64", 0o755, 0, 0)
        .map_err(|error| format!("mkdir model root: {error}"))?;
    let mut model = Model::new(root);
    let mut rng = Rng::new(seed);
    let mut checks = 0u32;
    for sequence in 0..ops {
        let dirs = model.dirs();
        let files = model.files();
        let non_dirs = model.non_dirs();
        let paths: Vec<Vec<u8>> = model.paths.keys().cloned().collect();
        let choice = if model.paths.len() > 120 && rng.next().is_multiple_of(2) {
            if rng.next().is_multiple_of(2) { 4 } else { 5 }
        } else {
            rng.below(10)
        };
        match choice {
            0 => apply_create(&mut ufs, &mut model, &dirs, &mut rng, sequence)?,
            1 => apply_mkdir(&mut ufs, &mut model, &dirs, &mut rng, sequence)?,
            2 => apply_symlink(&mut ufs, &mut model, &dirs, &mut rng, sequence)?,
            3 => apply_link(&mut ufs, &mut model, &dirs, &non_dirs, &mut rng, sequence)?,
            4 => apply_unlink(&mut ufs, &mut model, &non_dirs, &mut rng)?,
            5 => apply_rmdir(&mut ufs, &mut model, &mut rng)?,
            6 => apply_write(&mut ufs, &mut model, &files, &mut rng)?,
            7 => apply_truncate(&mut ufs, &mut model, &files, &mut rng)?,
            8 => apply_setattr(&mut ufs, &mut model, &paths, &mut rng)?,
            9 => apply_rename(&mut ufs, &mut model, &dirs, &mut rng, sequence)?,
            _ => unreachable!(),
        }
        if sequence % 250 == 249 {
            compare::compare(&mut ufs, &model)
                .map_err(|error| format!("seed {seed} step {sequence}: {error}"))?;
            ufs.verify_allocation()
                .map_err(|error| format!("seed {seed} step {sequence}: {error}"))?;
            checks += 1;
        }
    }
    compare::compare(&mut ufs, &model).map_err(|error| format!("seed {seed} final: {error}"))?;
    ufs.verify_allocation()
        .map_err(|error| format!("seed {seed} final: {error}"))?;
    let disk = ufs
        .unmount()
        .map_err(|error| format!("seed {seed} unmount: {error}"))?;
    fs::write(image_path, &disk.data)
        .map_err(|error| format!("write {}: {error}", image_path.display()))?;
    let fsck = oracle::fsck(image_path)?;
    let image_check = oracle::check_ufs(
        image_path,
        &model,
        vectors,
        &image_path.with_extension("manifest"),
    )?;
    Ok(SeedReport {
        seed,
        operations: ops,
        checks: checks + 1,
        fsck,
        image_check,
    })
}

/// Run every configured seed against the same fresh golden image.
pub fn run(config: &Config) -> Result<Summary, String> {
    let base = load_golden("ufs2-b4096")?;
    let mut seeds = Vec::new();
    for seed in 0..config.seeds {
        let image_path = config.build.join(format!("seed-{seed:02}.img"));
        seeds.push(run_seed(
            seed,
            config.operations,
            &base,
            &image_path,
            &config.vectors,
        )?);
    }
    Ok(Summary { seeds })
}
