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

/// Register `name` for `process`.
pub fn register(process: &Arc<Process>, name: &str) -> Result<(), Errno> {
    let mut registry = REGISTRY.lock();
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
    registry.insert(name.to_string(), server);
    Ok(())
}

/// Resolve `name` for `process`, enforcing the process namespace.
pub fn lookup(process: &Arc<Process>, name: &str) -> Result<Arc<Process>, Errno> {
    let mut registry = REGISTRY.lock();
    let Some(server) = registry.get(name).cloned() else {
        return Err(Errno::ENOENT);
    };
    if !process.namespace.lock().allows(name) {
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
pub fn on_process_exit(process: &Arc<Process>) {
    let servers: Vec<Arc<SchemeServer>> = process.schemes.lock().clone();
    let mut registry = REGISTRY.lock();
    for server in servers {
        server
            .dead
            .store(true, core::sync::atomic::Ordering::Release);
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
