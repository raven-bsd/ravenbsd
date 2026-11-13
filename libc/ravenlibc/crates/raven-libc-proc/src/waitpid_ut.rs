// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `waitpid`.

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
fn waitpid_passes_the_pid_and_options() {
    let mut fake = Fake {
        calls: alloc::vec::Vec::new(),
        result: Ok(9),
    };
    assert_eq!(waitpid(&mut fake, 9, 3), Ok(9));
    assert_eq!(fake.calls, alloc::vec![(9, 3, false)]);
}

#[test]
fn waitpid_passes_a_negative_group() {
    let mut fake = Fake {
        calls: alloc::vec::Vec::new(),
        result: Ok(1),
    };
    assert_eq!(waitpid(&mut fake, -7, 0), Ok(1));
    assert_eq!(fake.calls, alloc::vec![(-7, 0, false)]);
}
