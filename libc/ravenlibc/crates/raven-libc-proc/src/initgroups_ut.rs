// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `initgroups`.

use super::*;

struct Fake {
    count: usize,
    user: alloc::vec::Vec<u8>,
    basegid: u32,
    installed: alloc::vec::Vec<u32>,
    get_error: Option<Errno>,
    set_error: Option<Errno>,
}

impl Fake {
    fn new(count: usize) -> Self {
        Self {
            count,
            user: alloc::vec::Vec::new(),
            basegid: 0,
            installed: alloc::vec::Vec::new(),
            get_error: None,
            set_error: None,
        }
    }
}

impl GroupSource for Fake {
    fn getgrouplist(
        &mut self,
        user: &[u8],
        basegid: u32,
        groups: &mut [u32],
    ) -> Result<usize, Errno> {
        if let Some(error) = self.get_error {
            return Err(error);
        }
        self.user = user.to_vec();
        self.basegid = basegid;
        for (index, slot) in groups.iter_mut().enumerate().take(self.count) {
            *slot = index as u32;
        }
        Ok(self.count)
    }

    fn setgroups(&mut self, groups: &[u32]) -> Result<(), Errno> {
        if let Some(error) = self.set_error {
            return Err(error);
        }
        self.installed = groups.to_vec();
        Ok(())
    }
}

#[test]
fn a_small_list_is_installed() {
    let mut source = Fake::new(3);
    assert_eq!(initgroups(&mut source, b"alice", 7), Ok(()));
    assert_eq!(source.user, b"alice");
    assert_eq!(source.basegid, 7);
    assert_eq!(source.installed, alloc::vec![0, 1, 2]);
}

#[test]
fn a_long_list_is_truncated_silently() {
    let mut source = Fake::new(NGROUPS_MAX + 10);
    assert_eq!(initgroups(&mut source, b"alice", 0), Ok(()));
    assert_eq!(source.installed.len(), NGROUPS_MAX);
}

#[test]
fn getgrouplist_errors_are_propagated() {
    let mut source = Fake::new(3);
    source.get_error = Some(crate::errors::ENOMEM);
    assert_eq!(
        initgroups(&mut source, b"alice", 0),
        Err(crate::errors::ENOMEM)
    );
    assert!(source.installed.is_empty());
}

#[test]
fn setgroups_errors_are_propagated() {
    let mut source = Fake::new(3);
    source.set_error = Some(crate::errors::EPERM);
    assert_eq!(
        initgroups(&mut source, b"alice", 0),
        Err(crate::errors::EPERM)
    );
}
