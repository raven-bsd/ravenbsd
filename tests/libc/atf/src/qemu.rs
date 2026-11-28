// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! QEMU execution of one built target program.
//!
//! The ravenbsd kernel runs `/init` as PID 1, prints `init exited (N)` when it exits
//! (`kernel/src/process/mod.rs`), and with `test-exit=1` maps N = 0 to QEMU status 33 and any
//! other N to 35. The runner therefore packs one exit-status program as `/init`, boots the base
//! image, and reads N from the serial log. The program's output stays beside the log for
//! diagnosis; the 320 MiB boot image is removed after the run.

use std::collections::BTreeMap;
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
/// Largest serial log kept; a batch of hundreds of ATF bodies can exceed a few megabytes.
const SERIAL_LIMIT: usize = 32 * 1024 * 1024;
/// The donor `nobody` uid/gid used for `require.user=unprivileged` ATF cases.
const UNPRIVILEGED_UID: u32 = 65534;
/// The ATF metadata value that requests the unprivileged identity.
const UNPRIVILEGED_USER: &str = "unprivileged";
/// The dedicated 1777 tmpfs mount the G1 wrapper creates for unprivileged ATF cases.
const ATF_ROOT: &str = "/tmp/atf-root";

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

/// One ATF case in a batched guest boot.
#[derive(Clone, Debug)]
pub struct AtfBatchCase {
    /// The built target executable.
    pub program_path: PathBuf,
    /// The suite program name (used for the image name and diagnostics).
    pub program_name: String,
    /// The ATF case ident.
    pub ident: String,
    /// The ATF `require.user` metadata value, when the case requests an identity.
    pub user: Option<String>,
}

/// One batched ATF result, in the input case order.
#[derive(Clone, Debug)]
pub struct AtfBatchResult {
    /// The test process exit status parsed from the serial log.
    pub exit: Option<i32>,
    /// The ATF result file bytes.
    pub result: Option<Vec<u8>>,
    /// The reason the reader did not find a result.
    pub missing: Option<String>,
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
    let mut specs = guest_initfs_specs(
        config,
        guest,
        &image_program,
        &manifest,
        &service,
        reader_path,
        &svcd,
    )?;
    specs.extend(guest_credentials(workdir)?);
    specs.extend(guest_mounts(workdir)?);
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

/// One pjdfstest TAP program run in the guest.
pub struct TapJob<'a> {
    /// Absolute path to the vendored `.t` script.
    pub script: &'a Path,
    /// Program name (`open/00`), used for the staged path.
    pub program: &'a str,
    /// Vendored `tests` tree holding `conf` and `misc.sh`.
    pub tree: &'a Path,
    /// The built pjdfstest helper.
    pub helper: &'a Path,
    /// The built target wrapper (sets the work directory).
    pub wrapper: &'a Path,
    /// Per-run host work directory.
    pub workdir: &'a Path,
    /// Keep the initfs and boot image.
    pub keep: bool,
}

/// The guest observation of one TAP program.
pub struct TapObservation {
    /// The program's exit status from `G1-EXIT`.
    pub exit: Option<i32>,
    /// Captured stdout/stderr.
    pub output: String,
    /// Whether the boot hit the deadline.
    pub timed_out: bool,
}

/// Tools the pjdfstest shell bodies call. Each entry is `(guest name, host path)`.
fn tap_tools(root: &Path) -> Vec<(String, PathBuf)> {
    let mut tools = Vec::new();
    for name in [
        "basename", "cat", "chmod", "chown", "cp", "cut", "date", "df", "dirname", "echo", "env",
        "false", "head", "id", "ln", "ls", "mkdir", "mv", "pwd", "rm", "rmdir", "sleep", "sort",
        "stat", "tail", "touch", "tr", "true", "wc",
    ] {
        tools.push((
            name.to_string(),
            root.join("build/x86_64/uutils").join(name),
        ));
    }
    for (name, package) in [
        ("grep", "raven-grep"),
        ("awk", "raven-awk"),
        ("uname", "raven-uname"),
    ] {
        tools.push((
            name.to_string(),
            root.join("build/x86_64/user").join(package),
        ));
    }
    tools
}

/// Run one pjdfstest TAP program through the guest shell and capture its TAP output.
pub fn run_tap(config: &QemuConfig, guest: &Guest, job: &TapJob<'_>) -> Result<TapObservation> {
    let TapJob {
        script,
        program,
        tree,
        helper,
        wrapper,
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

    let stage = workdir.join("stage");
    fs::create_dir_all(stage.join("tests")).with_context(|| "cannot create tap stage")?;
    let staged_helper = stage.join("pjdfstest");
    strip_to(helper, &staged_helper)?;
    let staged_wrapper = stage.join("atf-wrapper");
    strip_to(wrapper, &staged_wrapper)?;

    let wrap = workdir.join("manifest");
    fs::write(
        &wrap,
        format!(
            "mode=spawn\nprogram=/tests/atf-wrapper\narg=/tests/atf-wrapper\n\
arg=/tmp/pjdfstest\narg=/bin/sh\narg=-c\n\
arg=cd /tmp/pjdfstest && exec /bin/sh /tests/pjdfstest/tests/{program}.t\n\
env=PATH=/bin:/usr/bin\nenv=HOME=/tmp\nenv=TMPDIR=/tmp\nenv=TZ=UTC\n\
env=LANG=C.UTF-8\nenv=LC_ALL=C.UTF-8\n"
        ),
    )
    .with_context(|| format!("cannot write {}", wrap.display()))?;
    let service = workdir.join("g1.toml");
    let svcd = workdir.join("svcd.conf");
    let (service_text, _reader_text, svcd_text) = guest_service(GuestMode::Exec);
    fs::write(&service, service_text)
        .with_context(|| format!("cannot write {}", service.display()))?;
    fs::write(&svcd, svcd_text).with_context(|| format!("cannot write {}", svcd.display()))?;

    let mut specs = service_specs(config, guest, workdir)?;
    specs.extend(guest_credentials(workdir)?);
    specs.extend(guest_mounts(workdir)?);
    specs.push(format!(
        "/tests/atf-wrapper={}:0755",
        staged_wrapper.display()
    ));
    specs.push(format!(
        "/tests/pjdfstest/pjdfstest={}:0755",
        staged_helper.display()
    ));
    specs.push(format!("/g1/manifest={}", wrap.display()));
    specs.push(format!("/etc/services.d/g1.toml={}", service.display()));
    specs.push(format!("/etc/svcd.conf={}", svcd.display()));
    specs.push(format!(
        "/bin/sh={}",
        config.root.join("build/x86_64/brush").display()
    ));
    for (name, path) in tap_tools(&config.root) {
        if !path.is_file() {
            bail!(
                "TAP tool missing: {} (build it with cargo xtask build user/uutils)",
                path.display()
            );
        }
        specs.push(format!("/bin/{name}={}", path.display()));
    }
    for name in ["conf", "misc.sh"] {
        // Raven is not in the upstream conf's OS list; use the Raven filesystem configuration
        // when it is present, but keep the upstream name the scripts source.
        let source = if name == "conf" && tree.join("conf.raven").is_file() {
            tree.join("conf.raven")
        } else if name == "misc.sh" && tree.join("misc.raven.sh").is_file() {
            tree.join("misc.raven.sh")
        } else {
            tree.join(name)
        };
        let destination = stage.join("tests").join(name);
        fs::copy(&source, &destination)
            .with_context(|| format!("cannot stage {}", source.display()))?;
        specs.push(format!(
            "/tests/pjdfstest/tests/{name}={}",
            destination.display()
        ));
    }
    let destination = stage.join("tests").join(format!("{program}.t"));
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    fs::copy(script, &destination).with_context(|| format!("cannot stage {}", script.display()))?;
    specs.push(format!(
        "/tests/pjdfstest/tests/{program}.t={}",
        destination.display()
    ));

    run_tool(
        "cargo",
        &mkinitfs_with(&initfs, &specs),
        &config.root,
        "cannot build the TAP guest initfs",
    )?;
    make_image(config, &initfs, &image)?;
    copy_firmware_vars(&vars)?;
    let (_, timed_out) = boot(config, &image, &vars, &serial_path, &error_path)?;
    let serial = read_prefix(&serial_path);
    let text = String::from_utf8_lossy(&serial).into_owned();
    let output = marker_block(&text, "G1-OUTPUT-BEGIN", "G1-OUTPUT-END")
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    if !keep {
        for transient in [&initfs, &image, &vars] {
            let _ = fs::remove_file(transient);
        }
    }
    Ok(TapObservation {
        exit: marker_int(&text, "G1-EXIT "),
        output,
        timed_out,
    })
}

/// Run a batch of ATF cases in one boot.
///
/// Each case is a `g1-run-<i>` service that execs the test with `-r /tmp/result-<i>`, followed
/// by a `g1-read-<i>` service that polls the result file and prints an indexed marker. The
/// services form a chain, so failures do not stop the next case.
pub fn run_atf_batch(
    config: &QemuConfig,
    guest: &Guest,
    cases: &[AtfBatchCase],
    wrapper: Option<&Path>,
    workdir: &Path,
    keep: bool,
) -> Result<Vec<AtfBatchResult>> {
    if cases.is_empty() {
        return Ok(Vec::new());
    }
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

    let programs_dir = workdir.join("programs");
    fs::create_dir_all(&programs_dir)
        .with_context(|| format!("cannot create {}", programs_dir.display()))?;
    let mut image_paths: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut used: BTreeMap<String, usize> = BTreeMap::new();
    for case in cases {
        if image_paths.contains_key(&case.program_path) {
            continue;
        }
        let mut base: String = case
            .program_name
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        if base.is_empty() {
            base.push_str("program");
        }
        let count = used.entry(base.clone()).or_insert(0);
        let unique = if *count == 0 {
            base.clone()
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        let destination = programs_dir.join(&unique);
        strip_to(&case.program_path, &destination)?;
        image_paths.insert(case.program_path.clone(), format!("/tests/{unique}"));
    }

    let mut specs = service_specs(config, guest, workdir)?;
    specs.extend(guest_credentials(workdir)?);
    specs.extend(guest_mounts(workdir)?);
    if let Some(wrapper) = wrapper {
        let destination = programs_dir.join("atf-wrapper");
        strip_to(wrapper, &destination)?;
        specs.push(format!("/tests/atf-wrapper={}:0755", destination.display()));
    }
    for (index, case) in cases.iter().enumerate() {
        let image_path = image_paths
            .get(&case.program_path)
            .expect("program image was registered");
        let result = if case.user.as_deref() == Some(UNPRIVILEGED_USER) {
            format!("{ATF_ROOT}/result-{index}")
        } else {
            format!("/tmp/result-{index}")
        };
        let manifest = workdir.join(format!("manifest-{index}"));
        fs::write(
            &manifest,
            batch_manifest(
                &case.ident,
                image_path,
                &result,
                wrapper.is_some(),
                case.user.as_deref(),
            ),
        )
        .with_context(|| format!("cannot write {}", manifest.display()))?;
        let run = workdir.join(format!("g1-run-{index}.toml"));
        let read = workdir.join(format!("g1-read-{index}.toml"));
        let needs = if index == 0 {
            "[\"scheme:file\", \"scheme:tty\"]".to_string()
        } else {
            format!("[\"g1-read-{}\"]", index - 1)
        };
        // The reader must not depend on the run service: a failing test exits nonzero, and a
        // dependency on a failed service would stall the whole chain. Both services wait for the
        // previous reader and then run concurrently; the reader polls the result file.
        fs::write(
            &run,
            format!(
                "name = \"g1-run-{index}\"\nexec = \"/sbin/g1wrap\"\n\
args = [\"g1wrap\", \"run\", \"{index}\"]\nneeds = {needs}\nreadiness = \"none\"\n\
restart = \"never\"\nmax_restarts = 0\nrequired = false\n"
            ),
        )
        .with_context(|| format!("cannot write {}", run.display()))?;
        fs::write(
            &read,
            format!(
                "name = \"g1-read-{index}\"\nexec = \"/sbin/g1wrap\"\n\
args = [\"g1wrap\", \"read\", \"{index}\"]\nneeds = {needs}\n\
namespace = [\"svc\", \"file\"]\n\
readiness = \"exit\"\nrestart = \"never\"\nmax_restarts = 0\nrequired = false\n"
            ),
        )
        .with_context(|| format!("cannot write {}", read.display()))?;
        specs.push(format!("/g1/manifest-{index}={}", manifest.display()));
        specs.push(format!(
            "/etc/services.d/g1-run-{index}.toml={}",
            run.display()
        ));
        specs.push(format!(
            "/etc/services.d/g1-read-{index}.toml={}",
            read.display()
        ));
    }
    let svcd = workdir.join("svcd.conf");
    fs::write(
        &svcd,
        format!("exit_when_exits = \"g1-read-{}\"\n", cases.len() - 1),
    )
    .with_context(|| format!("cannot write {}", svcd.display()))?;
    specs.push(format!("/etc/svcd.conf={}", svcd.display()));
    for image_path in image_paths.values() {
        let name = image_path.strip_prefix("/tests/").unwrap_or(image_path);
        specs.push(format!(
            "/tests/{name}={}:0755",
            programs_dir.join(name).display()
        ));
    }

    run_tool(
        "cargo",
        &mkinitfs_with(&initfs, &specs),
        &config.root,
        "cannot build the batch initfs",
    )?;
    make_image(config, &initfs, &image)?;
    copy_firmware_vars(&vars)?;
    let mut batch = config.clone();
    batch.timeout_secs = batch.timeout_secs.max(180);
    let (_, timed_out) = boot(&batch, &image, &vars, &serial_path, &error_path)?;
    let serial = read_prefix(&serial_path);
    let text = String::from_utf8_lossy(&serial);
    let mut results = parse_batch(&text, cases.len());
    if timed_out {
        for result in &mut results {
            if result.result.is_none() && result.missing.is_none() {
                result.missing = Some("batch timeout".to_string());
            }
        }
    }
    if !keep {
        for transient in [&initfs, &image, &vars] {
            let _ = fs::remove_file(transient);
        }
    }
    Ok(results)
}

fn service_specs(config: &QemuConfig, guest: &Guest, workdir: &Path) -> Result<Vec<String>> {
    let mut specs = vec![
        format!("/init={}", guest.path("init").display()),
        format!("/sbin/g1wrap={}", guest.path("g1wrap").display()),
    ];
    for name in [
        "logd", "consd", "initfsd", "ramfsd", "nulld", "zerod", "randd",
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
    specs.push(format!("/sbin/vfsd={}", guest.path("vfsd").display()));
    specs.extend(guest_root_services(workdir)?);
    Ok(specs)
}

/// Guest services that carry the donor `/root` skeleton.
///
/// `initfsd` reports every synthesized directory as 0755, so the donor 0700 `/root` mode cannot
/// come from the read-only initfs. A second ramfs instance is started with `--root-mode 700`;
/// `vfsd` must be able to resolve that scheme, so its generated manifest depends on `rootfsd` and
/// the start plan registers `rootfs` before `vfsd` takes its namespace snapshot. The G1 wrapper
/// mounts it at `/root` and writes `.profile` before a test runs. Unprivileged ATF cases get a
/// root-created 1777 work root under the mounted `/tmp`.
fn guest_root_services(workdir: &Path) -> Result<Vec<String>> {
    let directory = workdir.join("etc");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let rootfsd = directory.join("services-rootfsd.toml");
    fs::write(
        &rootfsd,
        "name = \"rootfsd\"\nexec = \"/sbin/ramfsd\"\n\
args = [\"ramfsd\", \"--scheme\", \"rootfs\", \"--root-mode\", \"700\"]\n\
needs = [\"logd\"]\nprovides = [\"scheme:rootfs\"]\n\
readiness = \"scheme\"\nready_timeout_ms = 60_000\nrestart = \"on-failure\"\n",
    )
    .with_context(|| format!("cannot write {}", rootfsd.display()))?;
    let vfsd = directory.join("services-vfsd.toml");
    fs::write(
        &vfsd,
        "name = \"vfsd\"\nexec = \"/sbin/vfsd\"\n\
needs = [\"logd\", \"initfsd\", \"ramfsd\", \"rootfsd\", \"nulld\", \"zerod\", \"randd\", \"scheme:pty\"]\n\
provides = [\"scheme:file\"]\nreadiness = \"scheme\"\n\
ready_timeout_ms = 60_000\nrestart = \"on-failure\"\n",
    )
    .with_context(|| format!("cannot write {}", vfsd.display()))?;
    Ok(vec![
        format!("/etc/services.d/rootfsd.toml={}", rootfsd.display()),
        format!("/etc/services.d/vfsd.toml={}", vfsd.display()),
    ])
}

fn strip_to(program: &Path, destination: &Path) -> Result<()> {
    fs::copy(program, destination).with_context(|| {
        format!(
            "cannot copy {} to {}",
            program.display(),
            destination.display()
        )
    })?;
    let _ = Command::new("strip")
        .arg("--strip-all")
        .arg(destination)
        .status();
    // `raven-spawn` maps the image file tail and requires it to end on a page boundary; the
    // kernel `PROC_EXEC` path does not. Pad the staged copy so a `posix_spawn` wrapper can start
    // the same program from the initfs. Trailing zeros are outside every ELF segment.
    let length = fs::metadata(destination)
        .with_context(|| format!("cannot stat {}", destination.display()))?
        .len();
    let padded = length.saturating_add(4095) & !4095;
    if padded > length {
        let file = fs::OpenOptions::new()
            .write(true)
            .open(destination)
            .with_context(|| format!("cannot open {}", destination.display()))?;
        file.set_len(padded)
            .with_context(|| format!("cannot pad {}", destination.display()))?;
    }
    Ok(())
}

fn batch_manifest(
    ident: &str,
    image_path: &str,
    result: &str,
    wrapper: bool,
    user: Option<&str>,
) -> String {
    let (program, prefix) = if wrapper {
        (
            "/tests/atf-wrapper",
            format!("arg=/tmp/atf-work\narg={image_path}\n"),
        )
    } else {
        (image_path, format!("arg={image_path}\n"))
    };
    let mut text = format!(
        "mode=exec\nprogram={program}\narg={program}\n{prefix}arg=-r\narg={result}\n\
arg=-s\narg=/tests\narg={ident}\n"
    );
    text.push_str(&user_manifest_line(user));
    for item in target_env() {
        text.push_str("env=");
        text.push_str(&item);
        text.push('\n');
    }
    text.push_str("result=");
    text.push_str(result);
    text.push('\n');
    text
}

/// Render the guest credential request for an ATF case that asks for a non-root identity.
fn user_manifest_line(user: Option<&str>) -> String {
    if user == Some(UNPRIVILEGED_USER) {
        format!("user={UNPRIVILEGED_UID}\n")
    } else {
        String::new()
    }
}

fn parse_batch(text: &str, count: usize) -> Vec<AtfBatchResult> {
    (0..count)
        .map(|index| {
            let run_marker = format!("G1-RUN {index}");
            let run_position = text.find(&run_marker);
            let exit = run_position.and_then(|position| {
                let rest = text.get(position..)?;
                let marker = "process: child exited (";
                let start = rest.find(marker)? + marker.len();
                let end = rest.get(start..)?.find(')')?;
                rest.get(start..start + end)?.trim().parse().ok()
            });
            let begin = format!("G1-RESULT-BEGIN {index}");
            let end = format!("G1-RESULT-END {index}");
            let result = text.find(&begin).and_then(|position| {
                let rest = text.get(position + begin.len()..)?;
                let rest = rest
                    .strip_prefix("\r\n")
                    .or_else(|| rest.strip_prefix('\n'))
                    .unwrap_or(rest);
                let stop = rest.find(&end)?;
                let mut bytes = rest.get(..stop)?.as_bytes().to_vec();
                while bytes.ends_with(b"\n") || bytes.ends_with(b"\r") {
                    bytes.pop();
                }
                Some(bytes)
            });
            let missing_marker = format!("G1-RESULT-MISSING {index}");
            let missing = text.find(&missing_marker).map(|position| {
                text.get(position + missing_marker.len()..)
                    .unwrap_or("")
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            });
            AtfBatchResult {
                exit,
                result,
                missing,
            }
        })
        .collect()
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

/// Stage the account files a login session provides. Donor bodies such as `t_dup` open
/// `/etc/passwd`; `initgroups(3)` in the wrapper reads `/etc/group`. The G1 initfs is a service
/// image with no account database, so the runner supplies the same minimal files the S179
/// credential image uses.
fn guest_credentials(workdir: &Path) -> Result<Vec<String>> {
    let directory = workdir.join("etc");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let passwd = directory.join("passwd");
    let group = directory.join("group");
    fs::write(
        &passwd,
        "root:x:0:0:root:/root:/bin/sh\n\
         nobody:x:65534:65534:nobody:/nonexistent:/nonexistent\n\
         raven:x:1000:1000:raven:/home/raven:/bin/sh\n",
    )
    .with_context(|| format!("cannot write {}", passwd.display()))?;
    fs::write(
        &group,
        "wheel:*:0:root\n\
         nobody:*:65534:\n\
         raven:*:1000:raven\n",
    )
    .with_context(|| format!("cannot write {}", group.display()))?;
    Ok(vec![
        format!("/etc/passwd={}:0644", passwd.display()),
        format!("/etc/group={}:0644", group.display()),
    ])
}

/// Stage the G1 mount table.
///
/// The product default mounts `/dev/null` on `null:`, but `vfsd` resolves the parent `/dev` in
/// the read-only initfs before it looks up the final component. The initfs has no `/dev`, so the
/// open fails with `ENOENT` before the null mount is reached. Mounting a ramfs at `/dev` makes
/// the parent a real directory; `/dev/null` itself stays the `null:` device mount and is never a
/// regular file. This is a test-only namespace file inside the ATF/G1 image, not a product mount
/// edit.
fn guest_mounts(workdir: &Path) -> Result<Vec<String>> {
    let directory = workdir.join("etc");
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let mounts = directory.join("mounts");
    fs::write(
        &mounts,
        "/ initfs:\n\
         /tmp ramfs:\n\
         /dev ramfs:\n\
         /dev/null null:\n\
         /dev/zero zero:\n\
         /dev/random rand:\n\
         /dev/urandom rand:\n",
    )
    .with_context(|| format!("cannot write {}", mounts.display()))?;
    Ok(vec![format!("/etc/mounts={}:0644", mounts.display())])
}

fn guest_initfs_specs(
    config: &QemuConfig,
    guest: &Guest,
    program: &Path,
    manifest: &Path,
    service: &Path,
    reader: Option<&Path>,
    svcd: &Path,
) -> Result<Vec<String>> {
    let workdir = manifest.parent().unwrap_or_else(|| Path::new("."));
    let mut specs = service_specs(config, guest, workdir)?;
    specs.push(format!("/tests/program={}:0755", program.display()));
    specs.push(format!("/g1/manifest={}", manifest.display()));
    specs.push(format!("/etc/services.d/g1.toml={}", service.display()));
    if let Some(reader) = reader {
        specs.push(format!("/etc/services.d/g1-read.toml={}", reader.display()));
    }
    specs.push(format!("/etc/svcd.conf={}", svcd.display()));
    Ok(specs)
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
    // Refuse a cached kernel from another commit, a dirty tree or the debug profile. The G1
    // runner does not rebuild the artifact, so booting a stale one would test the wrong ABI.
    crate::freshness::verify_kernel(&config.root, &config.kernel)?;
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
