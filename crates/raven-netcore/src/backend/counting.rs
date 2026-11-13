// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Device decorator that records packet and byte counters at the native boundary.

use alloc::rc::Rc;
use core::cell::Cell;

use crate::{Device, DeviceCaps, Instant, InterfaceCounters, TxError};

/// Counters shared between the device wrapper and the backend.
pub(crate) type SharedCounters = Rc<Cell<InterfaceCounters>>;

/// One device plus counters for every frame crossing the native stack boundary.
pub(crate) struct CountingDevice<D: Device> {
    inner: D,
    counters: SharedCounters,
}

impl<D: Device> CountingDevice<D> {
    pub(crate) fn new(inner: D, counters: SharedCounters) -> Self {
        Self { inner, counters }
    }
}

impl<D: Device> Device for CountingDevice<D> {
    fn capabilities(&self) -> DeviceCaps {
        self.inner.capabilities()
    }

    fn receive(&mut self, now: Instant, consume: &mut dyn FnMut(&[u8])) -> bool {
        let counters = Rc::clone(&self.counters);
        self.inner.receive(now, &mut |frame| {
            let mut value = counters.get();
            value.rx_packets = value.rx_packets.saturating_add(1);
            value.rx_bytes = value.rx_bytes.saturating_add(frame.len() as u64);
            counters.set(value);
            consume(frame);
        })
    }

    fn transmit(
        &mut self,
        now: Instant,
        len: usize,
        fill: &mut dyn FnMut(&mut [u8]),
    ) -> Result<(), TxError> {
        let result = self.inner.transmit(now, len, fill);
        let mut value = self.counters.get();
        match result {
            Ok(()) => {
                value.tx_packets = value.tx_packets.saturating_add(1);
                value.tx_bytes = value.tx_bytes.saturating_add(len as u64);
                self.counters.set(value);
                Ok(())
            }
            Err(error) => {
                value.tx_errors = value.tx_errors.saturating_add(1);
                self.counters.set(value);
                Err(error)
            }
        }
    }
}
