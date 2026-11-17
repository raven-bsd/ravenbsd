// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P5 POSIX filesystem behaviour fixture.
//!
//! Starts `initfsd`, `ramfsd` and `vfsd` from the initfs, then exercises the
//! POSIX behaviours through `file:` (vfsd/ramfsd) and the typed control
//! protocol. Every printed line is a hard assertion; a failure exits non-zero.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, prot};
use raven_initfs::Initfs;
use raven_proto_fs::{
    Call, CallRequest, CallResponse, DT_CHR, DT_FIFO, O_CREAT, O_RDONLY, O_WRONLY, S_IFCHR,
    S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, SetTime, Stat, StatVfs,
};
use raven_rt::entry;
use raven_rt::fs;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_scheme::Client;

entry!(main);

const START: &str = "/tmp/fs";
const READ_WRITE: u32 = 3;
const READ_ONLY: u32 = 1;
const PAGE: usize = 4096;
const MS_SYNC: usize = 4;

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("posix-fs: ok");
            0
        }
        Err(message) => {
            println!("posix-fs: fail {message}");
            1
        }
    }
}

fn run() -> Result<(), String> {
    start_services()?;
    setup()?;
    mkdir_rmdir()?;
    directory()?;
    path_resolution()?;
    stat_family()?;
    symlink()?;
    truncate()?;
    rename()?;
    hard_link()?;
    ownership()?;
    timestamp()?;
    special_file()?;
    statvfs()?;
    mmap_file()?;
    Ok(())
}

fn start_services() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|_| "initfs map".to_string())?;
    let initfs = Initfs::parse(image).map_err(|_| "initfs parse".to_string())?;
    for path in ["/sbin/initfsd", "/sbin/ramfsd", "/sbin/vfsd"] {
        spawn(
            &initfs,
            path,
            &SpawnOptions {
                args: &[],
                env: &[],
            },
        )
        .map_err(|_| format!("spawn {path}"))?;
        let scheme = match path {
            "/sbin/initfsd" => "initfs:",
            "/sbin/ramfsd" => "ramfs:",
            _ => "file:",
        };
        wait_scheme(scheme)?;
    }
    println!("posix-fs: services ok");
    Ok(())
}

fn wait_scheme(scheme: &str) -> Result<(), String> {
    for _ in 0..3000 {
        if fs::open(scheme, O_RDONLY).is_ok() {
            return Ok(());
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
    Err(format!("scheme {scheme} timeout"))
}

fn setup() -> Result<(), String> {
    match fs::mkdir(START, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(format!("mkdir start {}", error.description())),
    }
    mount_layout()?;
    Ok(())
}

fn mount_layout() -> Result<(), String> {
    let root = file_stat("/").map_err(|error| format!("stat root: {}", error.description()))?;
    let tmp = file_stat("/tmp").map_err(|error| format!("stat tmp: {}", error.description()))?;
    if root.mode & S_IFMT != S_IFDIR || tmp.mode & S_IFMT != S_IFDIR {
        return Err("root or /tmp is not a directory".to_string());
    }
    let mount = format!("{START}/mnt");
    match fs::mkdir(&mount, 0o755) {
        Ok(()) | Err(Errno::EEXIST) => {}
        Err(error) => return Err(format!("mkdir mount: {}", error.description())),
    }
    fs::mount(&mount, "ramfs:").map_err(|error| format!("mount: {}", error.description()))?;
    write_file(&format!("{mount}/nested"), b"nested")?;
    if file_read(&format!("{mount}/nested")).as_deref() != Ok(b"nested") {
        return Err("nested mount read".to_string());
    }
    fs::umount(&mount).map_err(|error| format!("umount: {}", error.description()))?;
    if file_stat(&format!("{mount}/nested")) != Err(Errno::ENOENT) {
        return Err("nested mount survived umount".to_string());
    }
    println!("posix-fs: mount ok");
    Ok(())
}

fn mkdir_rmdir() -> Result<(), String> {
    let dir = format!("{START}/dir");
    fs::mkdir(&dir, 0o755).map_err(|error| format!("mkdir dir: {}", error.description()))?;
    if fs::mkdir(&dir, 0o755) != Err(Errno::EEXIST) {
        return Err("duplicate mkdir".to_string());
    }
    fs::rmdir(&dir).map_err(|error| format!("rmdir dir: {}", error.description()))?;
    match file_stat(&dir) {
        Err(Errno::ENOENT) => {}
        other => return Err(format!("rmdir left {other:?}")),
    }
    fs::mkdir(&dir, 0o755).map_err(|error| format!("mkdir nonempty: {}", error.description()))?;
    write_file(&format!("{dir}/child"), b"x")?;
    if fs::rmdir(&dir) != Err(Errno::ENOTEMPTY) {
        return Err("rmdir accepted a non-empty directory".to_string());
    }
    fs::unlink(&format!("{dir}/child"))
        .map_err(|error| format!("unlink child: {}", error.description()))?;
    fs::rmdir(&dir).map_err(|error| format!("rmdir again: {}", error.description()))?;
    println!("posix-fs: mkdir-rmdir ok");
    Ok(())
}

fn directory() -> Result<(), String> {
    fs::mkdir(&format!("{START}/list"), 0o755)
        .map_err(|error| format!("mkdir list: {}", error.description()))?;
    write_file(&format!("{START}/list/a"), b"a")?;
    write_file(&format!("{START}/list/b"), b"b")?;
    let entries = file_read_dir(&format!("{START}/list"))
        .map_err(|error| format!("readdir: {}", error.description()))?;
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    if !names.contains(&"a") || !names.contains(&"b") || names.len() != 2 {
        return Err(format!("readdir returned {names:?}"));
    }
    let stat = file_stat(&format!("{START}/list"))
        .map_err(|error| format!("stat dir: {}", error.description()))?;
    if stat.mode & S_IFMT != S_IFDIR {
        return Err(format!("directory mode is {:o}", stat.mode));
    }
    println!("posix-fs: directory ok");
    Ok(())
}

fn path_resolution() -> Result<(), String> {
    write_file(&format!("{START}/path"), b"p")?;
    for path in [
        format!("{START}/./path"),
        format!("{START}//path"),
        format!("{START}/../fs/path"),
    ] {
        if file_stat(&path).is_err() {
            return Err(format!("stat {path}"));
        }
    }
    if file_stat(&format!("{START}/path/")) != Err(Errno::ENOTDIR) {
        return Err("trailing slash on a file".to_string());
    }
    if file_stat(&format!("{START}/../fs/missing")) != Err(Errno::ENOENT) {
        return Err("dotdot missing".to_string());
    }
    println!("posix-fs: path-resolution ok");
    Ok(())
}

fn stat_family() -> Result<(), String> {
    write_file(&format!("{START}/stat"), b"12345")?;
    let stat = file_stat(&format!("{START}/stat"))
        .map_err(|error| format!("stat file: {}", error.description()))?;
    if stat.mode & S_IFMT != S_IFREG || stat.size != 5 || stat.nlink != 1 {
        return Err(format!(
            "stat mode={:o} size={} nlink={}",
            stat.mode, stat.size, stat.nlink
        ));
    }
    let file = file_open(&format!("{START}/stat"), O_RDONLY)
        .map_err(|error| format!("open stat: {}", error.description()))?;
    let fstat = file
        .fstat()
        .map_err(|error| format!("fstat: {}", error.description()))?;
    let _ = file.close();
    if fstat.ino != stat.ino || fstat.size != stat.size {
        return Err("fstat disagrees with stat".to_string());
    }
    fs::symlink(&format!("{START}/statlink"), "stat")
        .map_err(|error| format!("stat symlink: {}", error.description()))?;
    let (op, bytes) = call(Call::Lstat {
        path: format!("{START}/statlink").as_bytes(),
    })?;
    match CallResponse::decode(op, &bytes).map_err(|_| "lstat decode".to_string())? {
        CallResponse::Stat { status: 0, stat } if stat.mode & S_IFMT == S_IFLNK => {}
        other => return Err(format!("lstat response {other:?}")),
    }
    println!("posix-fs: stat-family ok");
    Ok(())
}

fn symlink() -> Result<(), String> {
    write_file(&format!("{START}/target"), b"linked")?;
    fs::symlink(&format!("{START}/link"), "target")
        .map_err(|error| format!("symlink: {}", error.description()))?;
    let target = fs::readlink(&format!("{START}/link"))
        .map_err(|error| format!("readlink: {}", error.description()))?;
    if target != "target" {
        return Err(format!("readlink returned {target}"));
    }
    if file_read(&format!("{START}/link")).as_deref() != Ok(b"linked") {
        return Err("read through symlink".to_string());
    }
    fs::symlink(&format!("{START}/dangling"), "missing")
        .map_err(|error| format!("dangling: {}", error.description()))?;
    if file_stat(&format!("{START}/dangling")) != Err(Errno::ENOENT) {
        return Err("dangling link".to_string());
    }
    fs::symlink(&format!("{START}/loop"), "loop")
        .map_err(|error| format!("loop: {}", error.description()))?;
    if file_stat(&format!("{START}/loop")) != Err(Errno::ELOOP) {
        return Err("symlink loop".to_string());
    }
    println!("posix-fs: symlink ok");
    Ok(())
}

fn truncate() -> Result<(), String> {
    write_file(&format!("{START}/trunc"), b"0123456789")?;
    truncate_handle(&format!("{START}/trunc"), 4)?;
    if file_read(&format!("{START}/trunc")).as_deref() != Ok(b"0123") {
        return Err("truncate shrink".to_string());
    }
    truncate_handle(&format!("{START}/trunc"), 8)?;
    if file_read(&format!("{START}/trunc")).as_deref() != Ok(b"0123\0\0\0\0") {
        return Err("truncate extension".to_string());
    }
    println!("posix-fs: truncate ok");
    Ok(())
}

fn rename() -> Result<(), String> {
    write_file(&format!("{START}/from"), b"from")?;
    fs::rename(&format!("{START}/from"), &format!("{START}/to"))
        .map_err(|error| format!("rename: {}", error.description()))?;
    if file_read(&format!("{START}/to")).as_deref() != Ok(b"from") {
        return Err("rename lost data".to_string());
    }
    write_file(&format!("{START}/replace"), b"replace")?;
    fs::rename(&format!("{START}/replace"), &format!("{START}/to"))
        .map_err(|error| format!("replace rename: {}", error.description()))?;
    if file_read(&format!("{START}/to")).as_deref() != Ok(b"replace") {
        return Err("rename did not replace".to_string());
    }
    fs::mkdir(&format!("{START}/renamedir"), 0o755)
        .map_err(|error| format!("rename mkdir: {}", error.description()))?;
    fs::rename(
        &format!("{START}/renamedir"),
        &format!("{START}/renamedir2"),
    )
    .map_err(|error| format!("rename directory: {}", error.description()))?;
    if file_stat(&format!("{START}/renamedir")) != Err(Errno::ENOENT) {
        return Err("rename left the source".to_string());
    }
    println!("posix-fs: rename ok");
    Ok(())
}

fn hard_link() -> Result<(), String> {
    write_file(&format!("{START}/h1"), b"shared")?;
    call_status(Call::Link {
        path: format!("{START}/h2").as_bytes(),
        target: format!("{START}/h1").as_bytes(),
    })?;
    let stat = file_stat(&format!("{START}/h1"))
        .map_err(|error| format!("stat h1: {}", error.description()))?;
    if stat.nlink != 2 {
        return Err(format!("link count {}", stat.nlink));
    }
    if file_read(&format!("{START}/h2")).as_deref() != Ok(b"shared") {
        return Err("read through hard link".to_string());
    }
    fs::unlink(&format!("{START}/h1"))
        .map_err(|error| format!("unlink h1: {}", error.description()))?;
    let stat = file_stat(&format!("{START}/h2"))
        .map_err(|error| format!("stat h2: {}", error.description()))?;
    if stat.nlink != 1 || file_read(&format!("{START}/h2")).as_deref() != Ok(b"shared") {
        return Err("hard link after unlink".to_string());
    }
    call_status(Call::Link {
        path: format!("{START}/bad").as_bytes(),
        target: format!("{START}").as_bytes(),
    })
    .err()
    .ok_or_else(|| "hard link to a directory was accepted".to_string())?;
    println!("posix-fs: hard-link ok");
    Ok(())
}

fn ownership() -> Result<(), String> {
    write_file(&format!("{START}/own"), b"owner")?;
    fs::chmod(&format!("{START}/own"), 0o6755)
        .map_err(|error| format!("chmod own: {}", error.description()))?;
    call_status(Call::Chown {
        uid: 1000,
        gid: 100,
        path: format!("{START}/own").as_bytes(),
    })?;
    let stat = file_stat(&format!("{START}/own"))
        .map_err(|error| format!("stat own: {}", error.description()))?;
    if stat.uid != 1000 || stat.gid != 100 || stat.mode & 0o7777 != 0o755 {
        return Err(format!(
            "chown uid={} gid={} mode={:o}",
            stat.uid,
            stat.gid,
            stat.mode & 0o7777
        ));
    }
    fs::symlink(&format!("{START}/ownlink"), "own")
        .map_err(|error| format!("ownership symlink: {}", error.description()))?;
    call_status(Call::Lchown {
        uid: 2000,
        gid: u32::MAX,
        path: format!("{START}/ownlink").as_bytes(),
    })?;
    let target = file_stat(&format!("{START}/own"))
        .map_err(|error| format!("stat chown target: {}", error.description()))?;
    if target.uid != 1000 {
        return Err("lchown changed the target".to_string());
    }
    let client = Client::new();
    let handle = client
        .open(&format!("file:{START}/own"), READ_WRITE)
        .map_err(|error| format!("open own: {}", error.description()))?;
    let request = CallRequest::new(Call::Fchown {
        uid: u32::MAX,
        gid: 200,
    });
    let mut response = [0u8; 64];
    let result = client.call_typed(handle, &request, &mut response);
    let stat = client.fstat(handle);
    let _ = client.close(handle);
    let length = result.map_err(|error| format!("fchown call: {}", error.description()))?;
    status_response(request.op(), &response[..length])?;
    let stat = stat.map_err(|error| format!("fstat own: {}", error.description()))?;
    if stat.gid != 200 {
        return Err(format!("fchown gid={}", stat.gid));
    }
    println!("posix-fs: ownership ok");
    Ok(())
}

fn timestamp() -> Result<(), String> {
    write_file(&format!("{START}/time"), b"time")?;
    call_status(Call::Utimens {
        path: format!("{START}/time").as_bytes(),
        atime: SetTime::Set(111),
        mtime: SetTime::Set(222),
        follow: true,
    })?;
    let stat = file_stat(&format!("{START}/time"))
        .map_err(|error| format!("stat time: {}", error.description()))?;
    if (stat.atime, stat.mtime) != (111, 222) {
        return Err(format!("utimens atime={} mtime={}", stat.atime, stat.mtime));
    }
    call_status(Call::Utimens {
        path: format!("{START}/time").as_bytes(),
        atime: SetTime::Now,
        mtime: SetTime::Omit,
        follow: true,
    })?;
    let stat = file_stat(&format!("{START}/time"))
        .map_err(|error| format!("stat time now: {}", error.description()))?;
    if stat.atime == 111 || stat.mtime != 222 {
        return Err(format!(
            "utimens now atime={} mtime={}",
            stat.atime, stat.mtime
        ));
    }
    println!("posix-fs: timestamp ok");
    Ok(())
}

fn special_file() -> Result<(), String> {
    call_status(Call::Mkfifo {
        mode: 0o640,
        path: format!("{START}/fifo").as_bytes(),
    })?;
    let fifo = file_stat(&format!("{START}/fifo"))
        .map_err(|error| format!("stat fifo: {}", error.description()))?;
    if fifo.mode & S_IFMT != S_IFIFO {
        return Err(format!("fifo mode {:o}", fifo.mode));
    }
    call_status(Call::Mknod {
        mode: 0o020600,
        dev: 0x1234,
        path: format!("{START}/null").as_bytes(),
    })?;
    let node = file_stat(&format!("{START}/null"))
        .map_err(|error| format!("stat node: {}", error.description()))?;
    if node.mode & S_IFMT != S_IFCHR {
        return Err(format!("node mode {:o}", node.mode));
    }
    let entries = file_read_dir(START)
        .map_err(|error| format!("readdir special: {}", error.description()))?;
    let fifo_kind = entries
        .iter()
        .find(|entry| entry.name == "fifo")
        .map(|entry| entry.kind);
    let node_kind = entries
        .iter()
        .find(|entry| entry.name == "null")
        .map(|entry| entry.kind);
    if fifo_kind != Some(DT_FIFO) || node_kind != Some(DT_CHR) {
        return Err(format!(
            "readdir kinds fifo={fifo_kind:?} node={node_kind:?}"
        ));
    }
    println!("posix-fs: special-file ok");
    Ok(())
}

fn statvfs() -> Result<(), String> {
    let client = Client::new();
    let handle = client
        .open(&format!("file:{START}"), READ_ONLY)
        .map_err(|error| format!("statvfs open: {}", error.description()))?;
    let request = CallRequest::new(Call::Statvfs);
    let mut response = [0u8; 128];
    let result = client.call_typed(handle, &request, &mut response);
    let _ = client.close(handle);
    let length = result.map_err(|error| format!("statvfs call: {}", error.description()))?;
    let op = request.op();
    let bytes = &response[..length];
    match CallResponse::decode(op, bytes).map_err(|_| "statvfs decode".to_string())? {
        CallResponse::StatVfs { status: 0, statvfs } => check_statvfs(&statvfs)?,
        CallResponse::StatVfs { status, .. } | CallResponse::Status(status) => {
            return Err(format!("statvfs status {status}"));
        }
        other => return Err(format!("statvfs response {other:?}")),
    }
    println!("posix-fs: statvfs ok");
    Ok(())
}

fn mmap_file() -> Result<(), String> {
    let path = format!("{START}/mmap");
    let data = [0x41u8; 2 * PAGE];
    write_file(&path, &data)?;
    let shared = attach_mmap(&path, 0, 2 * PAGE as u64, true)
        .map_err(|error| format!("shared mmap: {}", error.description()))?;
    let address = raven_rt::sys::map_object(
        shared,
        0,
        2 * PAGE,
        (prot::READ | prot::WRITE) as usize,
        true,
    )
    .map_err(|error| format!("shared map: {}", error.description()))?;
    raven_rt::sys::copy_to_mapping(address, b"MMAP");
    raven_rt::sys::msync(Handle::SELF_PROC, address, 2 * PAGE, MS_SYNC)
        .map_err(|error| format!("msync: {}", error.description()))?;
    let contents =
        file_read(&path).map_err(|error| format!("read shared: {}", error.description()))?;
    if contents.get(..4) != Some(b"MMAP") {
        return Err("shared write-back".to_string());
    }
    raven_rt::sys::unmap(Handle::SELF_PROC, address, 2 * PAGE)
        .map_err(|error| format!("shared unmap: {}", error.description()))?;

    let private = attach_mmap(&path, 0, PAGE as u64, false)
        .map_err(|error| format!("private mmap: {}", error.description()))?;
    let address =
        raven_rt::sys::map_object(private, 0, PAGE, (prot::READ | prot::WRITE) as usize, false)
            .map_err(|error| format!("private map: {}", error.description()))?;
    raven_rt::sys::copy_to_mapping(address, b"PRIV");
    raven_rt::sys::unmap(Handle::SELF_PROC, address, PAGE)
        .map_err(|error| format!("private unmap: {}", error.description()))?;
    let contents =
        file_read(&path).map_err(|error| format!("read private: {}", error.description()))?;
    if contents.get(..4) != Some(b"MMAP") {
        return Err("private mapping changed the file".to_string());
    }

    if attach_mmap(&path, 2 * PAGE as u64, PAGE as u64, false) != Err(Errno::EINVAL) {
        return Err("mmap accepted an offset past EOF".to_string());
    }
    println!("posix-fs: mmap-file ok");
    Ok(())
}

fn attach_mmap(path: &str, offset: u64, len: u64, shared: bool) -> Result<Handle, Errno> {
    let client = Client::new();
    let handle = client.open(&format!("file:{path}"), READ_WRITE)?;
    let request = CallRequest::new(Call::Mmap {
        offset,
        len,
        shared,
    });
    let mut encoded = [0u8; 64];
    let encoded_len = request
        .encode(&mut encoded)
        .map_err(|error| error.errno())?;
    let mut response = [0u8; 8];
    let result = client.call(handle, &encoded[..encoded_len], &mut response);
    let count = result?;
    if count == 4 {
        let status = i32::from_le_bytes(response[..4].try_into().map_err(|_| Errno::EPROTO)?);
        return Err(errno_from_status(status));
    }
    if count < 8 {
        return Err(Errno::EPROTO);
    }
    Ok(Handle(u32::from_le_bytes(
        response[..4].try_into().map_err(|_| Errno::EPROTO)?,
    )))
}

fn errno_from_status(status: i32) -> Errno {
    if (-4095..0).contains(&status) {
        Errno(u16::try_from(-status).unwrap_or(Errno::EIO.0))
    } else {
        Errno::EIO
    }
}

fn check_statvfs(statvfs: &StatVfs) -> Result<(), String> {
    if statvfs.block_size == 0
        || statvfs.fragment_size == 0
        || statvfs.blocks == 0
        || statvfs.free_blocks > statvfs.blocks
        || statvfs.name_max == 0
    {
        return Err(format!("statvfs {statvfs:?}"));
    }
    Ok(())
}

fn file_open(path: &str, flags: u32) -> Result<fs::File, Errno> {
    fs::open(&format!("file:{path}"), flags)
}

fn file_stat(path: &str) -> Result<Stat, Errno> {
    fs::stat(&format!("file:{path}"))
}

fn file_read(path: &str) -> Result<Vec<u8>, Errno> {
    fs::read_to_vec(&format!("file:{path}"))
}

fn file_read_dir(path: &str) -> Result<Vec<fs::DirEntry>, Errno> {
    fs::read_dir(&format!("file:{path}"))
}

fn write_file(path: &str, data: &[u8]) -> Result<(), String> {
    let mut file = file_open(path, O_WRONLY | O_CREAT)
        .map_err(|error| format!("open {path}: {}", error.description()))?;
    let written = file
        .write(data)
        .map_err(|error| format!("write {path}: {}", error.description()))?;
    if written != data.len() {
        return Err(format!("short write {path}"));
    }
    file.close()
        .map_err(|error| format!("close {path}: {}", error.description()))
}

fn truncate_handle(path: &str, size: u64) -> Result<(), String> {
    let client = Client::new();
    let handle = client
        .open(&format!("file:{path}"), READ_WRITE)
        .map_err(|error| format!("truncate open: {}", error.description()))?;
    let request = CallRequest::new(Call::Truncate { size });
    let mut response = [0u8; 64];
    let result = client.call_typed(handle, &request, &mut response);
    let _ = client.close(handle);
    let length = result.map_err(|error| format!("truncate call: {}", error.description()))?;
    status_response(request.op(), &response[..length])
}

fn call(call: Call<'_>) -> Result<(u16, Vec<u8>), String> {
    let client = Client::new();
    let root = client
        .open("file:/", READ_ONLY)
        .map_err(|error| format!("root open: {}", error.description()))?;
    let request = CallRequest::new(call);
    let mut response = [0u8; 512];
    let result = client.call_typed(root, &request, &mut response);
    let _ = client.close(root);
    let length =
        result.map_err(|error| format!("call {:?}: {}", request.op(), error.description()))?;
    Ok((request.op(), response[..length].to_vec()))
}

fn call_status(request: Call<'_>) -> Result<(), String> {
    let (op, bytes) = call(request)?;
    status_response(op, &bytes)
}

fn status_response(op: u16, bytes: &[u8]) -> Result<(), String> {
    match CallResponse::decode(op, bytes).map_err(|_| "response decode".to_string())? {
        CallResponse::Status(0) => Ok(()),
        CallResponse::Status(status)
        | CallResponse::Readlink { status, .. }
        | CallResponse::StatFs { status, .. }
        | CallResponse::StatVfs { status, .. }
        | CallResponse::Stat { status, .. } => Err(format!("status {status}")),
    }
}
