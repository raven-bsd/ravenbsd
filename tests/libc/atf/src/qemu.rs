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
use crate::atf_proto;
use crate::result::{judge as judge_result, judge_missing, judge_unparsable, parse_result};
use crate::runner::Observation;
use crate::{ProcessStatus, TpCase};

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

/// Guest service and wrapper binaries built by `tests/fixtures/g1/build-guest.sh`.
#[derive(Clone, Debug)]
pub struct Guest {
    bin: PathBuf,
}

impl Guest {
    /// The stable artifact directory used by the build script.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            bin: root.join("build/fixtures/g1/guest/bin"),
        }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.bin.join(name)
    }

    fn require(&self) -> Result<()> {
        for name in [
            "init", "logd", "consd", "initfsd", "ramfsd", "nulld", "zerod", "randd", "vfsd",
            "g1wrap",
        ] {
            let path = self.path(name);
            if !path.is_file() {
                bail!(
                    "guest binary missing: {} (run sh tests/fixtures/g1/build-guest.sh)",
                    path.display()
                );
            }
        }
        Ok(())
    }
}

/// How the wrapper hands the image to the program.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuestMode {
    /// `spawn` with a pipe and result-file capture (the wrapper's own loader).
    Spawn,
    /// `execve` the program; the kernel's loader copies the file tail (argv/env only).
    Exec,
    /// `execve` the program, then a second service reads the result file.
    ExecRead,
}

/// What one service-mode boot observed.
#[derive(Clone, Debug)]
pub struct GuestObservation {
    /// The program's exit code from `G1-EXIT`.
    pub exit: Option<i32>,
    /// The captured stdout/stderr bytes.
    pub output: Vec<u8>,
    /// The captured result file, when the manifest asked for one.
    pub result: Option<Vec<u8>>,
    /// The wrapper's result-file error, when it could not read the file.
    pub result_error: Option<String>,
    /// QEMU's own exit status.
    pub qemu_status: Option<i32>,
    /// Whether the boot hit the deadline.
    pub timed_out: bool,
    /// The full serial log (used to read an ATF `-l` listing printed to the tty).
    pub serial: String,
}

/// The environment the host runner gives a target program.
#[must_use]
pub fn target_env() -> Vec<String> {
    [
        "PATH=/bin:/sbin",
        "HOME=/tmp",
        "TZ=UTC",
        "LANG=C.UTF-8",
        "LC_ALL=C.UTF-8",
        "TMPDIR=/tmp",
    ]
    .iter()
    .map(|value| (*value).to_string())
    .collect()
}

/// One program invocation in the guest service.
#[derive(Clone, Copy, Debug)]
pub struct GuestJob<'a> {
    /// The target executable.
    pub program: &'a Path,
    /// How the wrapper hands the image to the program.
    pub mode: GuestMode,
    /// Arguments after `argv[0]`.
    pub args: &'a [String],
    /// `KEY=VALUE` environment entries.
    pub env: &'a [String],
    /// An optional result path the reader service prints.
    pub result: Option<&'a str>,
    /// Per-run work directory.
    pub workdir: &'a Path,
    /// Keep the initfs and boot image.
    pub keep: bool,
}

/// Run one program through the guest service and wrapper with argv/env.
pub fn run_guest(
    config: &QemuConfig,
    guest: &Guest,
    job: &GuestJob<'_>,
) -> Result<GuestObservation> {
    let GuestJob {
        program,
        mode,
        args,
        env,
        result,
        workdir,
        keep,
    } = *job;
    guest.require()?;
    fs::create_dir_all(workdir).with_context(|| format!("cannot create {}", workdir.display()))?;
    let initfs = workdir.join("initfs.img");
    let image = workdir.join("boot.img");
    let vars = workdir.join("OVMF_VARS.fd");
    let serial_path = workdir.join("serial.log");
    let error_path = workdir.join("qemu.err");
    for stale in [&initfs, &image, &vars, &serial_path, &error_path] {
        let _ = fs::remove_file(stale);
    }
    let image_program = strip_program(program, workdir)?;
    let manifest = workdir.join("manifest");
    let service = workdir.join("g1.toml");
    let reader = workdir.join("g1-read.toml");
    let svcd = workdir.join("svcd.conf");
    let (service_text, reader_text, svcd_text) = guest_service(mode);
    fs::write(&manifest, guest_manifest(mode, args, env, result))
        .with_context(|| format!("cannot write {}", manifest.display()))?;
    fs::write(&service, service_text)
        .with_context(|| format!("cannot write {}", service.display()))?;
    let reader_path = match reader_text {
        Some(text) => {
            fs::write(&reader, text)
                .with_context(|| format!("cannot write {}", reader.display()))?;
            Some(reader.as_path())
        }
        None => None,
    };
    fs::write(&svcd, svcd_text).with_context(|| format!("cannot write {}", svcd.display()))?;
    let specs = guest_initfs_specs(
        config,
        guest,
        &image_program,
        &manifest,
        &service,
        reader_path,
        &svcd,
    );
    run_tool(
        "cargo",
        &mkinitfs_with(&initfs, &specs),
        &config.root,
        "cannot build the guest initfs",
    )?;
    make_image(config, &initfs, &image)?;
    copy_firmware_vars(&vars)?;
    let (status, timed_out) = boot(config, &image, &vars, &serial_path, &error_path)?;
    let serial = read_prefix(&serial_path);
    let text = String::from_utf8_lossy(&serial);
    let observation = GuestObservation {
        exit: marker_int(&text, "G1-EXIT ").or_else(|| parse_init_exit(&text)),
        output: marker_block(&text, "G1-OUTPUT-BEGIN", "G1-OUTPUT-END").unwrap_or_default(),
        result: marker_block(&text, "G1-RESULT-BEGIN", "G1-RESULT-END"),
        result_error: marker_string(&text, "G1-RESULT-ERROR "),
        qemu_status: status,
        timed_out,
        serial: text.into_owned(),
    };
    if !keep {
        for transient in [&initfs, &image, &vars] {
            let _ = fs::remove_file(transient);
        }
    }
    Ok(observation)
}

/// Judge an exit-status program from a service-mode observation.
#[must_use]
pub fn judge_guest(
    observation: &GuestObservation,
    expect_exit: i32,
    timeout_secs: u64,
) -> Observation {
    if observation.timed_out {
        return Observation {
            verdict: Verdict::Fail,
            reason: format!("timeout after {timeout_secs}s"),
        };
    }
    match observation.exit {
        Some(code) if code == expect_exit => Observation {
            verdict: Verdict::Pass,
            reason: String::new(),
        },
        Some(code) => Observation {
            verdict: Verdict::Fail,
            reason: format!("behaviour:expected exit {expect_exit}, got {code}"),
        },
        None => Observation {
            verdict: Verdict::Fail,
            reason: format!(
                "crash:no G1-EXIT ({})",
                match observation.qemu_status {
                    Some(code) => format!("qemu exit {code}"),
                    None => "qemu died without a status".to_string(),
                }
            ),
        },
    }
}

/// List an ATF program's bodies through the guest `-l` protocol.
pub fn list_atf(
    config: &QemuConfig,
    guest: &Guest,
    program: &Path,
    workdir: &Path,
    keep: bool,
) -> Result<Vec<TpCase>> {
    let args = vec!["-l".to_string()];
    let env = target_env();
    let observation = run_guest(
        config,
        guest,
        &GuestJob {
            program,
            mode: GuestMode::Exec,
            args: &args,
            env: &env,
            result: None,
            workdir,
            keep,
        },
    )?;
    if observation.timed_out {
        bail!("ATF listing timed out after {}s", config.timeout_secs);
    }
    let Some(listing) = extract_atf_list(&observation.serial) else {
        bail!(
            "ATF listing not found in the serial log for {}",
            program.display()
        );
    };
    atf_proto::parse_list(&listing).map_err(|error| anyhow::anyhow!("ATF listing: {error}"))
}

/// Run one ATF body and judge it from the guest result file.
pub fn run_atf_case(
    config: &QemuConfig,
    guest: &Guest,
    program: &Path,
    case: &TpCase,
    workdir: &Path,
    keep: bool,
) -> Result<Observation> {
    let result_path = "/tmp/atf-result";
    let args = vec![
        "-r".to_string(),
        result_path.to_string(),
        "-s".to_string(),
        "/tests".to_string(),
        case.ident.clone(),
    ];
    let env = target_env();
    let observation = run_guest(
        config,
        guest,
        &GuestJob {
            program,
            mode: GuestMode::ExecRead,
            args: &args,
            env: &env,
            result: Some(result_path),
            workdir,
            keep,
        },
    )?;
    if observation.timed_out {
        return Ok(Observation {
            verdict: Verdict::Fail,
            reason: format!("timeout after {}s", config.timeout_secs),
        });
    }
    let status = parse_service_exit(&observation.serial)
        .map_or(ProcessStatus::Signaled(0), ProcessStatus::Exited);
    match observation.result {
        Some(bytes) => match parse_result(&String::from_utf8_lossy(&bytes)) {
            Ok(expect) => {
                let (verdict, reason) = judge_result(&expect, status, false, config.timeout_secs);
                Ok(Observation { verdict, reason })
            }
            Err(_) => {
                let (verdict, reason) = judge_unparsable(&bytes, status);
                Ok(Observation { verdict, reason })
            }
        },
        None => {
            let (verdict, reason) = judge_missing(status, false, config.timeout_secs);
            Ok(Observation { verdict, reason })
        }
    }
}

fn extract_atf_list(serial: &str) -> Option<String> {
    let start = serial.find("Content-Type: application/X-atf-tp")?;
    let rest = &serial[start..];
    let end = rest
        .find("process: child exited")
        .or_else(|| rest.find("process: /tests/program exited"))
        .or_else(|| rest.find("svcd: g1"))
        .unwrap_or(rest.len());
    Some(rest.get(..end)?.replace('\r', ""))
}

fn parse_service_exit(serial: &str) -> Option<i32> {
    serial.lines().rev().find_map(|line| {
        let line = line.trim();
        for prefix in [
            "process: child exited (",
            "process: /tests/program exited (",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                return rest.strip_suffix(')')?.trim().parse().ok();
            }
        }
        None
    })
}

fn guest_service(mode: GuestMode) -> (&'static str, Option<&'static str>, &'static str) {
    const RUN: &str = "name = \"g1\"\nexec = \"/sbin/g1wrap\"\nargs = [\"g1wrap\", \"run\"]\n\
needs = [\"scheme:file\", \"scheme:tty\"]\nreadiness = \"exit\"\nrestart = \"never\"\n\
max_restarts = 0\nrequired = false\n";
    match mode {
        GuestMode::ExecRead => (
            RUN,
            Some(
                "name = \"g1-read\"\nexec = \"/sbin/g1wrap\"\nargs = [\"g1wrap\", \"read\"]\n\
needs = [\"g1\"]\nreadiness = \"exit\"\nrestart = \"never\"\nmax_restarts = 0\n\
required = false\n",
            ),
            "exit_when_exits = \"g1-read\"\n",
        ),
        _ => (RUN, None, "exit_when_exits = \"g1\"\n"),
    }
}

fn guest_manifest(
    mode: GuestMode,
    args: &[String],
    env: &[String],
    result: Option<&str>,
) -> String {
    let mode = if matches!(mode, GuestMode::Exec | GuestMode::ExecRead) {
        "exec"
    } else {
        "spawn"
    };
    let mut text = format!("mode={mode}\nprogram=/tests/program\narg=/tests/program\n");
    for arg in args {
        text.push_str("arg=");
        text.push_str(arg);
        text.push('\n');
    }
    for item in env {
        text.push_str("env=");
        text.push_str(item);
        text.push('\n');
    }
    if let Some(path) = result {
        text.push_str("result=");
        text.push_str(path);
        text.push('\n');
    }
    text
}

/// The kernel `PROC_EXEC` image limit is 1 MiB (`userland/rt/src/exec.rs`); the target link keeps
/// debug info, so strip a copy before packing. A missing `strip` leaves the original for the
/// loader to reject with its real reason.
fn strip_program(program: &Path, workdir: &Path) -> Result<PathBuf> {
    let stripped = workdir.join("program");
    fs::copy(program, &stripped).with_context(|| {
        format!(
            "cannot copy {} to {}",
            program.display(),
            stripped.display()
        )
    })?;
    let stripped_ok = Command::new("strip")
        .arg("--strip-all")
        .arg(&stripped)
        .status()
        .is_ok_and(|status| status.success());
    Ok(if stripped_ok {
        stripped
    } else {
        program.to_path_buf()
    })
}

fn guest_initfs_specs(
    config: &QemuConfig,
    guest: &Guest,
    program: &Path,
    manifest: &Path,
    service: &Path,
    reader: Option<&Path>,
    svcd: &Path,
) -> Vec<String> {
    let mut specs = vec![
        format!("/init={}", guest.path("init").display()),
        format!("/sbin/g1wrap={}", guest.path("g1wrap").display()),
        format!("/tests/program={}:0755", program.display()),
        format!("/g1/manifest={}", manifest.display()),
        format!("/etc/services.d/g1.toml={}", service.display()),
        format!("/etc/svcd.conf={}", svcd.display()),
    ];
    if let Some(reader) = reader {
        specs.push(format!("/etc/services.d/g1-read.toml={}", reader.display()));
    }
    for name in [
        "logd", "consd", "initfsd", "ramfsd", "nulld", "zerod", "randd", "vfsd",
    ] {
        specs.push(format!("/sbin/{name}={}", guest.path(name).display()));
        specs.push(format!(
            "/etc/services.d/{name}.toml={}",
            config
                .root
                .join(format!("etc/services.d/{name}.toml"))
                .display()
        ));
    }
    specs
}

fn mkinitfs_with(initfs: &Path, specs: &[String]) -> Vec<String> {
    let mut args = vec![
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
    ];
    args.extend(specs.iter().cloned());
    args
}

fn marker_int(text: &str, prefix: &str) -> Option<i32> {
    text.lines()
        .rev()
        .find_map(|line| line.trim().strip_prefix(prefix)?.trim().parse().ok())
}

fn marker_string(text: &str, prefix: &str) -> Option<String> {
    text.lines().rev().find_map(|line| {
        line.trim()
            .strip_prefix(prefix)
            .map(|value| value.trim().to_string())
    })
}

fn marker_block(text: &str, begin: &str, end: &str) -> Option<Vec<u8>> {
    let start = text.rfind(begin)? + begin.len();
    let rest = text.get(start..)?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))
        .unwrap_or(rest);
    let stop = rest.find(end)?;
    let mut bytes = rest.get(..stop)?.as_bytes().to_vec();
    while bytes.ends_with(b"\n") || bytes.ends_with(b"\r") {
        bytes.pop();
    }
    Some(bytes)
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
    mkinitfs_with(initfs, &[format!("/init={}", program.display())])
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
