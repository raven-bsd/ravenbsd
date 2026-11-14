// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `prctl` option table (ticket C66 5.H family `prctl`).

use super::*;
use crate::testutil::FakeProc;

#[test]
fn set_name_truncates_to_fifteen_bytes() {
    let mut proc = FakeProc::new();
    assert_eq!(set_name(&mut proc, b"a-name-longer-than-fifteen"), Ok(0));
    assert_eq!(&proc.name[..5], b"a-nam");
    assert_eq!(proc.name[15], 0);
}

#[test]
fn name_roundtrip() {
    let mut proc = FakeProc::new();
    set_name(&mut proc, b"worker").unwrap();
    assert_eq!(get_name(&mut proc).unwrap(), *b"worker\0\0\0\0\0\0\0\0\0\0");
}

#[test]
fn set_pdeathsig_accepts_valid_signals() {
    let mut proc = FakeProc::new();
    assert_eq!(prctl(&mut proc, PR_SET_PDEATHSIG, 0, 0, 0, 0), Ok(0));
    assert_eq!(prctl(&mut proc, PR_SET_PDEATHSIG, 9, 0, 0, 0), Ok(0));
    assert_eq!(proc.pdeathsig, 9);
    assert_eq!(prctl(&mut proc, PR_GET_PDEATHSIG, 0, 0, 0, 0), Ok(9));
}

#[test]
fn set_pdeathsig_rejects_out_of_range() {
    let mut proc = FakeProc::new();
    assert_eq!(
        prctl(&mut proc, PR_SET_PDEATHSIG, 65, 0, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn dumpable_roundtrip_and_validation() {
    let mut proc = FakeProc::new();
    assert_eq!(prctl(&mut proc, PR_GET_DUMPABLE, 0, 0, 0, 0), Ok(1));
    assert_eq!(prctl(&mut proc, PR_SET_DUMPABLE, 0, 0, 0, 0), Ok(0));
    assert_eq!(proc.dumpable, 0);
    assert_eq!(
        prctl(&mut proc, PR_SET_DUMPABLE, 2, 0, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn subreaper_roundtrip_and_validation() {
    let mut proc = FakeProc::new();
    assert_eq!(prctl(&mut proc, PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0), Ok(0));
    assert_eq!(prctl(&mut proc, PR_GET_CHILD_SUBREAPER, 0, 0, 0, 0), Ok(1));
    assert_eq!(
        prctl(&mut proc, PR_SET_CHILD_SUBREAPER, 3, 0, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn no_new_privs_requires_exactly_one() {
    let mut proc = FakeProc::new();
    assert_eq!(
        prctl(&mut proc, PR_SET_NO_NEW_PRIVS, 0, 0, 0, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        prctl(&mut proc, PR_SET_NO_NEW_PRIVS, 1, 1, 0, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(prctl(&mut proc, PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), Ok(0));
    assert_eq!(prctl(&mut proc, PR_GET_NO_NEW_PRIVS, 0, 0, 0, 0), Ok(1));
}

#[test]
fn timerslack_default_and_store() {
    let mut proc = FakeProc::new();
    assert_eq!(prctl(&mut proc, PR_GET_TIMERSLACK, 0, 0, 0, 0), Ok(50_000));
    assert_eq!(prctl(&mut proc, PR_SET_TIMERSLACK, 7, 0, 0, 0), Ok(0));
    assert_eq!(prctl(&mut proc, PR_GET_TIMERSLACK, 0, 0, 0, 0), Ok(7));
}

#[test]
fn name_options_are_rejected_by_the_value_dispatcher() {
    let mut proc = FakeProc::new();
    assert_eq!(
        prctl(&mut proc, PR_SET_NAME, 0, 0, 0, 0),
        Err(Errno::EINVAL)
    );
    assert_eq!(
        prctl(&mut proc, PR_GET_NAME, 0, 0, 0, 0),
        Err(Errno::EINVAL)
    );
}

#[test]
fn unknown_option_is_einval() {
    let mut proc = FakeProc::new();
    for option in [0, 5, 14, 31, 35, 40, 99, i32::MAX] {
        assert_eq!(prctl(&mut proc, option, 0, 0, 0, 0), Err(Errno::EINVAL));
    }
}

#[test]
fn every_supported_option_has_a_row() {
    let supported = [
        PR_SET_PDEATHSIG,
        PR_GET_PDEATHSIG,
        PR_GET_DUMPABLE,
        PR_SET_DUMPABLE,
        PR_SET_NAME,
        PR_GET_NAME,
        PR_SET_TIMERSLACK,
        PR_GET_TIMERSLACK,
        PR_SET_CHILD_SUBREAPER,
        PR_GET_CHILD_SUBREAPER,
        PR_SET_NO_NEW_PRIVS,
        PR_GET_NO_NEW_PRIVS,
    ];
    let mut proc = FakeProc::new();
    for option in supported {
        let result = if option == PR_SET_NAME {
            set_name(&mut proc, b"x")
        } else if option == PR_GET_NAME {
            get_name(&mut proc).map(|_| 0)
        } else if option == PR_SET_NO_NEW_PRIVS {
            prctl(&mut proc, option, 1, 0, 0, 0)
        } else {
            prctl(&mut proc, option, 0, 0, 0, 0)
        };
        assert!(result.is_ok(), "option {option} failed: {result:?}");
    }
}

#[test]
fn default_trait_methods_are_enosys() {
    struct Bare;
    impl crate::seams::ProcCtl for Bare {
        fn prctl_name_set(&mut self, _name: &[u8; 16]) -> Result<(), Errno> {
            Ok(())
        }
        fn prctl_name_get(&mut self) -> Result<[u8; 16], Errno> {
            Ok([0; 16])
        }
        fn set_title(&mut self, _title: Option<&[u8]>) -> Result<(), Errno> {
            Ok(())
        }
    }
    let mut bare = Bare;
    assert_eq!(
        prctl(&mut bare, PR_GET_DUMPABLE, 0, 0, 0, 0),
        Err(Errno::ENOSYS)
    );
}
