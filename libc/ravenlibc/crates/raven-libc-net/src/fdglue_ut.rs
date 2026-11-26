// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests.

use raven_libc_platform::Platform;
use raven_libc_platform::backend::FakeSys;

use crate::fdglue::FdGlue;
use crate::{Sock, SocketFds};

#[test]
fn pending_socket_installs_and_attaches_the_handle() {
    let platform = Platform::new(FakeSys::new());
    let glue = FdGlue::new(&platform);
    let fd = glue.install(None, false, true).expect("pending install");
    let pending = glue.resolve(fd).expect("resolve");
    assert_eq!(pending.sock, None);
    assert!(pending.nonblock);
    glue.attach(pending.ofd, Sock(5));
    let attached = glue.resolve(fd).expect("resolve");
    assert_eq!(attached.sock, Some(Sock(5)));
    let _ = platform.close(fd);
}
