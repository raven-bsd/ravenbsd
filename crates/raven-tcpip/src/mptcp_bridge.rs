// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MPTCP2a: bridge `raven_mptcp::Subflow` to the real plain-TCP `SubflowOps`.
//!
//! The adapter owns a shared stack (the host driver polls it) and one `SubflowId`. It exposes the
//! four lifecycle operations the meta-socket needs: send, live readiness, close and the backup
//! flag is held by `MetaSocket` itself. The feature is off by default; without `--features mptcp`
//! the plain TCP crate is byte-identical.
//!
//! This slice deliberately does not put MPTCP options on the wire. The host test feeds the
//! mapping metadata (DSS) to the peer `MetaSocket` after the real TCP bytes arrive, so the
//! transport bridge, reordering and failover are exercised without touching `raven-tcpip`'s
//! handshake. MPTCP2b owns the wire integration.

use alloc::rc::Rc;
#[cfg(any(test, feature = "testutil"))]
use alloc::vec::Vec;
use core::cell::RefCell;

use raven_abi::errno::Errno;
use raven_mptcp::{Error, Mapping, Subflow, SubflowReadiness};

use crate::device::Device;
use crate::mptcp_hooks::{SubflowId, SubflowOps};
use crate::readiness::Readiness;
use crate::stack::Stack;

/// One DSS mapping and the bytes that carried it, recorded for the host peer.
#[cfg(any(test, feature = "testutil"))]
#[derive(Clone, Debug)]
pub struct Mapped {
    /// The meta-socket mapping.
    pub mapping: Mapping,
    /// The payload that travelled over the real TCP subflow.
    pub data: Vec<u8>,
}

/// A `raven_mptcp::Subflow` backed by one real TCP socket.
pub struct StackSubflow<D: Device + 'static> {
    stack: Rc<RefCell<Stack<D>>>,
    id: SubflowId,
    #[cfg(any(test, feature = "testutil"))]
    mappings: Rc<RefCell<Vec<Mapped>>>,
}

impl<D: Device + 'static> StackSubflow<D> {
    /// Wrap a subflow created with [`SubflowOps::open_subflow`].
    #[must_use]
    pub fn new(stack: Rc<RefCell<Stack<D>>>, id: SubflowId) -> Self {
        Self {
            stack,
            id,
            #[cfg(any(test, feature = "testutil"))]
            mappings: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// Record sent mappings in a shared log for the host peer (testutil only).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn with_mapping_log(self, mappings: Rc<RefCell<Vec<Mapped>>>) -> Self {
        Self { mappings, ..self }
    }

    /// Take the mappings sent so far (testutil only).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn take_mappings(&self) -> Vec<Mapped> {
        core::mem::take(&mut *self.mappings.borrow_mut())
    }

    /// The underlying hook handle.
    #[must_use]
    pub fn id(&self) -> SubflowId {
        self.id
    }
}

impl<D: Device + 'static> Subflow for StackSubflow<D> {
    fn send(&mut self, _mapping: Mapping, payload: &[u8], _data_fin: bool) -> Result<(), Error> {
        let mut stack = self.stack.borrow_mut();
        match stack.send_on(self.id, payload) {
            Ok(count) if count == payload.len() => {
                #[cfg(any(test, feature = "testutil"))]
                self.mappings.borrow_mut().push(Mapped {
                    mapping: _mapping,
                    data: payload.to_vec(),
                });
                Ok(())
            }
            Ok(_) => Err(Error::WouldBlock),
            Err(Errno::EAGAIN) => Err(Error::WouldBlock),
            Err(Errno::EBADF | Errno::ECONNRESET | Errno::ENOTCONN) => Err(Error::Closed),
            Err(_) => Err(Error::WouldBlock),
        }
    }

    fn readiness(&mut self) -> SubflowReadiness {
        let stack = self.stack.borrow();
        let Readiness {
            readable,
            writable,
            hup,
            error,
        } = SubflowOps::readiness(&*stack, self.id);
        SubflowReadiness {
            readable,
            writable,
            closed: hup || error,
        }
    }

    fn close(&mut self) {
        self.stack.borrow_mut().close_subflow(self.id);
    }
}
