// SPDX-License-Identifier: MIT
// Copyright (c) 2014 The Rust Project Developers

//! Header: `sys/ipc.h`
//!
//! <https://github.com/NetBSD/src/blob/trunk/sys/sys/ipc.h>

use crate::prelude::*;

s! {
    pub struct ipc_perm {
        pub uid: crate::uid_t,
        pub gid: crate::gid_t,
        pub cuid: crate::uid_t,
        pub cgid: crate::gid_t,
        pub mode: crate::mode_t,
        pub _seq: c_ushort,
        pub _key: crate::key_t,
    }
}
