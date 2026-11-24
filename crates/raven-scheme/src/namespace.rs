// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Process scheme-namespace updates.
//!
//! A child process receives a snapshot of the schemes that were registered when it was created.
//! A service manager that starts a provider later (a driver publishing `disk:`, `blockd`
//! publishing `block:`, `fsd-ufs` publishing `ufs:`) makes it visible to an already-created
//! child by granting a scheme-name prefix on that child before its first thread starts. The
//! kernel checks the prefix against the caller's own namespace, so PID 1 (the wildcard root) can
//! grant the names its manifests declare while a confined service cannot widen itself.

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, namespace};

/// Largest scheme name the kernel accepts (mirrors the router's `NAME_MAX`).
pub const NAME_MAX: usize = 64;

/// Validate a prefix exactly as the kernel's namespace syscall does.
///
/// Only the lower-case scheme alphabet is accepted; an empty or oversized prefix is rejected
/// before any syscall, which keeps callers from turning a manifest typo into a broad grant.
pub fn valid_prefix(prefix: &str) -> bool {
    let bytes = prefix.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= NAME_MAX
        && bytes.iter().all(|byte| {
            matches!(
                *byte,
                b'a'..=b'z' | b'0'..=b'9' | b':' | b'_' | b'-'
            )
        })
}

/// Grant every scheme name starting with `prefix` to `proc`.
///
/// `proc` is `Handle::SELF_PROC` or a process handle with `ADMIN` rights. The caller must be
/// allowed to grant the prefix under the kernel's namespace rule.
pub fn allow_prefix(proc: Handle, prefix: &str) -> Result<(), Errno> {
    update(proc, namespace::ADD, prefix)
}

/// Remove a prefix grant from `proc`; handles opened earlier keep working.
pub fn remove_prefix(proc: Handle, prefix: &str) -> Result<(), Errno> {
    update(proc, namespace::REMOVE, prefix)
}

/// Remove every prefix from `proc`; exact scheme names stay visible.
pub fn clear_prefixes(proc: Handle) -> Result<(), Errno> {
    Errno::from_ret(crate::sys::syscall6(
        Syscall::PROC_NAMESPACE as usize,
        proc.0 as usize,
        namespace::CLEAR as usize,
        0,
        0,
        0,
        0,
    ))
    .map(|_| ())
}

fn update(proc: Handle, op: u32, prefix: &str) -> Result<(), Errno> {
    if !valid_prefix(prefix) {
        return Err(Errno::EINVAL);
    }
    Errno::from_ret(crate::sys::syscall6(
        Syscall::PROC_NAMESPACE as usize,
        proc.0 as usize,
        op as usize,
        prefix.as_ptr() as usize,
        prefix.len(),
        0,
        0,
    ))
    .map(|_| ())
}

#[cfg(test)]
#[path = "namespace_ut.rs"]
mod tests;
