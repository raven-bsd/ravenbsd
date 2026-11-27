// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ioctl` dispatch tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::ioctl::{ioctl, ioctl_tcgets, ioctl_tcsets};
use crate::platform::Platform;
use crate::xlat;

#[test]
fn fionbio_sets_and_clears_nonblock() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    ioctl(&platform, fd, xlat::FIONBIO, 1).expect("set");
    let status = platform.get(fd).expect("ref").status();
    assert_ne!(status & fcntl::O_NONBLOCK as u32, 0);
    ioctl(&platform, fd, xlat::FIONBIO, 0).expect("clear");
    let status = platform.get(fd).expect("ref").status();
    assert_eq!(status & fcntl::O_NONBLOCK as u32, 0);
}

#[test]
fn fioclex_and_fionclex() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    ioctl(&platform, fd, xlat::FIOCLEX, 0).expect("set");
    assert!(platform.cloexec(fd).expect("cloexec"));
    ioctl(&platform, fd, xlat::FIONCLEX, 0).expect("clear");
    assert!(!platform.cloexec(fd).expect("cloexec"));
}

#[test]
fn unknown_request_is_enotty() {
    let platform = Platform::new(FakeSys::new());
    assert_eq!(ioctl(&platform, 0, 0xdead, 0), Err(Errno::ENOTTY));
}

#[test]
fn tcgets_needs_a_tty() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(ioctl_tcgets(&platform, fd), Err(Errno::ENOTTY));
}

#[test]
fn tcsets_needs_a_tty() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/a", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    let termios = crate::ioctl::tty::default_termios();
    assert_eq!(ioctl_tcsets(&platform, fd, &termios), Err(Errno::ENOTTY));
}

#[test]
fn net_ioctl_classification() {
    use crate::ioctl::net::{NetIoctl, classify, net_ioctl};
    // FreeBSD sys/sys/sockio.h at 133344405: _IOW/_IOWR('i', n, struct ifreq) with
    // sizeof(struct ifreq) == 32, and SIOCGIFCONF over the 16-byte struct ifconf.
    assert_eq!(classify(0xc010_6924), Some(NetIoctl::GetIfConf));
    assert_eq!(classify(0xc020_6911), Some(NetIoctl::GetIfFlags));
    assert_eq!(classify(0x8020_6910), Some(NetIoctl::SetIfFlags));
    assert_eq!(classify(0xc020_6921), Some(NetIoctl::GetIfAddr));
    assert_eq!(classify(0x8020_690c), Some(NetIoctl::SetIfAddr));
    assert_eq!(classify(0xc020_6925), Some(NetIoctl::GetIfNetmask));
    assert_eq!(classify(0xc020_6933), Some(NetIoctl::GetIfMtu));
    // Any other request in the interface command group stays a network request.
    assert_eq!(classify(0xc020_6915), Some(NetIoctl::Other));
    // The old Linux encodings are not the native requests.
    assert_eq!(classify(0x8913), None);
    assert_eq!(classify(0x1234), None);
    assert_eq!(net_ioctl(0xc020_6911), Errno::ENOSYS);
    assert_eq!(net_ioctl(0x8913), Errno::ENOTTY);
    assert_eq!(net_ioctl(0x1234), Errno::ENOTTY);
}
