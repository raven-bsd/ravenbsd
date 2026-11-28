// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Guest argv/env wrapper for the G1 target runner.
//!
//! Runs as the `g1` service under `raven-init`. `/g1/manifest` names one program, its argv,
//! its environment and an optional result file. The wrapper spawns the program with a pipe on
//! stdout/stderr, prints the captured output, the exit code and the result file between stable
//! `G1-*` markers, and exits 0. `raven-init` is configured with `exit_when_exits = "g1"`, so its
//! exit status is the wrapper's; the host reads the program's own status from `G1-EXIT`.

#![no_std]
#![no_main]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_proto_fs::{O_CREAT, O_TRUNC, O_WRONLY, S_IFDIR, S_IFMT};
use raven_proto_svc::{State, Status};
use raven_rt::entry;
use raven_rt::spawn::{RtSys, SpawnOptions, spawn_image_with_fdmap};
use raven_scheme::Client;

entry!(main);

/// The manifest path inside the initfs.
const MANIFEST: &str = "/g1/manifest";
/// `file:` open for reading.
const OPEN_READ: u32 = 1;
/// Largest captured stream kept for the host.
const CAPTURE_LIMIT: usize = 1024 * 1024;
/// FreeBSD base root skeleton: `/root` is the 0700 root home; `.profile` is 0644.
const ROOT_PATH: &str = "/root";
const ROOT: &str = "file:/root";
const PROFILE_PATH: &str = "file:/root/.profile";
const PROFILE_ABSOLUTE: &str = "/root/.profile";
const PROFILE: &[u8] = include_bytes!("../../../guest-root/profile");
/// A second ramfs instance carries the donor `/root` mode; the default ramfs root is 0755.
const ROOT_SCHEME: &str = "rootfs:";

struct Manifest {
    mode: String,
    program: String,
    args: Vec<String>,
    env: Vec<String>,
    result: Option<String>,
}

impl Manifest {
    fn parse(text: &str) -> Result<Self, String> {
        let mut manifest = Self {
            mode: "spawn".to_string(),
            program: String::new(),
            args: Vec::new(),
            env: Vec::new(),
            result: None,
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("bad manifest line: {line}"));
            };
            let value = value.trim();
            match key.trim() {
                "mode" => manifest.mode = value.to_string(),
                "program" => manifest.program = value.to_string(),
                "arg" => manifest.args.push(value.to_string()),
                "env" => manifest.env.push(value.to_string()),
                "result" => manifest.result = Some(value.to_string()),
                other => return Err(format!("unknown manifest key: {other}")),
            }
        }
        if manifest.program.is_empty() {
            return Err("manifest has no program".to_string());
        }
        if manifest.args.is_empty() {
            manifest.args.push(manifest.program.clone());
        }
        Ok(manifest)
    }
}

/// Reproduce the FreeBSD base `/root` skeleton before a donor test runs.
///
/// The target guest root is the read-only initfs, whose synthesized directories are 0755, while
/// the FreeBSD base environment has `/root` at 0700. A second ramfs instance started by the guest
/// service set supplies the donor mode; this process is root, so it mounts that instance at
/// `/root` and writes the base `.profile`. The steps are idempotent: the donor assertions are
/// the `link("/root", ...)` / `link("/root/.profile", ...)` pair in NetBSD `t_link.c`.
fn ensure_root_skeleton() -> Result<(), String> {
    match raven_rt::fs::stat(ROOT) {
        Ok(stat) if stat.mode & S_IFMT == S_IFDIR => {}
        Ok(_) => return Err(format!("{ROOT} exists but is not a directory")),
        Err(Errno::ENOENT) => match raven_rt::fs::mount(ROOT_PATH, ROOT_SCHEME) {
            Ok(()) | Err(Errno::EEXIST) => {}
            Err(error) => return Err(format!("mount {ROOT}: {error:?}")),
        },
        Err(error) => return Err(format!("stat {ROOT}: {error:?}")),
    }
    let root = raven_rt::fs::stat(ROOT).map_err(|error| format!("stat {ROOT}: {error:?}"))?;
    if root.mode & S_IFMT != S_IFDIR {
        return Err(format!("{ROOT} is not a directory"));
    }
    if root.mode & 0o7777 != 0o700 {
        raven_rt::fs::chmod(ROOT_PATH, 0o700)
            .map_err(|error| format!("chmod {ROOT_PATH}: {error:?}"))?;
    }
    // The open protocol carries no creation mode; set the donor 0644 explicitly after writing.
    let mut profile = raven_rt::fs::open(PROFILE_PATH, O_WRONLY | O_CREAT | O_TRUNC)
        .map_err(|error| format!("open {PROFILE_PATH}: {error:?}"))?;
    profile
        .write(PROFILE)
        .map_err(|error| format!("write {PROFILE_PATH}: {error:?}"))?;
    profile
        .close()
        .map_err(|error| format!("close {PROFILE_PATH}: {error:?}"))?;
    raven_rt::fs::chmod(PROFILE_ABSOLUTE, 0o644)
        .map_err(|error| format!("chmod {PROFILE_ABSOLUTE}: {error:?}"))?;
    Ok(())
}

fn main(args: raven_rt::Args) -> i32 {
    let mut read = false;
    let mut root = false;
    let mut case = None;
    for index in 0..args.len() {
        if let Some(value) = args.get(index) {
            if value == "read" {
                read = true;
            }
            if value == "root" {
                root = true;
            }
            if let Ok(number) = value.parse::<usize>() {
                case = Some(number);
            }
        }
    }
    let result = if root {
        ensure_root_skeleton()
    } else if read {
        read_result(case)
    } else {
        run(case)
    };
    match result {
        Ok(()) => 0,
        Err(error) => {
            raven_rt::println!("G1-ERROR {error}");
            1
        }
    }
}

fn manifest_path(case: Option<usize>) -> String {
    match case {
        Some(index) => format!("/g1/manifest-{index}"),
        None => MANIFEST.to_string(),
    }
}

fn read_result(case: Option<usize>) -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|error| format!("initfs: {error:?}"))?;
    let initfs = Initfs::parse(image).map_err(|error| format!("initfs parse: {error:?}"))?;
    let path = manifest_path(case);
    let entry = initfs
        .get(&path)
        .ok_or_else(|| format!("manifest missing: {path}"))?;
    let text = core::str::from_utf8(entry.data).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest = Manifest::parse(text)?;
    let Some(result) = manifest.result else {
        raven_rt::println!("G1-RESULT-MISSING {case:?} no result path");
        return Ok(());
    };
    for attempt in 0..1000 {
        match read_file(&result) {
            Ok(bytes) if !bytes.is_empty() => {
                emit(case, "G1-RESULT", &bytes);
                return Ok(());
            }
            Ok(_) | Err(_) => {
                if attempt % 50 == 0 && run_finished(case) {
                    if let Ok(bytes) = read_file(&result) {
                        if !bytes.is_empty() {
                            emit(case, "G1-RESULT", &bytes);
                            return Ok(());
                        }
                    }
                    raven_rt::println!("G1-RESULT-MISSING {case:?} run finished without result");
                    return Ok(());
                }
                let _ = raven_rt::nanosleep(10_000_000);
            }
        }
    }
    raven_rt::println!("G1-RESULT-MISSING {case:?} {result}");
    Ok(())
}

fn run_finished(case: Option<usize>) -> bool {
    let Some(index) = case else {
        return false;
    };
    let client = Client::new();
    let path = format!("svc:/g1-run-{index}");
    let Ok(handle) = client.open(&path, OPEN_READ) else {
        return false;
    };
    let mut buffer = [0u8; 128];
    let state = client
        .read(handle, 0, &mut buffer)
        .ok()
        .and_then(|read| Status::decode(&buffer[..read]).ok())
        .map(|status| status.state);
    let _ = client.close(handle);
    matches!(state, Some(State::Failed | State::GaveUp | State::Stopped))
}

fn run(case: Option<usize>) -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|error| format!("initfs: {error:?}"))?;
    let initfs = Initfs::parse(image).map_err(|error| format!("initfs parse: {error:?}"))?;
    let path = manifest_path(case);
    let entry = initfs
        .get(&path)
        .ok_or_else(|| format!("manifest missing: {path}"))?;
    let text = core::str::from_utf8(entry.data).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest = Manifest::parse(text)?;
    // The donor base environment has the `/root` skeleton; these guests have no login process,
    // so the root wrapper materializes it before the test starts.
    ensure_root_skeleton()?;
    if manifest.mode == "exec" {
        // The kernel's `PROC_EXEC` loader copies the file tail; `raven-spawn`'s userland
        // image path is strict, so an EXEC wrapper is the portable way to pass argv/env.
        if let Some(index) = case {
            raven_rt::println!("G1-RUN {index}");
        }
        let argv: Vec<&[u8]> = manifest.args.iter().map(|arg| arg.as_bytes()).collect();
        let envp: Vec<&[u8]> = manifest.env.iter().map(|item| item.as_bytes()).collect();
        return match raven_rt::exec::execve(manifest.program.as_bytes(), &argv, &envp, &[]) {
            Ok(()) => Err("execve returned without replacing the image".to_string()),
            Err(error) => Err(format!("execve: {error:?}")),
        };
    }
    let program = initfs
        .get(&manifest.program)
        .ok_or_else(|| format!("program missing: {}", manifest.program))?;

    let argv: Vec<&str> = manifest.args.iter().map(String::as_str).collect();
    let envp: Vec<&str> = manifest.env.iter().map(String::as_str).collect();
    let options = SpawnOptions {
        args: &argv,
        env: &envp,
    };
    let mut image = program.data.to_vec();
    // `raven-spawn` maps file-tail pages, so an initfs entry must end on a page boundary.
    let aligned = image.len().saturating_add(4095) & !4095;
    image.resize(aligned, 0);
    let (read, write) = raven_rt::pipe::pipe().map_err(|error| format!("pipe: {error:?}"))?;
    let child = spawn_image_with_fdmap(
        &image,
        &manifest.program,
        &options,
        &[(1, write.0), (2, write.0)],
        &mut RtSys,
    )
    .map_err(|error| format!("spawn: {error:?}"))?;
    let _ = raven_rt::handle_close(write);

    let mut code = -1i32;
    RtSys
        .proc_wait(child, &mut code)
        .map_err(|error| format!("wait: {error:?}"))?;
    let output = read_pipe(read)?;
    emit(None, "G1-OUTPUT", &output);
    raven_rt::println!("G1-EXIT {code}");

    if let Some(result) = &manifest.result {
        match read_file(result) {
            Ok(bytes) => emit(None, "G1-RESULT", &bytes),
            Err(error) => raven_rt::println!("G1-RESULT-ERROR {error}"),
        }
    }
    Ok(())
}

fn emit(case: Option<usize>, label: &str, bytes: &[u8]) {
    match case {
        Some(index) => {
            raven_rt::println!("{label}-BEGIN {index}");
            let _ = raven_rt::debug_write(bytes);
            if !bytes.ends_with(b"\n") {
                let _ = raven_rt::debug_write(b"\n");
            }
            raven_rt::println!("{label}-END {index}");
        }
        None => {
            raven_rt::println!("{label}-BEGIN");
            let _ = raven_rt::debug_write(bytes);
            if !bytes.ends_with(b"\n") {
                let _ = raven_rt::debug_write(b"\n");
            }
            raven_rt::println!("{label}-END");
        }
    }
}

fn read_pipe(handle: Handle) -> Result<Vec<u8>, String> {
    let mut data = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let read = raven_rt::sys::read(handle, &mut buffer)
            .map_err(|error| format!("pipe read: {error:?}"))?;
        if read == 0 || data.len() >= CAPTURE_LIMIT {
            break;
        }
        data.extend_from_slice(&buffer[..read]);
    }
    let _ = raven_rt::handle_close(handle);
    Ok(data)
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    let client = Client::new();
    let scheme = if path.contains(':') {
        path.to_string()
    } else {
        format!("file:{path}")
    };
    let handle = client
        .open(&scheme, OPEN_READ)
        .map_err(|error| format!("open {scheme}: {error:?}"))?;
    let mut data = Vec::new();
    let mut offset = 0u64;
    let mut buffer = [0u8; 4096];
    loop {
        let read = client
            .read(handle, offset, &mut buffer)
            .map_err(|error| format!("read {scheme}: {error:?}"))?;
        if read == 0 || data.len() >= CAPTURE_LIMIT {
            break;
        }
        data.extend_from_slice(&buffer[..read]);
        offset = offset.saturating_add(read as u64);
    }
    let _ = client.close(handle);
    Ok(data)
}
