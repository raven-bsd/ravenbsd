// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target glue: initfs, manifests, spawning, waves and the process-wide core.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::clock;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_proto_svc::State;
use raven_rt::spawn::{RtSys, SpawnError, SpawnOptions, Sys};
use raven_rt::sync::Mutex;
use raven_scheme::Server;
use raven_svc_manifest::{
    GraphError, Manifest, Overrides, ParseError, ParseErrorKind, UnknownService, apply_overrides,
    parse_overrides,
};

use crate::config::{self, Config};
use crate::state::{MergeError, Notice, StartRequest, Supervisor};

/// The mapped initfs, set once at boot.
static INITFS: Mutex<Option<Initfs<'static>>> = Mutex::new(None);
/// The process-wide supervisor state.
static CORE: Mutex<Option<Arc<Mutex<Core>>>> = Mutex::new(None);
/// The registered `svc:` server, moved into its thread.
static SERVER: Mutex<Option<Server>> = Mutex::new(None);
/// Fallback clock counter when `CLOCK_MONOTONIC` is unavailable.
static FALLBACK_MS: AtomicU64 = AtomicU64::new(0);

/// State shared by the boot thread, workers and the `svc:` server.
pub(crate) struct Core {
    pub(crate) supervisor: Supervisor,
    pub(crate) config: Config,
    pub(crate) exit_code: Option<i32>,
    pub(crate) pending_starts: Vec<StartRequest>,
    pub(crate) pending_terminations: Vec<Handle>,
    pub(crate) all_up_printed: bool,
    /// True when the service was declared by the root filesystem and should run from `/root`.
    pub(crate) root_exec: Vec<bool>,
}

/// Map the initfs, parse manifests and run the service manager.
pub fn run() -> i32 {
    let image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(error) => {
            raven_rt::println!("svcd: initfs: {}", error.description());
            return 1;
        }
    };
    // The base/dev/tiny profiles ship no manifests and take the legacy banner path, which never
    // reads file data. The kernel has already fully validated this same image, so a cheap name
    // table scan can skip the userspace CRC pass that dominates a debug build's boot time. Any
    // image the scanner cannot read falls through to the strict parser below.
    if raven_rt::initfs_has_manifest(image) == Some(false) {
        return crate::legacy::run();
    }
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(error) => {
            raven_rt::println!("svcd: initfs: {:?}", error);
            return 1;
        }
    };
    if INITFS.lock().is_some() {
        raven_rt::println!("svcd: initfs: already mapped");
        return 1;
    }
    *INITFS.lock() = Some(initfs);

    let mut manifest_files = 0usize;
    let mut services = Vec::new();
    for entry in initfs.entries() {
        if !is_manifest_path(entry.path) {
            continue;
        }
        manifest_files += 1;
        let Ok(text) = core::str::from_utf8(entry.data) else {
            raven_rt::println!("svcd: {}: not UTF-8", entry.path);
            continue;
        };
        match Manifest::parse(text) {
            Ok(manifest) => services.push(manifest),
            Err(error) => {
                raven_rt::println!("svcd: {}: {}", entry.path, describe_parse_error(&error))
            }
        }
    }
    if manifest_files == 0 {
        return crate::legacy::run();
    }
    if services.is_empty() {
        raven_rt::println!("svcd: no valid service manifests");
        return 1;
    }

    if let Some(entry) = initfs.get("/etc/rc.conf") {
        let Ok(text) = core::str::from_utf8(entry.data) else {
            raven_rt::println!("svcd: /etc/rc.conf: not UTF-8");
            return 1;
        };
        let overrides = match parse_overrides(text) {
            Ok(overrides) => overrides,
            Err(error) => {
                raven_rt::println!("svcd: /etc/rc.conf: {}", describe_parse_error(&error));
                return 1;
            }
        };
        if let Err(UnknownService { service }) = apply_overrides(&mut services, &overrides) {
            raven_rt::println!("svcd: /etc/rc.conf: unknown service {}", service);
            return 1;
        }
    }

    let config = read_config(&initfs);
    let supervisor = match Supervisor::new(services) {
        Ok(supervisor) => supervisor,
        Err(error) => {
            raven_rt::println!("svcd: {}", describe_graph_error(&error));
            return 1;
        }
    };
    // A root-capable image has the blockd -> fsd-ufs -> vfsd chain in its initfs. Other profiles
    // keep the one-phase initfs plan unchanged.
    let root_capable = ["blockd", "fsd-ufs", "vfsd"].iter().all(|name| {
        supervisor
            .services()
            .iter()
            .any(|service| service.name == *name)
    });
    let root_exec = alloc::vec![false; supervisor.len()];
    let core = Arc::new(Mutex::new(Core {
        supervisor,
        config,
        exit_code: None,
        pending_starts: Vec::new(),
        pending_terminations: Vec::new(),
        all_up_printed: false,
        root_exec,
    }));
    if CORE.lock().is_some() {
        raven_rt::println!("svcd: core already initialised");
        return 1;
    }
    *CORE.lock() = Some(Arc::clone(&core));

    match Server::register("svc") {
        Ok(server) => {
            *SERVER.lock() = Some(server);
        }
        Err(error) => {
            raven_rt::println!("svcd: svc: {}", error.description());
            return 1;
        }
    }
    match raven_rt::thread::spawn(crate::svc::thread, 0, 96 * 1024) {
        Ok(_thread) => {}
        Err(_) => {
            raven_rt::println!("svcd: svc: cannot create the server thread");
            return 1;
        }
    }

    if root_capable {
        let selected = bootstrap_selection(core.lock().supervisor.services());
        {
            let guard = core.lock();
            raven_rt::println!(
                "svcd: bootstrap plan: {}",
                plan_text(&guard.supervisor, Some(&selected))
            );
        }
        if let Some(code) = run_waves(&core, Some(&selected), false, false) {
            return code;
        }
        match activate_root(&core) {
            Ok(()) => {}
            Err(reason) => {
                raven_rt::println!("svcd: root: {reason}; starting the initfs plan");
            }
        }
        // Root one-shot services read through vfsd; start them one at a time so the shared
        // file-serving path is not exercised by several new clients at once.
        if let Some(code) = run_waves(&core, None, true, true) {
            return code;
        }
    } else if let Some(code) = run_waves(&core, None, false, false) {
        return code;
    }

    core.lock().supervisor.set_boot_complete();
    loop {
        pump(&core);
        if let Some(code) = core.lock().exit_code {
            return code;
        }
        let (all_up_first, settled, settled_code, exit_when_settled) = {
            let mut guard = core.lock();
            let all_up = guard.supervisor.all_up();
            let first = all_up && !guard.all_up_printed;
            if first {
                guard.all_up_printed = true;
            }
            let settled = guard.supervisor.settled();
            (
                first,
                settled,
                guard.supervisor.settled_exit_code(),
                guard.config.exit_when_settled,
            )
        };
        if all_up_first {
            raven_rt::println!("svcd: all services up");
        }
        if exit_when_settled && settled {
            raven_rt::println!("svcd: settled");
            return settled_code;
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
}

/// Start the current plan, optionally restricted to a bootstrap selection and to services that
/// have not been started yet. Returns an exit code when a service asked svcd to exit.
fn run_waves(
    core: &Arc<Mutex<Core>>,
    selected: Option<&[bool]>,
    only_unstarted: bool,
    serial: bool,
) -> Option<i32> {
    let waves = { core.lock().supervisor.plan().waves.clone() };
    for wave in waves {
        let members: Vec<usize> = {
            let guard = core.lock();
            wave.iter()
                .copied()
                .filter(|index| {
                    selected.is_none_or(|selected| selected.get(*index).copied().unwrap_or(false))
                        && (!only_unstarted
                            || !guard
                                .supervisor
                                .runtime()
                                .get(*index)
                                .is_some_and(|runtime| runtime.started))
                })
                .collect()
        };
        if members.is_empty() {
            continue;
        }
        if serial {
            for index in members {
                if let Some(code) = start_and_wait(core, &[index]) {
                    return Some(code);
                }
            }
        } else if let Some(code) = start_and_wait(core, &members) {
            return Some(code);
        }
    }
    None
}

/// Start `members` and wait until each has been ready or reached a terminal state.
fn start_and_wait(core: &Arc<Mutex<Core>>, members: &[usize]) -> Option<i32> {
    for &index in members {
        let generation = { core.lock().supervisor.start(index, now_ms()) };
        if let Some(generation) = generation {
            spawn_worker(core, index, generation);
        }
    }
    loop {
        pump(core);
        if let Some(code) = core.lock().exit_code {
            return Some(code);
        }
        let done = {
            let guard = core.lock();
            members.iter().all(|&index| {
                guard.supervisor.runtime().get(index).is_none_or(|runtime| {
                    runtime.ever_ready
                        || matches!(
                            runtime.state,
                            State::Failed | State::GaveUp | State::Stopped
                        )
                })
            })
        };
        if done {
            return None;
        }
        let _ = raven_rt::nanosleep(5_000_000);
    }
}

/// Select the bootstrap services and every dependency they need by service name or scheme.
fn bootstrap_selection(services: &[Manifest]) -> Vec<bool> {
    const SEEDS: &[&str] = &[
        "logd", "initfsd", "ramfsd", "nulld", "zerod", "randd", "blockd", "fsd-ufs", "vfsd", "devd",
    ];
    let mut selected = alloc::vec![false; services.len()];
    for seed in SEEDS {
        if let Some(index) = services.iter().position(|service| service.name == *seed) {
            selected[index] = true;
        }
    }
    loop {
        let mut changed = false;
        for index in 0..services.len() {
            if !selected[index] {
                continue;
            }
            for need in &services[index].needs {
                let dependency = if need.starts_with("scheme:") {
                    services.iter().position(|service| {
                        service
                            .provides
                            .iter()
                            .any(|provide| provide.as_str() == need.as_str())
                    })
                } else {
                    services
                        .iter()
                        .position(|service| service.name.as_str() == need.as_str())
                };
                if let Some(dependency) = dependency
                    && !selected[dependency]
                {
                    selected[dependency] = true;
                    changed = true;
                }
            }
        }
        if !changed {
            return selected;
        }
    }
}

/// Render the plan as one space-separated line, optionally filtered to `selected`.
fn plan_text(supervisor: &Supervisor, selected: Option<&[bool]>) -> String {
    let mut text = String::new();
    for wave in &supervisor.plan().waves {
        for &index in wave {
            if let Some(selected) = selected
                && !selected.get(index).copied().unwrap_or(false)
            {
                continue;
            }
            let Some(service) = supervisor.services().get(index) else {
                continue;
            };
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&service.name);
        }
    }
    if text.is_empty() {
        text.push_str("(none)");
    }
    text
}

/// Mount the root filesystem at `/root`, read and merge its service set.
fn activate_root(core: &Arc<Mutex<Core>>) -> Result<(), String> {
    raven_rt::fs::mount("/root", "ufs:")
        .map_err(|error| alloc::format!("/root mount failed: {}", error.description()))?;
    raven_rt::println!("svcd: root mounted at /root");
    let services = read_root_services()?;
    let overrides = read_root_overrides()?;
    let root_count = services.len();
    let root_names: Vec<String> = services
        .iter()
        .map(|service| service.name.clone())
        .collect();
    let mut guard = core.lock();
    let added = guard
        .supervisor
        .merge(services, overrides.as_ref())
        .map_err(describe_merge_error)?;
    let total = guard.supervisor.len();
    guard.root_exec.resize(total, false);
    for name in &root_names {
        if let Some(index) = guard.supervisor.find(name) {
            guard.root_exec[index] = true;
        }
    }
    raven_rt::println!("svcd: root services={} added={}", root_count, added);
    raven_rt::println!("svcd: service plan: {}", plan_text(&guard.supervisor, None));
    Ok(())
}

fn read_root_services() -> Result<Vec<Manifest>, String> {
    let entries = raven_rt::fs::read_dir("file:/root/etc/services.d")
        .map_err(|error| alloc::format!("/root/etc/services.d: {}", error.description()))?;
    let mut names: Vec<String> = entries
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.ends_with(".toml"))
        .collect();
    names.sort();
    let mut services = Vec::new();
    for name in names {
        let path = alloc::format!("file:/root/etc/services.d/{name}");
        let data = raven_rt::fs::read_to_vec(&path)
            .map_err(|error| alloc::format!("{path}: {}", error.description()))?;
        let text = core::str::from_utf8(&data).map_err(|_| alloc::format!("{path}: not UTF-8"))?;
        match Manifest::parse(text) {
            Ok(manifest) => services.push(manifest),
            Err(error) => raven_rt::println!(
                "svcd: /root/etc/services.d/{name}: {}",
                describe_parse_error(&error)
            ),
        }
    }
    Ok(services)
}

fn read_root_overrides() -> Result<Option<Overrides>, String> {
    match raven_rt::fs::read_to_vec("file:/root/etc/rc.conf") {
        Ok(data) => {
            let text = core::str::from_utf8(&data)
                .map_err(|_| "/root/etc/rc.conf: not UTF-8".to_string())?;
            parse_overrides(text).map(Some).map_err(|error| {
                alloc::format!("/root/etc/rc.conf: {}", describe_parse_error(&error))
            })
        }
        Err(raven_abi::errno::Errno::ENOENT) => Ok(None),
        Err(error) => Err(alloc::format!("/root/etc/rc.conf: {}", error.description())),
    }
}

fn describe_merge_error(error: MergeError) -> String {
    match error {
        MergeError::UnknownService(service) => {
            alloc::format!("root rc.conf names unknown service `{service}`")
        }
        MergeError::Graph(error) => describe_graph_error(&error),
    }
}

/// True when `index` should start from the root filesystem.
pub(crate) fn service_root_exec(core: &Arc<Mutex<Core>>, index: usize) -> bool {
    core.lock().root_exec.get(index).copied().unwrap_or(false)
}

/// Execute queued control requests from the `svc:` server.
pub(crate) fn pump(core: &Arc<Mutex<Core>>) {
    let (starts, terminations) = {
        let mut guard = core.lock();
        (
            core::mem::take(&mut guard.pending_starts),
            core::mem::take(&mut guard.pending_terminations),
        )
    };
    for handle in terminations {
        // The worker owns the handle and observes the terminal state through PROC_WAIT.
        let _ = raven_rt::proc_terminate(handle, 0);
    }
    for request in starts {
        let should_start = { core.lock().supervisor.should_start(request) };
        if should_start {
            spawn_worker(core, request.index, request.generation);
        }
    }
}

/// Start one worker thread for `index`.
pub(crate) fn spawn_worker(core: &Arc<Mutex<Core>>, index: usize, generation: u64) {
    let argument = ((generation as usize) << 32) | (index & 0xFFFF_FFFF);
    match raven_rt::thread::spawn(crate::worker::thread, argument, 96 * 1024) {
        Ok(_thread) => {}
        Err(_) => {
            let notices = {
                let mut guard = core.lock();
                guard.supervisor.spawn_failed(
                    index,
                    generation,
                    "cannot create the worker thread".to_string(),
                )
            };
            emit(core, &notices);
        }
    }
}

/// Print the supervisor's notices.
pub(crate) fn emit(core: &Arc<Mutex<Core>>, notices: &[Notice]) {
    if notices.is_empty() {
        return;
    }
    let guard = core.lock();
    for notice in notices {
        match notice {
            Notice::Up(index) => {
                if let Some(service) = guard.supervisor.services().get(*index) {
                    raven_rt::println!("svcd: {} up", service.name);
                }
            }
            Notice::Failed(index, reason) => {
                if let Some(service) = guard.supervisor.services().get(*index) {
                    raven_rt::println!("svcd: {} failed: {}", service.name, reason);
                }
            }
            Notice::CrashLoop(index) => {
                if let Some(service) = guard.supervisor.services().get(*index) {
                    raven_rt::println!("svcd: {} crash loop, giving up", service.name);
                }
            }
        }
    }
}

/// Spawn `manifest.exec` with its args, environment and grants.
pub(crate) fn spawn_service(manifest: &Manifest, from_root: bool) -> Result<Handle, String> {
    let initfs = {
        let guard = INITFS.lock();
        guard.as_ref().copied()
    };
    let Some(initfs) = initfs else {
        return Err("initfs is not mapped".to_string());
    };
    let mut grants = Vec::new();
    let specs = crate::grants::parse_grants(&manifest.name, &manifest.grants)?;
    for spec in specs {
        grants.push(Grant {
            handle: spec.handle(),
            rights: spec.rights(),
        });
    }
    let args: Vec<&str> = manifest.args.iter().map(String::as_str).collect();
    let environment: Vec<String> = manifest
        .env
        .iter()
        .map(|(key, value)| alloc::format!("{key}={value}"))
        .collect();
    let environment: Vec<&str> = environment.iter().map(String::as_str).collect();
    let options = SpawnOptions {
        args: &args,
        env: &environment,
    };

    if from_root {
        let path = alloc::format!("file:/root{}", manifest.exec);
        match raven_rt::spawn::read_file_image(&path) {
            Ok(image) => {
                let mut sys = GrantSys {
                    grants: grants.clone(),
                };
                return raven_rt::spawn::spawn_image_with(
                    &image,
                    &manifest.exec,
                    &options,
                    &mut sys,
                )
                .map_err(describe_spawn);
            }
            Err(SpawnError::FileNotFound) => {
                raven_rt::println!(
                    "svcd: {}: {} not found, using initfs {}",
                    manifest.name,
                    path,
                    manifest.exec
                );
            }
            Err(error) => return Err(describe_spawn(error)),
        }
    }

    let mut sys = GrantSys { grants };
    raven_rt::spawn::spawn_with(&initfs, &manifest.exec, &options, &mut sys).map_err(describe_spawn)
}

/// The current monotonic time in milliseconds.
pub(crate) fn now_ms() -> u64 {
    match raven_rt::clock_get(clock::MONOTONIC) {
        Ok(time) => {
            let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
            let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
            seconds
                .saturating_mul(1_000)
                .saturating_add(nanos / 1_000_000)
        }
        Err(_) => FALLBACK_MS.fetch_add(1, Ordering::Relaxed),
    }
}

/// The process-wide core, once `run` has initialised it.
pub(crate) fn core_handle() -> Option<Arc<Mutex<Core>>> {
    CORE.lock().clone()
}

/// Move the registered `svc:` server into its thread.
pub(crate) fn take_server() -> Option<Server> {
    SERVER.lock().take()
}

#[derive(Clone, Copy)]
struct Grant {
    handle: Handle,
    rights: Rights,
}

struct GrantSys {
    grants: Vec<Grant>,
}

impl Sys for GrantSys {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        RtSys.proc_create()
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
        RtSys.memobj_create(len)
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno> {
        RtSys.map(proc, memobj, offset, addr, len, flags)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        RtSys.unmap(proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        RtSys.protect(proc, addr, len, protection)
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        for grant in &self.grants {
            raven_rt::handle_transfer(proc, grant.handle, grant.rights)?;
        }
        RtSys.thread_create(proc, entry, stack_top, arg)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        RtSys.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        RtSys.write_parent_mem(addr, bytes);
    }
}

fn read_config(initfs: &Initfs<'_>) -> Config {
    let Some(entry) = initfs.get("/etc/svcd.conf") else {
        return Config::default();
    };
    let Ok(text) = core::str::from_utf8(entry.data) else {
        raven_rt::println!("svcd: /etc/svcd.conf: not UTF-8");
        return Config::default();
    };
    let (config, warnings) = config::parse(text);
    for warning in warnings {
        raven_rt::println!("svcd: /etc/svcd.conf: {}", warning);
    }
    config
}

fn is_manifest_path(path: &str) -> bool {
    path.starts_with("/etc/services.d/") && path.ends_with(".toml")
}

fn describe_spawn(error: SpawnError) -> String {
    match error {
        SpawnError::NotFound => "executable not found in the initfs".to_string(),
        SpawnError::FileNotFound => "executable not found on the file server".to_string(),
        SpawnError::BadElf(error) => alloc::format!("bad ELF: {error:?}"),
        SpawnError::HasInterpreter => "ELF interpreter is not supported".to_string(),
        SpawnError::TooLarge => "image is too large".to_string(),
        SpawnError::Sys(error) => error.description().to_string(),
    }
}

/// Render one manifest parse error for `svcd: <file>: <error>`.
pub(crate) fn describe_parse_error(error: &ParseError) -> String {
    let detail = match &error.kind {
        ParseErrorKind::Syntax => "syntax error".to_string(),
        ParseErrorKind::UnknownKey(key) => alloc::format!("unknown key `{key}`"),
        ParseErrorKind::DuplicateKey(key) => alloc::format!("duplicate key `{key}`"),
        ParseErrorKind::MissingKey(key) => alloc::format!("missing key `{key}`"),
        ParseErrorKind::InvalidValue(key) => alloc::format!("invalid value for `{key}`"),
        ParseErrorKind::IntegerOutOfRange(key) => {
            alloc::format!("value out of range for `{key}`")
        }
        ParseErrorKind::InvalidName => "invalid service name".to_string(),
        ParseErrorKind::InvalidExec => "exec must be an absolute path".to_string(),
        ParseErrorKind::TooLarge => "value too large".to_string(),
    };
    alloc::format!("line {}: {}", error.line, detail)
}

/// Render one dependency-graph error for the fatal `svcd: <reason>` line.
pub(crate) fn describe_graph_error(error: &GraphError) -> String {
    match error {
        GraphError::TooLarge => "too many services".to_string(),
        GraphError::Missing { service, need } => {
            alloc::format!("service `{service}` needs `{need}`, which is not provided")
        }
        GraphError::AmbiguousProvider => "ambiguous service name or scheme provider".to_string(),
        GraphError::Cycle(cycle) => {
            alloc::format!("dependency cycle: {}", cycle.join(" -> "))
        }
    }
}
