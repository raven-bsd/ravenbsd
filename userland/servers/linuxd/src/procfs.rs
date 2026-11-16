// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Synthetic `/proc` and `/sys` content for the Linux personality.
//!
//! The values are deterministic and derived from the Linux context (pid, comm, argv, mappings,
//! break) or from the fixed M5 machine description; nothing is read from the host. Only the paths
//! listed in the B30 contract are served; everything else is `ENOENT`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::table::{DirEntry, LinuxProcess};

/// Linux `DT_REG`.
const DT_REG: u8 = 8;
/// Linux `DT_DIR`.
const DT_DIR: u8 = 4;
/// Linux `DT_LNK`.
const DT_LNK: u8 = 10;

/// One resolved synthetic path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Regular file content.
    File(Vec<u8>),
    /// Directory entries.
    Directory(Vec<DirEntry>),
    /// Symlink target.
    Symlink(Vec<u8>),
}

/// Resolve an absolute path for `process`.
///
/// # Errors
///
/// Returns [`Errno::ENOENT`] for unknown paths, [`Errno::EINVAL`] for a non-absolute path and
/// [`Errno::ENOTDIR`] for a file used as a directory.
pub fn lookup(process: &LinuxProcess, path: &[u8]) -> Result<Resource, Errno> {
    let path = core::str::from_utf8(path).map_err(|_| Errno::EINVAL)?;
    let path = normalize(path).ok_or(Errno::EINVAL)?;
    match path {
        "/proc" => Ok(Resource::Directory(proc_dir())),
        "/proc/self" => Ok(Resource::Directory(self_dir())),
        "/proc/self/exe" => Ok(Resource::Symlink(process.exe.clone())),
        "/proc/self/maps" => Ok(Resource::File(maps(process))),
        "/proc/self/fd" => Ok(Resource::Directory(fd_dir(process))),
        "/proc/self/status" => Ok(Resource::File(status(process))),
        "/proc/self/cmdline" => Ok(Resource::File(cmdline(process))),
        "/proc/self/stat" => Ok(Resource::File(stat(process))),
        "/proc/cpuinfo" => Ok(Resource::File(cpuinfo())),
        "/proc/meminfo" => Ok(Resource::File(meminfo())),
        "/sys" => Ok(Resource::Directory(sys_dir())),
        "/sys/devices" => Ok(Resource::Directory(devices_dir())),
        "/sys/devices/system" => Ok(Resource::Directory(system_dir())),
        "/sys/devices/system/cpu" => Ok(Resource::Directory(cpu_dir())),
        "/sys/devices/system/cpu/online" => Ok(Resource::File(b"0\n".to_vec())),
        _ => {
            if let Some(node) = process.root().lookup(path.as_bytes()) {
                if node.is_dir() {
                    return Ok(Resource::Directory(
                        process.root().entries(path.as_bytes()).unwrap_or_default(),
                    ));
                }
                // The root tree is metadata-only; file contents stay in the initfs.
                return Err(Errno::EACCES);
            }
            match path.strip_prefix("/proc/self/fd/") {
                Some(fd) => {
                    let fd = parse_fd(fd).ok_or(Errno::ENOENT)?;
                    process
                        .fd_target(fd)
                        .map(|target| Resource::Symlink(target.to_vec()))
                        .ok_or(Errno::ENOENT)
                }
                None => Err(Errno::ENOENT),
            }
        }
    }
}

fn normalize(path: &str) -> Option<&str> {
    if !path.starts_with('/') {
        return None;
    }
    if path.len() > 1 {
        Some(path.trim_end_matches('/'))
    } else {
        Some(path)
    }
}

fn parse_fd(text: &str) -> Option<i32> {
    if text.is_empty() || text.len() > 6 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn entry(name: &str, kind: u8, ino: u64) -> DirEntry {
    DirEntry {
        name: name.as_bytes().to_vec(),
        kind,
        ino,
    }
}

fn proc_dir() -> Vec<DirEntry> {
    vec![
        entry("self", DT_DIR, 1),
        entry("cpuinfo", DT_REG, 2),
        entry("meminfo", DT_REG, 3),
    ]
}

fn self_dir() -> Vec<DirEntry> {
    vec![
        entry("exe", DT_LNK, 10),
        entry("maps", DT_REG, 11),
        entry("fd", DT_DIR, 12),
        entry("status", DT_REG, 13),
        entry("cmdline", DT_REG, 14),
        entry("stat", DT_REG, 15),
    ]
}

fn fd_dir(process: &LinuxProcess) -> Vec<DirEntry> {
    let mut entries = vec![entry(".", DT_DIR, 1), entry("..", DT_DIR, 1)];
    for fd in 0..=2 {
        entries.push(entry(&format!("{fd}"), DT_LNK, 20 + u64::from(fd as u8)));
    }
    for (index, slot) in process.files().iter().enumerate() {
        if slot.is_some() {
            let fd = 3 + index as u32;
            entries.push(entry(&format!("{fd}"), DT_LNK, 20 + u64::from(fd)));
        }
    }
    entries
}

fn sys_dir() -> Vec<DirEntry> {
    vec![entry("devices", DT_DIR, 30)]
}

fn devices_dir() -> Vec<DirEntry> {
    vec![entry("system", DT_DIR, 31)]
}

fn system_dir() -> Vec<DirEntry> {
    vec![entry("cpu", DT_DIR, 32)]
}

fn cpu_dir() -> Vec<DirEntry> {
    vec![entry("online", DT_REG, 33)]
}

fn cmdline(process: &LinuxProcess) -> Vec<u8> {
    let mut out = Vec::new();
    for argument in &process.argv {
        out.extend_from_slice(argument);
        out.push(0);
    }
    if out.is_empty() {
        out.extend_from_slice(&process.exe);
        out.push(0);
    }
    out
}

fn maps(process: &LinuxProcess) -> Vec<u8> {
    let mut out = String::new();
    for region in &process.regions {
        let end = region.start.saturating_add(region.len);
        out.push_str(&format!(
            "{:08x}-{:08x} rw-p 00000000 00:00 0          [anon]\n",
            region.start, end
        ));
    }
    out.into_bytes()
}

fn status(process: &LinuxProcess) -> Vec<u8> {
    let name = String::from_utf8_lossy(&process.comm);
    let vm = process.regions.iter().map(|region| region.len).sum::<u64>();
    let text = format!(
        "Name:\t{name}\n\
         Umask:\t0022\n\
         State:\tR (running)\n\
         Tgid:\t{}\n\
         Pid:\t{}\n\
         PPid:\t0\n\
         Uid:\t0\t0\t0\t0\n\
         Gid:\t0\t0\t0\t0\n\
         Groups:\t\n\
         VmPeak:\t{} kB\n\
         VmSize:\t{} kB\n\
         VmRSS:\t{} kB\n\
         Threads:\t1\n\
         SigQ:\t0/0\n\
         SigPnd:\t0000000000000000\n\
         ShdPnd:\t0000000000000000\n\
         SigBlk:\t0000000000000000\n\
         SigIgn:\t0000000000000000\n\
         SigCgt:\t0000000000000000\n",
        process.pid,
        process.pid,
        vm / 1024,
        vm / 1024,
        vm / 1024,
    );
    text.into_bytes()
}

fn stat(process: &LinuxProcess) -> Vec<u8> {
    let name = String::from_utf8_lossy(&process.comm);
    let vm = process.regions.iter().map(|region| region.len).sum::<u64>();
    let rss = vm / 4096;
    let mut fields: Vec<String> = Vec::with_capacity(52);
    fields.push(process.pid.to_string());
    fields.push(format!("({name})"));
    fields.push("R".to_string());
    fields.push("0".to_string()); // ppid
    fields.push("0".to_string()); // pgrp
    fields.push("0".to_string()); // session
    fields.push("0".to_string()); // tty_nr
    fields.push("0".to_string()); // tpgid
    fields.push("0".to_string()); // flags
    fields.push("0".to_string()); // minflt
    fields.push("0".to_string()); // cminflt
    fields.push("0".to_string()); // majflt
    fields.push("0".to_string()); // cmajflt
    fields.push("0".to_string()); // utime
    fields.push("0".to_string()); // stime
    fields.push("0".to_string()); // cutime
    fields.push("0".to_string()); // cstime
    fields.push("20".to_string()); // priority
    fields.push("0".to_string()); // nice
    fields.push("1".to_string()); // threads
    fields.push("0".to_string()); // itrealvalue
    fields.push("0".to_string()); // starttime
    fields.push(vm.to_string()); // vsize
    fields.push(rss.to_string()); // rss
    fields.push("0".to_string()); // rsslim
    fields.push("0".to_string()); // startcode
    fields.push("0".to_string()); // endcode
    fields.push("0".to_string()); // startstack
    fields.push("0".to_string()); // kstkesp
    fields.push("0".to_string()); // kstkeip
    fields.push("0".to_string()); // signal
    fields.push("0".to_string()); // blocked
    fields.push("0".to_string()); // sigignore
    fields.push("0".to_string()); // sigcatch
    fields.push("0".to_string()); // wchan
    fields.push("0".to_string()); // nswap
    fields.push("0".to_string()); // cnswap
    fields.push("0".to_string()); // exit_signal
    fields.push("0".to_string()); // processor
    fields.push("0".to_string()); // rt_priority
    fields.push("0".to_string()); // policy
    fields.push("0".to_string()); // delayacct_blkio_ticks
    fields.push("0".to_string()); // guest_time
    fields.push("0".to_string()); // cguest_time
    fields.push("0".to_string()); // start_data
    fields.push("0".to_string()); // end_data
    fields.push(process.brk_start.to_string()); // start_brk
    fields.push("0".to_string()); // arg_start
    fields.push("0".to_string()); // arg_end
    fields.push("0".to_string()); // env_start
    fields.push("0".to_string()); // env_end
    fields.push("0".to_string()); // exit_code
    let mut out = fields.join(" ");
    out.push('\n');
    out.into_bytes()
}

fn cpuinfo() -> Vec<u8> {
    b"processor\t: 0\n\
      vendor_id\t: RavenBSD\n\
      cpu family\t: 6\n\
      model\t\t: 0\n\
      model name\t: RavenBSD virtual CPU\n\
      stepping\t: 0\n\
      cpu MHz\t\t: 1000.000\n\
      cache size\t: 0 KB\n\
      physical id\t: 0\n\
      siblings\t: 1\n\
      core id\t\t: 0\n\
      cpu cores\t: 1\n\
      flags\t\t: fpu pse tsc msr pae cx8 apic sep pge cmov pat\n"
        .to_vec()
}

fn meminfo() -> Vec<u8> {
    b"MemTotal:         262144 kB\n\
      MemFree:          131072 kB\n\
      MemAvailable:     196608 kB\n\
      Buffers:               0 kB\n\
      Cached:                0 kB\n\
      SwapCached:            0 kB\n\
      SwapTotal:             0 kB\n\
      SwapFree:              0 kB\n"
        .to_vec()
}

#[cfg(test)]
#[path = "procfs_ut.rs"]
mod tests;
