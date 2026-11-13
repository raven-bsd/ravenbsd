// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! fdsan tests: tag encoding, error levels and the mismatch matrix.

use crate::fd::fdsan::{
    Action, ErrorLevel, FdsanState, Mismatch, OwnerType, tag, tag_type, tag_value,
};

#[test]
fn tag_round_trip() {
    let value = tag(OwnerType::File, 0x1234);
    assert_eq!(tag_type(value), OwnerType::File as u8);
    assert_eq!(tag_value(value), 0x1234);
}

#[test]
fn zero_value_is_unowned() {
    assert_eq!(tag(OwnerType::File, 0), 0);
    assert_eq!(tag_type(0), 0);
    assert_eq!(tag_value(0), 0);
}

#[test]
fn tag_type_keeps_every_owner() {
    for owner in [
        OwnerType::File,
        OwnerType::Dir,
        OwnerType::UniqueFd,
        OwnerType::Libc,
        OwnerType::Socket,
        OwnerType::ZipArchive,
        OwnerType::NativeHandle,
        OwnerType::RustFile,
        OwnerType::GenericFf,
    ] {
        assert_eq!(tag_type(tag(owner, 7)), owner as u8);
    }
}

#[test]
fn tag_value_is_sign_extended() {
    let raw = 0x0080_0000_0000_0001u64;
    assert_eq!(tag_value(raw), 0xff80_0000_0000_0001u64);
}

#[test]
fn level_from_env() {
    assert_eq!(ErrorLevel::from_env(None), ErrorLevel::WarnAlways);
    assert_eq!(ErrorLevel::from_env(Some(b"warn")), ErrorLevel::WarnAlways);
    assert_eq!(
        ErrorLevel::from_env(Some(b"warn_once")),
        ErrorLevel::WarnOnce
    );
    assert_eq!(ErrorLevel::from_env(Some(b"fatal")), ErrorLevel::Fatal);
    assert_eq!(ErrorLevel::from_env(Some(b"1")), ErrorLevel::Fatal);
    assert_eq!(ErrorLevel::from_env(Some(b"off")), ErrorLevel::Disabled);
    assert_eq!(ErrorLevel::from_env(Some(b"0")), ErrorLevel::Disabled);
    assert_eq!(ErrorLevel::from_env(Some(b"other")), ErrorLevel::WarnAlways);
}

#[test]
fn disabled_reports_nothing() {
    let state = FdsanState::new(ErrorLevel::Disabled);
    assert_eq!(state.classify(Mismatch::DoubleClose), Action::None);
}

#[test]
fn warn_reports_every_time() {
    let state = FdsanState::new(ErrorLevel::WarnAlways);
    assert_eq!(state.classify(Mismatch::DoubleClose), Action::Warn);
    assert_eq!(state.classify(Mismatch::DoubleClose), Action::Warn);
}

#[test]
fn warn_once_reports_once() {
    let state = FdsanState::new(ErrorLevel::WarnOnce);
    assert_eq!(state.classify(Mismatch::DoubleClose), Action::Warn);
    assert_eq!(state.classify(Mismatch::DoubleClose), Action::None);
}

#[test]
fn fatal_is_an_abort() {
    let state = FdsanState::new(ErrorLevel::Fatal);
    assert_eq!(state.classify(Mismatch::DoubleClose), Action::Abort);
}

#[test]
fn level_can_be_changed() {
    let state = FdsanState::new(ErrorLevel::WarnAlways);
    assert_eq!(state.level(), ErrorLevel::WarnAlways);
    assert_eq!(state.set_level(ErrorLevel::Fatal), ErrorLevel::WarnAlways);
    assert_eq!(state.level(), ErrorLevel::Fatal);
}

#[test]
fn wrong_owner_message_names_both() {
    let message = Mismatch::WrongOwner {
        expected: tag(OwnerType::File, 1),
        actual: tag(OwnerType::Dir, 2),
    }
    .message();
    assert!(message.contains("FILE*"));
    assert!(message.contains("DIR*"));
    assert!(message.contains("0x1"));
    assert!(message.contains("0x2"));
}

#[test]
fn expected_owned_and_unowned_messages() {
    let owned = Mismatch::ExpectedOwned {
        expected: tag(OwnerType::File, 3),
    }
    .message();
    assert!(owned.contains("unowned"));
    let unowned = Mismatch::ExpectedUnowned {
        actual: tag(OwnerType::RustFile, 4),
    }
    .message();
    assert!(unowned.contains("Rust File"));
}

#[test]
fn double_close_message() {
    assert!(Mismatch::DoubleClose.message().contains("double-close"));
}

#[cfg(feature = "fdsan")]
mod enabled {
    use raven_libc_abi::fcntl;

    use crate::backend::FakeSys;
    use crate::fd::fdsan::{OwnerType, tag};
    use crate::platform::Platform;

    fn platform() -> Platform<FakeSys> {
        let fake = FakeSys::new();
        fake.file(b"/a", b"x");
        Platform::new(fake)
    }

    #[test]
    fn matching_tag_is_silent() {
        let platform = platform();
        let fd = platform
            .open(b"/a", fcntl::O_RDONLY as u32, 0)
            .expect("open");
        let owner = tag(OwnerType::File, 7);
        platform.set_tag(fd, owner).expect("tag");
        platform.close_with_tag(fd, owner).expect("close");
        assert!(platform.backend().debug_output().is_empty());
    }

    #[test]
    fn mismatch_warns_through_debug_write() {
        let platform = platform();
        let fd = platform
            .open(b"/a", fcntl::O_RDONLY as u32, 0)
            .expect("open");
        platform
            .close_with_tag(fd, tag(OwnerType::File, 7))
            .expect("close");
        let output = platform.backend().debug_output();
        assert!(output.starts_with(b"fdsan: "));
        assert!(String::from_utf8_lossy(&output).contains("actually unowned"));
    }

    #[test]
    fn double_close_is_reported() {
        let platform = platform();
        let fd = platform
            .open(b"/a", fcntl::O_RDONLY as u32, 0)
            .expect("open");
        let owner = tag(OwnerType::Dir, 1);
        platform.set_tag(fd, owner).expect("tag");
        platform.close_with_tag(fd, owner).expect("first close");
        assert_eq!(platform.close_with_tag(fd, owner), Err(crate::Errno::EBADF));
        let output = platform.backend().debug_output();
        assert!(String::from_utf8_lossy(&output).contains("double-close"));
    }
}
