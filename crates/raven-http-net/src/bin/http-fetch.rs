// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `http-fetch`: one bounded GET through `raven_http` + `raven_http_net`.
//!
//! The E3a live gate invokes this static tool in the release image:
//!
//! `http-fetch http://10.0.2.2:18080/known.txt`
//!
//! It verifies the status, the `Content-Length` and the exact body, prints one marker per fact and
//! returns 0 only on a complete match. The host side is a one-shot local server; no external
//! network and no TLS are involved.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::clock;
    use raven_http::{Client, Clock, RequestBuilder};
    use raven_http_net::live_transport;
    use raven_rt::entry;

    entry!(main);

    /// The exact body served by `tests/fixtures/http-fetch/server.py`.
    const EXPECTED_BODY: &[u8] = b"known-http-body";
    /// The exact wire length of [`EXPECTED_BODY`].
    const EXPECTED_LENGTH: &str = "15";
    /// Default URL when the shell does not pass one.
    const DEFAULT_URL: &str = "http://10.0.2.2:18080/known.txt";
    /// Bound the response memory for this tiny gate.
    const MAX_BODY: usize = 4096;
    /// Total fetch budget.
    const TIMEOUT_MS: u32 = 20_000;

    struct SystemClock;

    impl Clock for SystemClock {
        fn now_ms(&self) -> u64 {
            match raven_rt::clock_get(clock::MONOTONIC) {
                Ok(time) => {
                    let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
                    let nanos = u64::try_from(time.tv_nsec).unwrap_or(0);
                    seconds
                        .saturating_mul(1_000)
                        .saturating_add(nanos / 1_000_000)
                }
                Err(_) => 0,
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let url = args.get(1).unwrap_or(DEFAULT_URL);
        let request = match RequestBuilder::new(url) {
            Ok(builder) => builder.timeout_ms(TIMEOUT_MS).max_body(MAX_BODY).build(),
            Err(error) => {
                raven_rt::eprintln!("http-fetch: request={error:?}");
                return 1;
            }
        };
        let mut transport = live_transport();
        let clock = SystemClock;
        let response = {
            let mut client = Client::new(&mut transport, &clock);
            match client.get(&request) {
                Ok(response) => response,
                Err(error) => {
                    raven_rt::eprintln!("http-fetch: error={error:?}");
                    return 1;
                }
            }
        };
        raven_rt::println!("http-fetch: status={}", response.status);
        let length = response.header("content-length").unwrap_or("missing");
        raven_rt::println!("http-fetch: content-length={length}");
        let body = core::str::from_utf8(&response.body).unwrap_or("<binary>");
        raven_rt::println!("http-fetch: body={body}");
        if response.status != 200 {
            raven_rt::eprintln!("http-fetch: status-mismatch");
            return 1;
        }
        if length != EXPECTED_LENGTH {
            raven_rt::eprintln!("http-fetch: length-mismatch");
            return 1;
        }
        if response.body != EXPECTED_BODY {
            raven_rt::eprintln!("http-fetch: body-mismatch");
            return 1;
        }
        raven_rt::println!("http-fetch: ok");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
