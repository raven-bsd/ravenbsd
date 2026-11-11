// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target glue: initfs, manifests, spawning, waves and the process-wide core.

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::clock;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_proto_svc::State;
use raven_rt::spawn::{RtSys, SpawnError, SpawnOptions, Sys};
use raven_rt::sync::Mutex;
use raven_scheme::Server;
use raven_svc_manifest::{
    GraphError, Manifest, ParseError, ParseErrorKind, UnknownService, apply_overrides,
    parse_overrides,
};

use crate::config::{self, Config};
use crate::state::{Notice, StartRequest, Supervisor};

/// The mapped initfs, set once at boot.
static INITFS: Mutex<Option<Initfs<'static>>> = Mutex::new(None);
/// The process-wide supervisor state.
static CORE: Mutex<Option<Arc<Mutex<Core>>>> = Mutex::new(None);
/// The registered `svc:` server, moved into its thread.
static SERVER: Mutex<Option<Server>> = Mutex::new(None);
/// The `svc:` server thread, joined before `svcd` exits.
static SERVER_THREAD: Mutex<Option<raven_rt::thread::Thread>> = Mutex::new(None);
/// Worker threads, joined before `svcd` exits in test mode.
static WORKERS: Mutex<Vec<raven_rt::thread::Thread>> = Mutex::new(Vec::new());
/// Fallback clock counter when `CLOCK_MONOTONIC` is unavailable.
static FALLBACK_MS: AtomicU64 = AtomicU64::new(0);
/// Number of helper threads that have not yet returned from their entry function.
static LIVE_THREADS: AtomicU32 = AtomicU32::new(0);

/// State shared by the boot thread, workers and the `svc:` server.
pub(crate) struct Core {
    pub(crate) supervisor: Supervisor,
    pub(crate) config: Config,
    pub(crate) exit_code: Option<i32>,
    pub(crate) pending_starts: Vec<StartRequest>,
    pub(crate) pending_closes: Vec<Handle>,
    pub(crate) all_up_printed: bool,
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
    let core = Arc::new(Mutex::new(Core {
        supervisor,
        config,
        exit_code: None,
        pending_starts: Vec::new(),
        pending_closes: Vec::new(),
        all_up_printed: false,
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
    LIVE_THREADS.fetch_add(1, Ordering::AcqRel);
    match raven_rt::thread::spawn(crate::svc::thread, 0, 96 * 1024) {
        Ok(thread) => *SERVER_THREAD.lock() = Some(thread),
        Err(_) => {
            LIVE_THREADS.fetch_sub(1, Ordering::AcqRel);
            raven_rt::println!("svcd: svc: cannot create the server thread");
            return 1;
        }
    }

    let waves = { core.lock().supervisor.plan().waves.clone() };
    for wave in waves {
        for &index in &wave {
            let generation = { core.lock().supervisor.start(index, now_ms()) };
            if let Some(generation) = generation {
                spawn_worker(&core, index, generation);
            }
        }
        loop {
            pump(&core);
            if let Some(code) = core.lock().exit_code {
                shutdown();
                return code;
            }
            let done = {
                let guard = core.lock();
                wave.iter().all(|&index| {
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
                break;
            }
            let _ = raven_rt::nanosleep(5_000_000);
        }
    }

    core.lock().supervisor.set_boot_complete();
    loop {
        pump(&core);
        if let Some(code) = core.lock().exit_code {
            shutdown();
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
            shutdown();
            return settled_code;
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
}

/// Execute queued control requests from the `svc:` server.
pub(crate) fn pump(core: &Arc<Mutex<Core>>) {
    let (starts, closes) = {
        let mut guard = core.lock();
        (
            core::mem::take(&mut guard.pending_starts),
            core::mem::take(&mut guard.pending_closes),
        )
    };
    for handle in closes {
        let _ = raven_rt::handle_close(handle);
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
    LIVE_THREADS.fetch_add(1, Ordering::AcqRel);
    match raven_rt::thread::spawn(crate::worker::thread, argument, 96 * 1024) {
        Ok(thread) => WORKERS.lock().push(thread),
        Err(_) => {
            LIVE_THREADS.fetch_sub(1, Ordering::AcqRel);
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

/// Record that one helper thread is about to return from its entry function.
pub(crate) fn thread_finished() {
    LIVE_THREADS.fetch_sub(1, Ordering::AcqRel);
}

/// Stop the `svc:` server and wait for every helper thread to return before `main` exits.
///
/// `EXIT` marks only the calling thread dead; the process ends when its last thread exits. Test
/// modes therefore stop the server and wait for the workers they created. The `Thread` handles are
/// dropped instead of joined: `Thread::join` unmaps the stack as soon as the done word is set,
/// which races with the joining thread's `futex_wake`/`thread_exit` and faults. The whole address
/// space is released by the process exit, so leaking those stacks is harmless here.
fn shutdown() {
    crate::svc::request_stop();
    let deadline = now_ms().saturating_add(2_000);
    while LIVE_THREADS.load(Ordering::Acquire) != 0 && now_ms() < deadline {
        let _ = raven_rt::nanosleep(1_000_000);
    }
    // Give the last thread time to execute THREAD_EXIT after its entry function returned.
    let _ = raven_rt::nanosleep(20_000_000);
    let _ = SERVER_THREAD.lock().take();
    let workers = {
        let mut guard = WORKERS.lock();
        core::mem::take(&mut *guard)
    };
    drop(workers);
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
pub(crate) fn spawn_service(manifest: &Manifest) -> Result<Handle, String> {
    let initfs = {
        let guard = INITFS.lock();
        guard.as_ref().copied()
    };
    let Some(initfs) = initfs else {
        return Err("initfs is not mapped".to_string());
    };
    let mut grants = Vec::new();
    for grant in &manifest.grants {
        match grant.as_str() {
            "initfs" => grants.push(Handle::INITFS),
            other => return Err(alloc::format!("unknown grant `{other}`")),
        }
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

struct GrantSys {
    grants: Vec<Handle>,
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

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        for source in &self.grants {
            raven_rt::handle_transfer(proc, *source, Rights::ALL)?;
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
