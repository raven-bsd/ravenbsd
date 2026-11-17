// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the service status and reopen helpers.

use super::*;

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::client::Client;
use crate::kernel::Kernel;
use crate::testing::FakeKernel;
use raven_abi::types::Handle;

#[test]
fn parse_generation_accepts_decimal_lines() {
    assert_eq!(parse_generation(b"1\n"), Ok(1));
    assert_eq!(parse_generation(b"42\r\n"), Ok(42));
    assert_eq!(parse_generation(b"0"), Ok(0));
    assert_eq!(parse_generation(b"x"), Err(Errno::EPROTO));
}

#[test]
fn parse_service_state_maps_tokens() {
    assert_eq!(
        parse_service_state(b"rst running 7 0\n"),
        ServiceState::Running
    );
    assert_eq!(
        parse_service_state(b"rst starting 0 0\n"),
        ServiceState::Pending
    );
    assert_eq!(
        parse_service_state(b"rst restarting 0 1\n"),
        ServiceState::Pending
    );
    assert_eq!(
        parse_service_state(b"rst stopped 0 0\n"),
        ServiceState::Stopped
    );
    assert_eq!(
        parse_service_state(b"rst gave-up 0 0\n"),
        ServiceState::Stopped
    );
}

#[test]
fn service_generation_opens_the_gen_file() {
    let kernel = FakeKernel::new();
    kernel.set_read_data(b"3\n");
    let client = Client::with_kernel(kernel.clone());
    assert_eq!(service_generation(&client, "rst"), Ok(3));
    assert!(
        kernel
            .open_calls()
            .iter()
            .any(|(path, _)| path == "svc:/rst.gen")
    );
}

#[test]
fn service_running_opens_the_status_file() {
    let kernel = FakeKernel::new();
    kernel.set_read_data(b"rst running 7 0\n");
    let client = Client::with_kernel(kernel.clone());
    assert_eq!(service_running(&client, "rst"), Ok(true));
    assert!(
        kernel
            .open_calls()
            .iter()
            .any(|(path, _)| path == "svc:/rst")
    );
}

#[derive(Clone)]
struct ScriptKernel {
    state: Rc<RefCell<ScriptState>>,
}

struct ScriptState {
    next: u32,
    rst_failures: u32,
    generation: u64,
    files: BTreeMap<u32, Vec<u8>>,
}

impl ScriptKernel {
    fn new(rst_failures: u32, generation: u64) -> Self {
        Self {
            state: Rc::new(RefCell::new(ScriptState {
                next: 0,
                rst_failures,
                generation,
                files: BTreeMap::new(),
            })),
        }
    }

    fn remaining_failures(&self) -> u32 {
        self.state.borrow().rst_failures
    }
}

impl Kernel for ScriptKernel {
    fn open(&self, path: &str, _flags: u32) -> Result<Handle, Errno> {
        let mut state = self.state.borrow_mut();
        if path == "rst:" && state.rst_failures > 0 {
            state.rst_failures -= 1;
            return Err(Errno::ENOENT);
        }
        let next = state.next + 1;
        state.next = next;
        let data = match path {
            "svc:/rst.gen" => format!("{}\n", state.generation).into_bytes(),
            "svc:/rst" => b"rst running 7 0\n".to_vec(),
            _ => b"rst\n".to_vec(),
        };
        state.files.insert(next, data);
        Ok(Handle(next))
    }

    fn read(&self, file: Handle, _offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        let state = self.state.borrow();
        let data = state.files.get(&file.0).cloned().unwrap_or_default();
        let length = data.len().min(buffer.len());
        buffer[..length].copy_from_slice(&data[..length]);
        Ok(length)
    }

    fn close(&self, _file: Handle) -> Result<(), Errno> {
        Ok(())
    }
}

#[test]
fn reopen_waits_for_the_advanced_generation() {
    let kernel = ScriptKernel::new(2, 2);
    let client = Client::with_kernel(kernel.clone());
    let handle = reopen(&client, "rst:", "rst", 1, &|_nanos| {}).unwrap();
    assert_eq!(handle, Handle(5));
    assert_eq!(kernel.remaining_failures(), 0);
}

#[test]
fn reopen_is_bounded_when_the_scheme_stays_down() {
    let kernel = ScriptKernel::new(u32::MAX, 2);
    let client = Client::with_kernel(kernel.clone());
    assert_eq!(
        reopen(&client, "rst:", "rst", 1, &|_nanos| {}),
        Err(Errno::ENOENT)
    );
    // One initial attempt plus MAX_REOPEN_TRIES retries were consumed.
    assert_eq!(
        kernel.remaining_failures(),
        u32::MAX - (MAX_REOPEN_TRIES + 1)
    );
}

#[test]
fn reopen_does_not_retry_without_a_generation_change() {
    let kernel = ScriptKernel::new(u32::MAX, 1);
    let client = Client::with_kernel(kernel.clone());
    assert_eq!(
        reopen(&client, "rst:", "rst", 1, &|_nanos| {}),
        Err(Errno::ENOENT)
    );
    // Only the initial attempt was consumed: the generation never advanced.
    assert_eq!(kernel.remaining_failures(), u32::MAX - 1);
}
