// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Global scheme-name registry.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use spin::Mutex;

use crate::process::{Process, ProcessState};

use super::SchemeServer;

static REGISTRY: Mutex<BTreeMap<String, Arc<SchemeServer>>> = Mutex::new(BTreeMap::new());

/// Boot-service names that may not be published by an unprivileged process.
///
/// The release profile starts these servers as root; `file`, `vfs`, `tty` and `net` are the
/// critical namespace owners from the K532 contract, and the other entries are their boot-time
/// dependencies. A name not in this list keeps the ordinary first-come admission.
const PROTECTED_NAMES: &[&str] = &[
    "audio", "block", "cons", "dev", "file", "initfs", "log", "net", "null", "proc", "ramfs",
    "rand", "tty", "vfs", "zero",
];

/// Live services whose `fstat` may authorize setuid/setgid.
const EXEC_SOURCES: &[&str] = &["file", "initfs", "vfs"];

/// Credential class recorded when a protected name was first published.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Publisher {
    euid: u32,
}

/// Protected-name publisher identities survive server death; an unprivileged process must not
/// claim a name during the restart window.
static PUBLISHERS: Mutex<BTreeMap<String, Publisher>> = Mutex::new(BTreeMap::new());

/// True when `name` belongs to the protected boot-service namespace.
#[must_use]
pub fn protected_name(name: &str) -> bool {
    PROTECTED_NAMES.contains(&name)
}

/// True when a publisher with `euid` may claim `name` after `recorded` was set.
///
/// A protected name needs effective root. The recorded credential makes the restart policy
/// explicit and keeps the name reserved after its first owner dies; an unprivileged caller can
/// never satisfy it.
#[must_use]
fn publisher_allowed(name: &str, euid: u32, recorded: Option<u32>) -> bool {
    if !protected_name(name) {
        return true;
    }
    if euid != 0 {
        return false;
    }
    recorded.is_none_or(|recorded_euid| recorded_euid == euid)
}

/// Authorize `process` to publish `name` without recording a new identity.
///
/// `SCHEME_REGISTER` calls this before admission; [`register`] repeats the check so a direct
/// caller cannot bypass the authority.
pub fn authorize(process: &Arc<Process>, name: &str) -> Result<(), Errno> {
    let euid = process.ids.lock().euid;
    let recorded = PUBLISHERS.lock().get(name).map(|publisher| publisher.euid);
    if publisher_allowed(name, euid, recorded) {
        Ok(())
    } else {
        Err(Errno::EPERM)
    }
}

/// Register `name` for `process`.
pub fn register(process: &Arc<Process>, name: &str) -> Result<Arc<SchemeServer>, Errno> {
    let euid = process.ids.lock().euid;
    let mut registry = REGISTRY.lock();
    if protected_name(name) {
        let mut publishers = PUBLISHERS.lock();
        let recorded = publishers.get(name).map(|publisher| publisher.euid);
        if !publisher_allowed(name, euid, recorded) {
            return Err(Errno::EPERM);
        }
        if recorded.is_none() {
            publishers.insert(name.to_string(), Publisher { euid });
        }
    }
    if let Some(existing) = registry.get(name) {
        if existing.is_live() {
            return Err(Errno::EEXIST);
        }
        registry.remove(name);
    }
    let server = Arc::new(SchemeServer {
        name: name.to_string(),
        owner: Arc::downgrade(process),
        dead: core::sync::atomic::AtomicBool::new(false),
    });
    process.schemes.lock().push(Arc::clone(&server));
    process.namespace.lock().add(name);
    registry.insert(name.to_string(), Arc::clone(&server));
    Ok(server)
}

/// True when `server` is the live owner of a boot-trusted executable source.
///
/// The check uses the publisher's current effective uid and the registry ownership of
/// `initfs`/`file`/`vfs`; a hostile scheme's user-controlled `fstat` is never enough.
#[must_use]
pub fn file_source_trusted(server: &Arc<Process>) -> bool {
    if server.ids.lock().euid != 0 {
        return false;
    }
    let registry = REGISTRY.lock();
    EXEC_SOURCES.iter().any(|name| {
        registry.get(*name).is_some_and(|entry| {
            entry.is_live()
                && entry
                    .owner
                    .upgrade()
                    .is_some_and(|owner| Arc::ptr_eq(&owner, server))
        })
    })
}

/// Resolve `name` for `process`, enforcing the process namespace snapshot.
pub fn lookup(process: &Arc<Process>, name: &str) -> Result<Arc<Process>, Errno> {
    let mut registry = REGISTRY.lock();
    let Some(server) = registry.get(name).cloned() else {
        return Err(Errno::ENOENT);
    };
    let allowed = process.namespace.lock().allows(name);
    if !allowed {
        return Err(Errno::EACCES);
    }
    if !server.is_live() {
        registry.remove(name);
        return Err(Errno::EIO);
    }
    let Some(owner) = server.owner.upgrade() else {
        registry.remove(name);
        return Err(Errno::EIO);
    };
    Ok(owner)
}

/// Names visible to a wildcard namespace snapshot.
pub fn names() -> Vec<String> {
    REGISTRY
        .lock()
        .iter()
        .filter(|(_, server)| server.is_live())
        .map(|(name, _)| name.clone())
        .collect()
}

/// Remove every registry entry owned by `process`.
///
/// Allocation-free for the teardown path: pop one server at a time so the process's scheme list
/// is not cloned into a temporary `Vec`, and release that list before taking the registry lock
/// (register takes the registry first, so the reverse order would invert).
pub fn on_process_exit(process: &Arc<Process>) {
    loop {
        let server = process.schemes.lock().pop();
        let Some(server) = server else {
            break;
        };
        server
            .dead
            .store(true, core::sync::atomic::Ordering::Release);
        let mut registry = REGISTRY.lock();
        if registry
            .get(&server.name)
            .is_some_and(|current| Arc::ptr_eq(current, &server))
        {
            registry.remove(&server.name);
        }
    }
}

/// True when the process is still running.
pub fn owner_running(process: &Arc<Process>) -> bool {
    *process.state.lock() == ProcessState::Running
}

#[cfg(test)]
#[path = "registry_ut.rs"]
mod tests;
