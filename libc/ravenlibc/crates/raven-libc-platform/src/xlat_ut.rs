// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Table tests for the translation tables and the hand-copied fixture.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;
use raven_proto_fs::{
    WIRE_OPEN_APPEND, WIRE_OPEN_CREATE, WIRE_OPEN_DIRECTORY, WIRE_OPEN_NOFOLLOW, WIRE_OPEN_READ,
    WIRE_OPEN_TRUNC, WIRE_OPEN_WRITE,
};

use crate::xlat::{self, FcntlAction, IoctlAction};

fn fixture() -> Vec<(String, i64)> {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/xlat.toml"
    ))
    .expect("fixture readable");
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let parsed = if let Some(hex) = value.strip_prefix("0x") {
            i64::from_str_radix(hex, 16).expect("hex")
        } else if let Some(octal) = value.strip_prefix("0o") {
            i64::from_str_radix(octal, 8).expect("octal")
        } else {
            value.parse::<i64>().expect("decimal")
        };
        out.push((name.trim().to_string(), parsed));
    }
    out
}

#[test]
fn fixture_matches_the_open_table() {
    let rows = fixture();
    for (name, value) in rows {
        if let Some((_, expected, _)) = xlat::OPEN_FLAGS.iter().find(|row| row.0 == name) {
            assert_eq!(i64::from(*expected), value, "open row {name}");
        }
    }
}

#[test]
fn fixture_matches_the_fcntl_table() {
    let rows = fixture();
    for (name, value) in rows {
        if let Some((_, expected, _)) = xlat::FCNTL_COMMANDS.iter().find(|row| row.0 == name) {
            assert_eq!(i64::from(*expected), value, "fcntl row {name}");
        }
    }
}

#[test]
fn fixture_matches_the_ioctl_table() {
    let rows = fixture();
    for (name, value) in rows {
        if let Some((_, expected, _)) = xlat::IOCTL_REQUESTS.iter().find(|row| row.0 == name) {
            assert_eq!(i64::from(*expected), value, "ioctl row {name}");
        }
    }
}

#[test]
fn fixture_matches_the_stat_defaults() {
    let rows = fixture();
    for (name, value) in rows {
        if let Some((_, expected)) = crate::stat::STAT_DEFAULTS.iter().find(|row| row.0 == name) {
            assert_eq!(*expected, value, "stat row {name}");
        }
    }
}

#[test]
fn every_fixture_row_is_in_a_table() {
    let rows = fixture();
    let mut known = 0usize;
    for (name, _) in &rows {
        let in_table = xlat::OPEN_FLAGS.iter().any(|row| row.0 == name)
            || xlat::FCNTL_COMMANDS.iter().any(|row| row.0 == name)
            || xlat::IOCTL_REQUESTS.iter().any(|row| row.0 == name)
            || crate::stat::STAT_DEFAULTS.iter().any(|row| row.0 == name);
        assert!(in_table, "fixture row {name} has no source table row");
        known = known.saturating_add(1);
    }
    assert_eq!(known, rows.len());
    assert!(known >= 50);
}

#[test]
fn open_read_only() {
    let spec = xlat::open_flags(0).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_READ);
    assert_eq!(spec.status, 0);
    assert!(!spec.cloexec);
}

#[test]
fn open_write_only() {
    let spec = xlat::open_flags(fcntl::O_WRONLY as u32).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_WRITE);
    assert_eq!(spec.status, 1);
}

#[test]
fn open_read_write() {
    let spec = xlat::open_flags(fcntl::O_RDWR as u32).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_READ | WIRE_OPEN_WRITE);
    assert_eq!(spec.status, 2);
}

#[test]
fn open_create_and_excl() {
    let spec =
        xlat::open_flags((fcntl::O_RDWR | fcntl::O_CREAT | fcntl::O_EXCL) as u32).expect("valid");
    assert_eq!(
        spec.wire,
        WIRE_OPEN_READ | WIRE_OPEN_WRITE | WIRE_OPEN_CREATE | 1 << 3
    );
}

#[test]
fn open_excl_without_create_is_ignored() {
    let spec = xlat::open_flags((fcntl::O_RDONLY | fcntl::O_EXCL) as u32).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_READ);
}

#[test]
fn open_trunc_on_read_only_is_dropped() {
    let spec = xlat::open_flags((fcntl::O_RDONLY | fcntl::O_TRUNC) as u32).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_READ);
}

#[test]
fn open_trunc_on_write_is_kept() {
    let spec = xlat::open_flags((fcntl::O_WRONLY | fcntl::O_TRUNC) as u32).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_WRITE | WIRE_OPEN_TRUNC);
}

#[test]
fn open_append_and_flags() {
    let spec = xlat::open_flags(
        (fcntl::O_WRONLY
            | fcntl::O_APPEND
            | fcntl::O_CLOEXEC
            | fcntl::O_NONBLOCK
            | fcntl::O_NOFOLLOW) as u32,
    )
    .expect("valid");
    assert_eq!(
        spec.wire,
        WIRE_OPEN_WRITE | WIRE_OPEN_APPEND | WIRE_OPEN_NOFOLLOW
    );
    assert!(spec.cloexec);
    assert_eq!(
        spec.status & fcntl::O_NONBLOCK as u32,
        fcntl::O_NONBLOCK as u32
    );
}

#[test]
fn open_directory() {
    let spec = xlat::open_flags((fcntl::O_RDONLY | fcntl::O_DIRECTORY) as u32).expect("valid");
    assert_eq!(spec.wire, WIRE_OPEN_READ | WIRE_OPEN_DIRECTORY);
}

#[test]
fn open_path_is_eopnotsupp() {
    assert_eq!(
        xlat::open_flags(fcntl::O_PATH as u32),
        Err(Errno::EOPNOTSUPP)
    );
}

#[test]
fn open_tmpfile_is_eopnotsupp() {
    assert_eq!(
        xlat::open_flags(fcntl::O_TMPFILE as u32),
        Err(Errno::EOPNOTSUPP)
    );
}

#[test]
fn open_unknown_bit_is_einval() {
    assert_eq!(xlat::open_flags(1 << 30), Err(Errno::EINVAL));
}

#[test]
fn fcntl_actions() {
    assert_eq!(xlat::fcntl_action(fcntl::F_DUPFD), FcntlAction::DupFd);
    assert_eq!(
        xlat::fcntl_action(fcntl::F_DUPFD_CLOEXEC),
        FcntlAction::DupFdCloexec
    );
    assert_eq!(xlat::fcntl_action(fcntl::F_GETFD), FcntlAction::GetFd);
    assert_eq!(xlat::fcntl_action(fcntl::F_SETFD), FcntlAction::SetFd);
    assert_eq!(xlat::fcntl_action(fcntl::F_GETFL), FcntlAction::GetFl);
    assert_eq!(xlat::fcntl_action(fcntl::F_SETFL), FcntlAction::SetFl);
    assert_eq!(xlat::fcntl_action(fcntl::F_GETLK), FcntlAction::GetLk);
    assert_eq!(xlat::fcntl_action(fcntl::F_SETLKW), FcntlAction::SetLkw);
    assert_eq!(
        xlat::fcntl_action(fcntl::F_OFD_GETLK),
        FcntlAction::OfdGetLk
    );
    assert_eq!(xlat::fcntl_action(9999), FcntlAction::Invalid);
}

#[test]
fn ioctl_actions() {
    assert_eq!(xlat::ioctl_action(xlat::TCGETS), IoctlAction::TcGets);
    assert_eq!(xlat::ioctl_action(xlat::TCSETS), IoctlAction::TcSets);
    assert_eq!(
        xlat::ioctl_action(xlat::TIOCGWINSZ),
        IoctlAction::TiocGWinsz
    );
    assert_eq!(xlat::ioctl_action(xlat::FIONREAD), IoctlAction::FionRead);
    assert_eq!(xlat::ioctl_action(xlat::FIONBIO), IoctlAction::FionBio);
    assert_eq!(xlat::ioctl_action(0xdead), IoctlAction::Unknown);
}

#[test]
fn call_status_decoding() {
    assert_eq!(xlat::call_status(0), Ok(()));
    assert_eq!(xlat::call_status(-2), Err(Errno::ENOENT));
    assert_eq!(xlat::call_status(-4095), Err(Errno(4095)));
    assert_eq!(xlat::call_status(1), Err(Errno::EPROTO));
    assert_eq!(xlat::call_status(-4096), Err(Errno::EPROTO));
}

#[test]
fn packet_status_decoding() {
    assert_eq!(xlat::packet_status(0), Ok(()));
    assert_eq!(xlat::packet_status(2), Err(Errno::ENOENT));
    assert_eq!(xlat::packet_status(4095), Err(Errno(4095)));
    assert_eq!(xlat::packet_status(4096), Err(Errno::EPROTO));
}
