// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-only demonstration of the `raven-scheme` client API against the fake kernel.

use raven_scheme::Client;
use raven_scheme::testing::FakeKernel;

fn main() {
    let kernel = FakeKernel::new();
    kernel.set_read_data(b"hello from raven-scheme");
    let client = Client::with_kernel(kernel);
    let file = client.open("file:/demo", 1).expect("open");
    let mut buffer = [0u8; 32];
    let read = client.read(file, 0, &mut buffer).expect("read");
    println!("{}", String::from_utf8_lossy(&buffer[..read]));
    client.close(file).expect("close");
}
