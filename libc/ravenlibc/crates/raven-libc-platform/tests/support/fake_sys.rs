// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Helpers for the process-lifetime integration test.
//!
//! This is the C55 `FakeSys` wiring: one `Platform<FakeSys>` is driven through the same sequence a
//! real process performs (open, dup2, read, close, mmap, nanosleep). The helper asserts at every
//! step so a regression points at the exact operation.

use raven_libc_abi::fcntl;

use raven_libc_platform::Platform;
use raven_libc_platform::backend::FakeSys;
use raven_libc_platform::fd::ofd::OfdKind;

/// A scripted process world.
pub struct ProcessWorld {
    /// The platform under test.
    pub platform: Platform<FakeSys>,
}

impl ProcessWorld {
    /// A world with one file and one tty.
    pub fn new() -> Self {
        let fake = FakeSys::new();
        fake.dir(b"/");
        fake.dir(b"/tmp");
        fake.file(b"/tmp/x", b"hello, process");
        fake.tty(b"/dev/tty");
        Self {
            platform: Platform::new(fake),
        }
    }

    /// Open `/tmp/x`, read it through a `dup2` alias, then close both descriptors.
    pub fn open_dup_read_close(&self) -> Vec<u8> {
        let fd = self
            .platform
            .open(b"/tmp/x", fcntl::O_RDONLY as u32, 0)
            .expect("open");
        assert_eq!(fd, 0);
        self.platform.dup2(fd, 5, false).expect("dup2");
        assert_eq!(self.platform.ofd_count(), 1);
        assert_eq!(self.platform.ofd_refs(), 2);
        let mut buffer = [0u8; 32];
        let count = self.platform.read(5, &mut buffer).expect("read");
        assert_eq!(&buffer[..count], b"hello, process");
        self.platform.close(fd).expect("close fd");
        assert_eq!(self.platform.ofd_count(), 1, "the alias keeps the OFD");
        self.platform.close(5).expect("close alias");
        assert_eq!(self.platform.ofd_count(), 0);
        buffer[..count].to_vec()
    }

    /// Mmap an anonymous page and sleep; both go through the backend seam.
    pub fn mmap_and_sleep(&self) -> usize {
        let address = raven_libc_platform::mem::mmap(
            &self.platform,
            0,
            4096,
            3,
            raven_libc_abi::mman::MAP_PRIVATE | raven_libc_abi::mman::MAP_ANONYMOUS,
            -1,
            0,
        )
        .expect("mmap");
        assert!(self.platform.backend().mapped(address).is_some());
        assert_eq!(
            raven_libc_platform::time::nanosleep(&self.platform, 1_000_000),
            Ok(0)
        );
        address
    }

    /// Open `/dev/tty` and verify the kind recorded at open time.
    pub fn tty_kind(&self) -> OfdKind {
        let fd = self
            .platform
            .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
            .expect("tty open");
        let kind = self.platform.get(fd).expect("ref").kind();
        self.platform.close(fd).expect("tty close");
        kind
    }
}

impl Default for ProcessWorld {
    fn default() -> Self {
        Self::new()
    }
}
