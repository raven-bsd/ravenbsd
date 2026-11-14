// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The watchdog and memory-cap wrapper around a target body.
//!
//! The body runs on a worker thread. A timeout returns [`ReplayError::Timeout`] and leaks the
//! worker (joining a hung thread would hang the test binary); the test process still fails.

use std::sync::Mutex;
use std::sync::mpsc;
use std::time::Duration;

use crate::{Body, MAX_INPUT, ReplayError, TIMEOUT_SECS, alloc_probe};

static SERIAL: Mutex<()> = Mutex::new(());

/// Run one body under `TIMEOUT_SECS` and `cap`.
pub fn execute(
    target: &'static str,
    body: Body,
    data: &[u8],
    cap: usize,
) -> Result<(), ReplayError> {
    execute_with_timeout(target, body, data, cap, TIMEOUT_SECS)
}

/// The testable form with an explicit timeout.
pub fn execute_with_timeout(
    target: &'static str,
    body: Body,
    data: &[u8],
    cap: usize,
    secs: u64,
) -> Result<(), ReplayError> {
    let limited = data.get(..MAX_INPUT).unwrap_or(data).to_vec();
    let _guard = SERIAL
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let (sender, receiver) = mpsc::channel();
    let _worker = std::thread::Builder::new()
        .name(format!("fuzz-{target}"))
        .spawn(move || {
            alloc_probe::mark();
            let result = std::panic::catch_unwind(|| body(&limited));
            let peak = alloc_probe::peak_since_mark();
            let message = result.err().map(|payload| panic_message(payload.as_ref()));
            let _ = sender.send((message, peak));
        })
        .map_err(ReplayError::Io)?;

    match receiver.recv_timeout(Duration::from_secs(secs)) {
        Ok((Some(message), _peak)) => Err(ReplayError::Panicked { target, message }),
        Ok((None, peak)) if peak > cap => Err(ReplayError::MemoryCap { target, peak, cap }),
        Ok((None, _peak)) => Ok(()),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(ReplayError::Timeout { target, secs }),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(ReplayError::Panicked {
            target,
            message: "the body thread disconnected".to_string(),
        }),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

#[cfg(test)]
#[path = "harness_ut.rs"]
mod tests;
