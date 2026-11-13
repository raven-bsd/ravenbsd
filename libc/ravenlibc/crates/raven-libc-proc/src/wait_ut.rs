// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `wait`.

use super::*;

struct Fake {
    calls: alloc::vec::Vec<(i32, u32, bool)>,
    result: Result<i32, Errno>,
}

impl Wait4 for Fake {
    fn wait4(
        &mut self,
        pid: i32,
        options: u32,
        rusage: Option<&mut [u8; 144]>,
    ) -> Result<i32, Errno> {
        self.calls.push((pid, options, rusage.is_some()));
        self.result
    }
}

#[test]
fn wait_any_is_minus_one() {
    assert_eq!(WAIT_ANY, -1);
}

#[test]
fn wait_calls_wait4_with_any_and_no_options() {
    let mut fake = Fake {
        calls: alloc::vec::Vec::new(),
        result: Ok(7),
    };
    assert_eq!(wait(&mut fake), Ok(7));
    assert_eq!(fake.calls, alloc::vec![(-1, 0, false)]);
}

#[test]
fn wait_propagates_an_error() {
    let mut fake = Fake {
        calls: alloc::vec::Vec::new(),
        result: Err(crate::errors::ECHILD),
    };
    assert_eq!(wait(&mut fake), Err(crate::errors::ECHILD));
}
