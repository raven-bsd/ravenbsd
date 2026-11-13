// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `waitid`.

use super::*;

struct Fake {
    result: Result<i32, Errno>,
    seen: Option<(IdType, i64, u32)>,
}

impl Wait6 for Fake {
    fn wait6(
        &mut self,
        idtype: IdType,
        id: i64,
        options: u32,
        info: &mut [u8; 128],
    ) -> Result<i32, Errno> {
        self.seen = Some((idtype, id, options));
        info[0] = 0xaa;
        self.result
    }
}

#[test]
fn a_ready_child_keeps_the_record() {
    let mut fake = Fake {
        result: Ok(7),
        seen: None,
    };
    let mut info = [0u8; 128];
    assert_eq!(waitid(&mut fake, IdType::Pid, 7, &mut info, 0), Ok(()));
    assert_eq!(info[0], 0xaa);
    assert_eq!(fake.seen, Some((IdType::Pid, 7, 0)));
}

#[test]
fn wnohang_with_nothing_ready_zeroes_the_record() {
    let mut fake = Fake {
        result: Ok(0),
        seen: None,
    };
    let mut info = [0xa5u8; 128];
    assert_eq!(waitid(&mut fake, IdType::All, 0, &mut info, 1), Ok(()));
    assert!(info.iter().all(|byte| *byte == 0));
}

#[test]
fn an_error_is_propagated() {
    let mut fake = Fake {
        result: Err(crate::errors::ECHILD),
        seen: None,
    };
    let mut info = [0u8; 128];
    assert_eq!(
        waitid(&mut fake, IdType::Pid, 1, &mut info, 0),
        Err(crate::errors::ECHILD)
    );
}
