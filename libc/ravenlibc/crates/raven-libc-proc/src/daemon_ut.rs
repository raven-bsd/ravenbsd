// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `daemon` model.

use super::*;

struct Fake {
    fork_result: Result<DaemonFork, Errno>,
    log: alloc::vec::Vec<alloc::string::String>,
}

impl Fake {
    fn new(fork_result: Result<DaemonFork, Errno>) -> Self {
        Self {
            fork_result,
            log: alloc::vec::Vec::new(),
        }
    }
}

impl DaemonOps for Fake {
    fn fork(&mut self) -> Result<DaemonFork, Errno> {
        self.log.push(alloc::string::String::from("fork"));
        self.fork_result
    }

    fn setsid(&mut self) -> Result<(), Errno> {
        self.log.push(alloc::string::String::from("setsid"));
        Ok(())
    }

    fn chdir_root(&mut self) -> Result<(), Errno> {
        self.log.push(alloc::string::String::from("chdir"));
        Ok(())
    }

    fn open_devnull(&mut self) -> Result<i32, Errno> {
        self.log.push(alloc::string::String::from("open"));
        Ok(5)
    }

    fn dup2(&mut self, fd: i32, newfd: i32) -> Result<(), Errno> {
        self.log.push(alloc::format!("dup2:{fd}:{newfd}"));
        Ok(())
    }

    fn close(&mut self, fd: i32) -> Result<(), Errno> {
        self.log.push(alloc::format!("close:{fd}"));
        Ok(())
    }
}

#[test]
fn the_parent_step_returns_parent_exit() {
    let mut fake = Fake::new(Ok(DaemonFork::Parent));
    assert_eq!(daemon(&mut fake, false, false), Ok(DaemonStep::ParentExit));
    assert_eq!(fake.log, alloc::vec!["fork"]);
}

#[test]
fn the_child_runs_all_steps_in_order() {
    let mut fake = Fake::new(Ok(DaemonFork::Child));
    assert_eq!(daemon(&mut fake, false, false), Ok(DaemonStep::ChildReady));
    assert_eq!(
        fake.log,
        alloc::vec![
            "fork", "setsid", "chdir", "open", "dup2:5:0", "dup2:5:1", "dup2:5:2", "close:5"
        ]
    );
}

#[test]
fn nochdir_skips_the_chdir() {
    let mut fake = Fake::new(Ok(DaemonFork::Child));
    daemon(&mut fake, true, true).expect("daemon");
    assert_eq!(fake.log, alloc::vec!["fork", "setsid"]);
}

#[test]
fn noclose_skips_devnull_and_dup2() {
    let mut fake = Fake::new(Ok(DaemonFork::Child));
    daemon(&mut fake, false, true).expect("daemon");
    assert_eq!(fake.log, alloc::vec!["fork", "setsid", "chdir"]);
}

#[test]
fn a_fork_error_is_propagated() {
    let mut fake = Fake::new(Err(crate::errors::ENOSYS));
    assert_eq!(daemon(&mut fake, false, false), Err(crate::errors::ENOSYS));
}

#[test]
fn a_devnull_already_on_012_is_not_closed() {
    struct Five(Fake);
    impl DaemonOps for Five {
        fn fork(&mut self) -> Result<DaemonFork, Errno> {
            self.0.fork()
        }
        fn setsid(&mut self) -> Result<(), Errno> {
            self.0.setsid()
        }
        fn chdir_root(&mut self) -> Result<(), Errno> {
            self.0.chdir_root()
        }
        fn open_devnull(&mut self) -> Result<i32, Errno> {
            Ok(2)
        }
        fn dup2(&mut self, fd: i32, newfd: i32) -> Result<(), Errno> {
            self.0.dup2(fd, newfd)
        }
        fn close(&mut self, fd: i32) -> Result<(), Errno> {
            self.0.close(fd)
        }
    }
    let mut fake = Five(Fake::new(Ok(DaemonFork::Child)));
    daemon(&mut fake, true, false).expect("daemon");
    assert_eq!(
        fake.0.log,
        alloc::vec!["fork", "setsid", "dup2:2:0", "dup2:2:1"]
    );
}
