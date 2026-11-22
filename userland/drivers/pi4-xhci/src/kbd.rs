// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Best-effort bridge from the xHCI keyboard service to the `kbd:` console queue.

use raven_scheme::Client;

const OPEN_WRITE: u32 = 2;
const ATTEMPTS: u8 = 5;
const RETRY_MS: u64 = 10;

/// Push decoded ASCII bytes to the `usb-hid` `kbd:` service.
///
/// The service is optional for the fake `pi4-usb` qtest (which drains its local queue instead);
/// on the Pi 4 release `usb-xhci` starts after `usb-hid`, so the first open normally succeeds.
/// A missing service is not a driver failure: the local queue and the transcript still show the
/// decoded key.
pub(crate) fn push(bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    let client = Client::new();
    for _ in 0..ATTEMPTS {
        if let Ok(handle) = client.open("kbd:console", OPEN_WRITE) {
            let _ = client.write(handle, 0, bytes);
            let _ = client.close(handle);
            return;
        }
        let _ = raven_rt::nanosleep(RETRY_MS * 1_000_000);
    }
}
