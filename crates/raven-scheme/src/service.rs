// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Service status and bounded reopen over the `svc:` control scheme.
//!
//! `svc:/<name>` returns the `<name> <state> <handle> <restarts>` line and `svc:/<name>.gen`
//! returns the service generation. A client that loses its scheme handle to a restarted service
//! reads the generation, reopens a bounded number of times, and fails with the open error when the
//! service has not come back up. There is no silent retry loop.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::client::Client;
use crate::kernel::Kernel;

/// Maximum reopen attempts before [`reopen`] gives up.
pub const MAX_REOPEN_TRIES: u32 = 10;
/// Sleep between reopen attempts, in nanoseconds.
pub const REOPEN_DELAY_NS: u64 = 10_000_000;

/// The service states that matter to a reconnecting client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServiceState {
    /// Not running.
    Stopped,
    /// Running and healthy.
    Running,
    /// Starting or restarting; not ready yet.
    Pending,
}

impl ServiceState {
    /// The stable `svc:` spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Running => "running",
            Self::Pending => "pending",
        }
    }
}

/// Parse the generation value returned by `svc:/<name>.gen`.
///
/// # Errors
///
/// Returns [`Errno::EPROTO`] when the bytes are not a decimal `u64`.
pub fn parse_generation(bytes: &[u8]) -> Result<u64, Errno> {
    let text = core::str::from_utf8(bytes).map_err(|_| Errno::EPROTO)?;
    text.trim().parse::<u64>().map_err(|_| Errno::EPROTO)
}

/// Parse the state token from one `svc:/<name>` status line.
///
/// `running` maps to [`ServiceState::Running`]; `starting` and `restarting` map to
/// [`ServiceState::Pending`]; everything else maps to [`ServiceState::Stopped`].
pub fn parse_service_state(bytes: &[u8]) -> ServiceState {
    let line = bytes
        .strip_suffix(b"\r\n")
        .or_else(|| bytes.strip_suffix(b"\n"))
        .unwrap_or(bytes);
    let state = line.split(|byte| *byte == b' ').nth(1).unwrap_or(&[]);
    match state {
        b"running" => ServiceState::Running,
        b"starting" | b"restarting" => ServiceState::Pending,
        _ => ServiceState::Stopped,
    }
}

/// Read the generation of one service.
///
/// # Errors
///
/// Returns the open/read/parse error.
pub fn service_generation<K: Kernel>(client: &Client<K>, name: &str) -> Result<u64, Errno> {
    let path = alloc::format!("svc:/{name}.gen");
    let handle = client.open(&path, 1)?;
    let mut buffer = [0u8; 24];
    let read = client.read(handle, 0, &mut buffer)?;
    let _ = client.close(handle);
    parse_generation(buffer.get(..read).unwrap_or(&[]))
}

/// Read the state of one service.
///
/// # Errors
///
/// Returns the open/read error.
pub fn service_state<K: Kernel>(client: &Client<K>, name: &str) -> Result<ServiceState, Errno> {
    let path = alloc::format!("svc:/{name}");
    let handle = client.open(&path, 1)?;
    let mut buffer = [0u8; 96];
    let read = client.read(handle, 0, &mut buffer)?;
    let _ = client.close(handle);
    Ok(parse_service_state(buffer.get(..read).unwrap_or(&[])))
}

/// Whether one service is currently running.
///
/// # Errors
///
/// Returns the status read error.
pub fn service_running<K: Kernel>(client: &Client<K>, name: &str) -> Result<bool, Errno> {
    service_state(client, name).map(|state| state == ServiceState::Running)
}

/// Reopen `path` after a service restart.
///
/// `previous` is the generation the caller observed before the restart. The caller supplies a
/// nanosecond `sleep` (for example `raven_rt::nanosleep`). The open is retried only while the
/// service has advanced past `previous` and reports itself running, at most
/// [`MAX_REOPEN_TRIES`] times. Any other failure returns the open error unchanged, so a client
/// never loops silently behind a stale handle.
///
/// # Errors
///
/// Returns the last `OPEN` error (for example `EIO`, `ENOENT` or `ESTALE`).
pub fn reopen<K: Kernel>(
    client: &Client<K>,
    path: &str,
    name: &str,
    previous: u64,
    sleep: &dyn Fn(u64),
) -> Result<Handle, Errno> {
    let mut tries = 0u32;
    loop {
        match client.open(path, 1) {
            Ok(handle) => return Ok(handle),
            Err(error) => {
                let advanced =
                    service_generation(client, name).is_ok_and(|generation| generation != previous);
                let running = service_running(client, name).unwrap_or(false);
                if !(advanced && running) || tries >= MAX_REOPEN_TRIES {
                    return Err(error);
                }
                tries = tries.saturating_add(1);
                sleep(REOPEN_DELAY_NS);
            }
        }
    }
}

#[cfg(test)]
#[path = "service_ut.rs"]
mod tests;
