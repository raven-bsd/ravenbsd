// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel objects stored in a process handle table.

use alloc::sync::Arc;

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_handles::HandleTable;

use super::Process;
use super::thread::Thread;

/// Objects a handle can refer to in S08.
#[allow(dead_code)] // Object access is used by later MAP/THREAD tickets; PID 1 only closes/dups now.
#[derive(Clone)]
pub enum Object {
    /// The process itself (well-known handle 0).
    Process(Arc<Process>),
    /// The boot initfs memory object (well-known handle 1).
    MemObjectInitfs,
    /// Root namespace placeholder (well-known handle 2).
    Namespace,
    /// A thread object (created by later tickets).
    Thread(Arc<Thread>),
}

/// The kernel's handle table type.
pub type Table = HandleTable<Object>;

/// Install the well-known handles 0, 1 and 2 in a fresh process.
pub fn install_initial(table: &mut Table, process: &Arc<Process>) -> Result<(), Errno> {
    table.insert(Object::Process(Arc::clone(process)), Rights::ALL)?;
    table.insert(Object::MemObjectInitfs, Rights::READ.union(Rights::MAP))?;
    table.insert(Object::Namespace, Rights::READ)?;
    Ok(())
}
