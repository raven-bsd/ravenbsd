// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `wait3`.

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
        let present = rusage.is_some();
        if let Some(record) = rusage {
            record[0] = 7;
        }
        self.calls.push((pid, options, present));
        self.result
    }
}

#[test]
fn wait3_calls_wait4_with_any_and_rusage() {
    let mut fake = Fake {
        calls: alloc::vec::Vec::new(),
        result: Ok(5),
    };
    let mut rusage = [0u8; 144];
    assert_eq!(wait3(&mut fake, 2, &mut rusage), Ok(5));
    assert_eq!(fake.calls, alloc::vec![(-1, 2, true)]);
    assert_eq!(rusage[0], 7);
}

#[test]
fn wait3_propagates_an_error() {
    let mut fake = Fake {
        calls: alloc::vec::Vec::new(),
        result: Err(crate::errors::EINTR),
    };
    let mut rusage = [0u8; 144];
    assert_eq!(wait3(&mut fake, 0, &mut rusage), Err(crate::errors::EINTR));
}
