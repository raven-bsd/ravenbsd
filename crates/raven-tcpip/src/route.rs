// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Longest-prefix IPv4 route table.

use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::Ipv4Addr;

/// One IPv4 route.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Route {
    /// Destination prefix.
    pub prefix: Ipv4Addr,
    /// Prefix length, 0..=32.
    pub prefix_len: u8,
    /// Next hop, or `None` for an on-link destination.
    pub gateway: Option<Ipv4Addr>,
    /// IP MTU of the path behind this route.
    pub mtu: usize,
}

impl Route {
    /// Whether this route matches `destination`.
    #[must_use]
    pub fn matches(&self, destination: Ipv4Addr) -> bool {
        if self.prefix_len > 32 {
            return false;
        }
        let mask = if self.prefix_len == 0 {
            0
        } else {
            u32::MAX << (32 - self.prefix_len)
        };
        let destination = u32::from_be_bytes(destination.octets());
        let prefix = u32::from_be_bytes(self.prefix.octets());
        destination & mask == prefix & mask
    }
}

/// A bounded route table with longest-prefix matching.
#[derive(Clone, Debug)]
pub struct RouteTable {
    routes: Vec<Route>,
    capacity: usize,
}

impl RouteTable {
    /// Create a table holding at most `capacity` routes.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            routes: Vec::new(),
            capacity,
        }
    }

    /// Add a route, replacing an identical prefix/gateway pair.
    ///
    /// # Errors
    /// Returns [`Errno::ENOSPC`] when the table is full.
    pub fn add(&mut self, route: Route) -> Result<(), Errno> {
        if route.prefix_len > 32 || route.mtu == 0 {
            return Err(Errno::EINVAL);
        }
        if let Some(existing) = self.routes.iter_mut().find(|existing| {
            existing.prefix_len == route.prefix_len && existing.prefix == route.prefix
        }) {
            *existing = route;
            return Ok(());
        }
        if self.routes.len() >= self.capacity {
            return Err(Errno::ENOSPC);
        }
        self.routes.push(route);
        Ok(())
    }

    /// Remove a route by destination prefix and length.
    pub fn remove(&mut self, prefix: Ipv4Addr, prefix_len: u8) -> bool {
        let before = self.routes.len();
        self.routes
            .retain(|route| !(route.prefix == prefix && route.prefix_len == prefix_len));
        self.routes.len() != before
    }

    /// Longest-prefix match.
    #[must_use]
    pub fn lookup(&self, destination: Ipv4Addr) -> Option<&Route> {
        self.routes
            .iter()
            .filter(|route| route.matches(destination))
            .max_by_key(|route| route.prefix_len)
    }

    /// Number of routes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// All routes.
    #[must_use]
    pub fn entries(&self) -> &[Route] {
        &self.routes
    }

    /// Remove every route.
    pub fn clear(&mut self) {
        self.routes.clear();
    }
}
