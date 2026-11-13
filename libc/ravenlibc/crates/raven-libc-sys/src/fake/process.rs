// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fake process table.
//!
//! Mirrors `kernel/src/syscall/process_ctl.rs:22-33` (`PROC_CREATE`), `:39-58`
//! (`PROC_TERMINATE`) and `:106-155` (`PROC_WAIT`, including the immediate reap of a process
//! that was created but never started).

use raven_abi::errno::Errno;

use super::handle::HandleTable;

/// Process state of the fake kernel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessState {
    /// Created, no thread started.
    Created,
    /// Running (the fake never schedules it).
    Running,
    /// Exited.
    Exited,
}

/// One child process.
#[derive(Clone, Debug)]
pub struct ChildProcess {
    /// Its handle table.
    pub handles: HandleTable,
    /// Current state.
    pub state: ProcessState,
    /// Exit code stored for `PROC_WAIT`.
    pub exit_code: i32,
}

impl Default for ChildProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl ChildProcess {
    /// A created process with an empty handle table.
    pub fn new() -> Self {
        Self {
            handles: HandleTable::new(),
            state: ProcessState::Created,
            exit_code: 0,
        }
    }

    /// Terminate the process, storing `code` for `PROC_WAIT`.
    pub fn terminate(&mut self, code: i32) -> Result<(), Errno> {
        if self.state == ProcessState::Exited {
            return Err(Errno::ESRCH);
        }
        self.state = ProcessState::Exited;
        self.exit_code = code;
        Ok(())
    }
}
