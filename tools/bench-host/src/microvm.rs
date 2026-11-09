// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal QEMU microVM boot time and QEMU RSS at READY.

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::result::BenchResult;
use crate::setup::{ensure_microvm_init, write_initramfs};

const RUNS: usize = 10;
const READY_TIMEOUT: Duration = Duration::from_secs(20);

pub(crate) fn run() -> Result<()> {
    if let Err(error) = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/kvm")
    {
        return emit_unmeasurable(format!("not measurable: /dev/kvm unavailable: {error}"));
    }

    let kernel = kernel_path();
    if let Err(error) = fs::File::open(&kernel) {
        return emit_unmeasurable(format!(
            "not measurable: kernel {} is not readable: {error}",
            kernel.display()
        ));
    }
    let init = ensure_microvm_init()?;
    let initramfs = write_initramfs(&init)?;

    let mut latencies = Vec::new();
    let mut rss = Vec::new();
    let mut failures = Vec::new();
    for _ in 0..RUNS {
        match boot_once(&kernel, &initramfs) {
            Ok((latency, memory)) => {
                latencies.push(latency.as_nanos() as f64);
                rss.push(memory as f64);
            }
            Err(error) => failures.push(format!("{error:#}")),
        }
    }
    if latencies.is_empty() {
        return emit_unmeasurable(format!(
            "not measurable: QEMU microVM boot failed: {}",
            failures.join("; ")
        ));
    }
    let note = if failures.is_empty() {
        Some(format!(
            "kernel={}; qemu -M microvm -enable-kvm",
            kernel.display()
        ))
    } else {
        Some(format!(
            "kernel={}; {} of {RUNS} boots failed: {}",
            kernel.display(),
            failures.len(),
            failures.join("; ")
        ))
    };
    let latency = BenchResult::from_samples("microvm-boot", "ns/op", &latencies, note.clone());
    latency.print();
    latency.write()?;
    let memory = BenchResult::from_samples("microvm-rss", "bytes", &rss, note);
    memory.print();
    memory.write()?;
    Ok(())
}

fn kernel_path() -> PathBuf {
    let release = fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| String::from("unknown"));
    Path::new("/boot").join(format!("vmlinuz-{release}"))
}

fn boot_once(kernel: &Path, initramfs: &Path) -> Result<(Duration, u64)> {
    let start = Instant::now();
    let mut child = Command::new("qemu-system-x86_64")
        .args([
            "-M",
            "microvm",
            "-enable-kvm",
            "-m",
            "128",
            "-nographic",
            "-kernel",
        ])
        .arg(kernel)
        .arg("-initrd")
        .arg(initramfs)
        .args(["-append", "console=ttyS0 panic=-1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("cannot start qemu-system-x86_64")?;

    let stdout = child.stdout.take().context("QEMU has no stdout")?;
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });

    let deadline = Instant::now() + READY_TIMEOUT;
    let mut ready = false;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match receiver.recv_timeout(remaining) {
            Ok(line) if line.contains("READY") => {
                ready = true;
                break;
            }
            Ok(_) => {}
            Err(_) => break,
        }
    }
    let elapsed = start.elapsed();
    let rss = if ready {
        read_vm_rss(child.id()).unwrap_or(0)
    } else {
        0
    };
    let _ = child.kill();
    let _ = child.wait();
    if !ready {
        bail!("QEMU did not print READY within {READY_TIMEOUT:?}");
    }
    Ok((elapsed, rss))
}

fn read_vm_rss(pid: u32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    parse_vmrss(&status)
}

fn parse_vmrss(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let kib = line.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    Some(kib * 1024)
}

fn emit_unmeasurable(note: String) -> Result<()> {
    let result = BenchResult::from_samples("microvm-boot", "ns/op", &[], Some(note));
    result.print();
    result.write()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_running_kernel_path() {
        let path = kernel_path();
        assert_eq!(path.parent(), Some(Path::new("/boot")));
        assert!(
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("vmlinuz-"))
        );
    }

    #[test]
    fn parses_vmrss() {
        assert_eq!(
            parse_vmrss("Name:\tqemu\nVmRSS:\t  1234 kB\n"),
            Some(1_263_616)
        );
        assert_eq!(parse_vmrss("Name:\tqemu\n"), None);
    }
}
