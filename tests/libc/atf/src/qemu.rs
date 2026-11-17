// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! QEMU execution of one built target program.
//!
//! The ravenbsd kernel runs `/init` as PID 1, prints `init exited (N)` when it exits
//! (`kernel/src/process/mod.rs`), and with `test-exit=1` maps N = 0 to QEMU status 33 and any
//! other N to 35. The runner therefore packs one exit-status program as `/init`, boots the base
//! image, and reads N from the serial log. The program's output stays beside the log for
//! diagnosis; the 320 MiB boot image is removed after the run.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::Verdict;
use crate::runner::Observation;

/// Default x86_64 firmware pair, matching `xtask` and the qtest harness.
const OVMF_CODE: &str = "/usr/share/OVMF/OVMF_CODE_4M.fd";
const OVMF_VARS: &str = "/usr/share/OVMF/OVMF_VARS_4M.fd";
/// `test-exit` enables the C5 `isa-debug-exit` protocol; `console=ttyS0` puts the log on stdio.
const CMDLINE: &str = "console=ttyS0 test-exit=1";
/// Poll interval while waiting for QEMU.
const POLL: Duration = Duration::from_millis(50);
/// Largest serial prefix kept for the reason/error text.
const SERIAL_LIMIT: usize = 64 * 1024;

/// Where the target kernel and the QEMU tools come from.
#[derive(Clone, Debug)]
pub struct QemuConfig {
    /// Repository root.
    pub root: PathBuf,
    /// Target kernel ELF.
    pub kernel: PathBuf,
    /// Maximum wall-clock seconds for one boot.
    pub timeout_secs: u64,
}

impl QemuConfig {
    /// Build a config for the x86_64 target kernel under `build/x86_64`.
    #[must_use]
    pub fn new(root: &Path, timeout_secs: u64) -> Self {
        Self {
            root: root.to_path_buf(),
            kernel: root.join("build/x86_64/kernel.elf"),
            timeout_secs,
        }
    }
}

/// Run one exit-status program as `/init` and judge its exit code.
pub fn run_init_program(
    config: &QemuConfig,
    program: &Path,
    expect_exit: i32,
    workdir: &Path,
    keep: bool,
) -> Result<Observation> {
    fs::create_dir_all(workdir).with_context(|| format!("cannot create {}", workdir.display()))?;
    let initfs = workdir.join("initfs.img");
    let image = workdir.join("boot.img");
    let vars = workdir.join("OVMF_VARS.fd");
    let serial_path = workdir.join("serial.log");
    let error_path = workdir.join("qemu.err");
    for stale in [&initfs, &image, &vars, &serial_path, &error_path] {
        let _ = fs::remove_file(stale);
    }

    make_initfs(config, program, &initfs)?;
    make_image(config, &initfs, &image)?;
    copy_firmware_vars(&vars)?;
    let (status, timed_out) = boot(config, &image, &vars, &serial_path, &error_path)?;
    let serial = read_prefix(&serial_path);
    let text = String::from_utf8_lossy(&serial);
    if !keep {
        // A boot image is 320 MiB; the serial log and the error log are the useful evidence.
        for transient in [&initfs, &image, &vars] {
            let _ = fs::remove_file(transient);
        }
    }
    Ok(judge(
        &text,
        status,
        timed_out,
        expect_exit,
        config.timeout_secs,
    ))
}

/// Pack the program as `/init`.
fn make_initfs(config: &QemuConfig, program: &Path, initfs: &Path) -> Result<()> {
    let args = mkinitfs_args(initfs, program);
    run_tool(
        "cargo",
        &args,
        &config.root,
        "cannot build the target initfs",
    )
}

/// Assemble the UEFI boot image that holds the kernel and the initfs.
fn make_image(config: &QemuConfig, initfs: &Path, image: &Path) -> Result<()> {
    if !config.kernel.is_file() {
        bail!(
            "target kernel not found: {} (build the x86_64 kernel first)",
            config.kernel.display()
        );
    }
    let args = image_args(&config.kernel, initfs, image);
    run_tool(
        "cargo",
        &args,
        &config.root,
        "cannot assemble the target boot image",
    )
}

fn copy_firmware_vars(destination: &Path) -> Result<()> {
    let source =
        std::env::var_os("RAVEN_OVMF_VARS").map_or_else(|| PathBuf::from(OVMF_VARS), PathBuf::from);
    if !source.is_file() {
        bail!(
            "required host tool OVMF vars not found: {}",
            source.display()
        );
    }
    fs::copy(&source, destination).with_context(|| {
        format!(
            "cannot copy OVMF vars {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

/// Boot the image and return QEMU's exit status plus whether the deadline expired.
fn boot(
    config: &QemuConfig,
    image: &Path,
    vars: &Path,
    serial_path: &Path,
    error_path: &Path,
) -> Result<(Option<i32>, bool)> {
    let code =
        std::env::var_os("RAVEN_OVMF_CODE").map_or_else(|| PathBuf::from(OVMF_CODE), PathBuf::from);
    if !code.is_file() {
        bail!("required host tool OVMF code not found: {}", code.display());
    }
    let args = qemu_args(image, &code, vars);
    let serial = fs::File::create(serial_path)
        .with_context(|| format!("cannot create {}", serial_path.display()))?;
    let errors = fs::File::create(error_path)
        .with_context(|| format!("cannot create {}", error_path.display()))?;
    let mut command = Command::new("qemu-system-x86_64");
    command
        .args(&args)
        .stdin(Stdio::null())
        .stdout(serial)
        .stderr(errors);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .with_context(|| "required host tool `qemu-system-x86_64` is missing")?;
    let pid = child.id();
    let start = Instant::now();
    let deadline = Duration::from_secs(config.timeout_secs);
    loop {
        if let Some(status) = child.try_wait().context("cannot poll QEMU")? {
            return Ok((status.code(), false));
        }
        if start.elapsed() >= deadline {
            kill_group(&mut child, pid);
            let _ = child.wait();
            return Ok((None, true));
        }
        std::thread::sleep(POLL);
    }
}

#[cfg(unix)]
fn kill_group(child: &mut Child, pid: u32) {
    let killed = Command::new("kill")
        .arg("-KILL")
        .arg("--")
        .arg(format!("-{pid}"))
        .status()
        .is_ok_and(|status| status.success());
    if !killed {
        let _ = child.kill();
    }
}

#[cfg(not(unix))]
fn kill_group(child: &mut Child, _pid: u32) {
    let _ = child.kill();
}

fn run_tool(program: &str, args: &[String], cwd: &Path, context: &str) -> Result<()> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("{context}: cannot run `{program}`"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("no output");
    bail!("{context}: {line}");
}

fn read_prefix(path: &Path) -> Vec<u8> {
    let Ok(mut file) = fs::File::open(path) else {
        return Vec::new();
    };
    let mut buffer = Vec::new();
    let _ = file
        .by_ref()
        .take(SERIAL_LIMIT as u64)
        .read_to_end(&mut buffer);
    buffer
}

/// The last `init exited (N)` line in the serial log.
#[must_use]
pub fn parse_init_exit(text: &str) -> Option<i32> {
    text.lines().rev().find_map(|line| {
        let line = line.trim();
        let rest = line.strip_prefix("init exited (")?;
        rest.strip_suffix(')')?.trim().parse().ok()
    })
}

/// Turn one boot into a verdict.
#[must_use]
pub fn judge(
    text: &str,
    qemu_status: Option<i32>,
    timed_out: bool,
    expect_exit: i32,
    timeout_secs: u64,
) -> Observation {
    if timed_out {
        return Observation {
            verdict: Verdict::Fail,
            reason: format!("timeout after {timeout_secs}s"),
        };
    }
    if let Some(code) = parse_init_exit(text) {
        if code == expect_exit {
            return Observation {
                verdict: Verdict::Pass,
                reason: String::new(),
            };
        }
        return Observation {
            verdict: Verdict::Fail,
            reason: format!("behaviour:expected exit {expect_exit}, got {code}"),
        };
    }
    let detail = match qemu_status {
        Some(code) => format!("qemu exit {code}"),
        None => "qemu died without a status".to_string(),
    };
    Observation {
        verdict: Verdict::Fail,
        reason: format!("crash:no init exit ({detail})"),
    }
}

fn mkinitfs_args(initfs: &Path, program: &Path) -> Vec<String> {
    vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "raven-initfs".to_string(),
        "--features".to_string(),
        "std".to_string(),
        "--example".to_string(),
        "mkinitfs".to_string(),
        "--".to_string(),
        initfs.display().to_string(),
        format!("/init={}", program.display()),
    ]
}

fn image_args(kernel: &Path, initfs: &Path, image: &Path) -> Vec<String> {
    vec![
        "run".to_string(),
        "-q".to_string(),
        "-p".to_string(),
        "xtask".to_string(),
        "--".to_string(),
        "image".to_string(),
        "--arch".to_string(),
        "x86_64".to_string(),
        "--profile".to_string(),
        "base".to_string(),
        "--cmdline".to_string(),
        CMDLINE.to_string(),
        "--initfs".to_string(),
        initfs.display().to_string(),
        "--kernel".to_string(),
        kernel.display().to_string(),
        "--output".to_string(),
        image.display().to_string(),
    ]
}

fn qemu_args(image: &Path, code: &Path, vars: &Path) -> Vec<String> {
    [
        ("-machine", "q35".to_string()),
        ("-m", "512M".to_string()),
        ("-smp", "4".to_string()),
        ("-serial", "stdio".to_string()),
        ("-display", "none".to_string()),
        ("-monitor", "none".to_string()),
        ("-no-reboot", String::new()),
        (
            "-drive",
            format!("if=pflash,format=raw,readonly=on,file={}", code.display()),
        ),
        (
            "-drive",
            format!("if=pflash,format=raw,file={}", vars.display()),
        ),
        ("-drive", format!("format=raw,file={}", image.display())),
        (
            "-device",
            "isa-debug-exit,iobase=0xf4,iosize=0x04".to_string(),
        ),
    ]
    .into_iter()
    .flat_map(|(flag, value)| {
        if value.is_empty() {
            vec![flag.to_string()]
        } else {
            vec![flag.to_string(), value]
        }
    })
    .collect()
}

#[cfg(test)]
#[path = "qemu_ut.rs"]
mod tests;
