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

use raven_abi::types::Handle;
use raven_initfs::Initfs;
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

fn main(args: raven_rt::Args) -> i32 {
    let mut read = false;
    for index in 0..args.len() {
        if args.get(index) == Some("read") {
            read = true;
            break;
        }
    }
    let result = if read { read_result() } else { run() };
    match result {
        Ok(()) => 0,
        Err(error) => {
            raven_rt::println!("G1-ERROR {error}");
            1
        }
    }
}

fn read_result() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|error| format!("initfs: {error:?}"))?;
    let initfs = Initfs::parse(image).map_err(|error| format!("initfs parse: {error:?}"))?;
    let entry = initfs
        .get(MANIFEST)
        .ok_or_else(|| "manifest missing".to_string())?;
    let text = core::str::from_utf8(entry.data).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest = Manifest::parse(text)?;
    let Some(result) = manifest.result else {
        raven_rt::println!("G1-RESULT-ERROR no result path");
        return Ok(());
    };
    match read_file(&result) {
        Ok(bytes) => emit("G1-RESULT-BEGIN", &bytes, "G1-RESULT-END"),
        Err(error) => raven_rt::println!("G1-RESULT-ERROR {error}"),
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let image = raven_rt::map_initfs().map_err(|error| format!("initfs: {error:?}"))?;
    let initfs = Initfs::parse(image).map_err(|error| format!("initfs parse: {error:?}"))?;
    let entry = initfs
        .get(MANIFEST)
        .ok_or_else(|| "manifest missing".to_string())?;
    let text = core::str::from_utf8(entry.data).map_err(|_| "manifest is not UTF-8".to_string())?;
    let manifest = Manifest::parse(text)?;
    if manifest.mode == "exec" {
        // The kernel's `PROC_EXEC` loader copies the file tail; `raven-spawn`'s userland
        // image path is strict, so an EXEC wrapper is the portable way to pass argv/env.
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
    emit("G1-OUTPUT-BEGIN", &output, "G1-OUTPUT-END");
    raven_rt::println!("G1-EXIT {code}");

    if let Some(result) = &manifest.result {
        match read_file(result) {
            Ok(bytes) => emit("G1-RESULT-BEGIN", &bytes, "G1-RESULT-END"),
            Err(error) => raven_rt::println!("G1-RESULT-ERROR {error}"),
        }
    }
    Ok(())
}

fn emit(begin: &str, bytes: &[u8], end: &str) {
    raven_rt::println!("{begin}");
    let _ = raven_rt::debug_write(bytes);
    if !bytes.ends_with(b"\n") {
        let _ = raven_rt::debug_write(b"\n");
    }
    raven_rt::println!("{end}");
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
