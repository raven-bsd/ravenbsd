// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Client helpers over the S11 scheme syscalls.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_fs::{CallRequest, Stat};

use crate::kernel::{Kernel, RtKernel};

/// A typed client for one or more scheme files.
pub struct Client<K: Kernel = RtKernel> {
    kernel: K,
}

impl Client<RtKernel> {
    /// Create a client using the real syscall implementation.
    pub const fn new() -> Self {
        Self { kernel: RtKernel }
    }
}

impl Default for Client<RtKernel> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: Kernel> Client<K> {
    /// Create a client over a caller-provided kernel implementation.
    pub const fn with_kernel(kernel: K) -> Self {
        Self { kernel }
    }

    /// Borrow the kernel implementation.
    pub const fn kernel(&self) -> &K {
        &self.kernel
    }

    /// Open a scheme path with S11 flags.
    pub fn open(&self, path: &str, flags: u32) -> Result<Handle, Errno> {
        self.kernel.open(path, flags)
    }

    /// Read from `file` at `offset`.
    pub fn read(&self, file: Handle, offset: u64, buffer: &mut [u8]) -> Result<usize, Errno> {
        self.kernel.read(file, offset, buffer)
    }

    /// Write to `file` at `offset`.
    pub fn write(&self, file: Handle, offset: u64, buffer: &[u8]) -> Result<usize, Errno> {
        self.kernel.write(file, offset, buffer)
    }

    /// Close `file`.
    pub fn close(&self, file: Handle) -> Result<(), Errno> {
        self.kernel.close(file)
    }

    /// Duplicate `file` with a rights mask.
    pub fn dup(&self, file: Handle, rights: u32) -> Result<Handle, Errno> {
        self.kernel.dup(file, rights)
    }

    /// Read the `FSTAT` record for `file`.
    pub fn fstat(&self, file: Handle) -> Result<Stat, Errno> {
        let mut data = [0u8; 64];
        self.kernel.fstat(file, &mut data)?;
        Stat::decode(&data).map_err(|_| Errno::EPROTO)
    }

    /// Send a typed `CALL` request.
    pub fn call(&self, file: Handle, request: &[u8], response: &mut [u8]) -> Result<usize, Errno> {
        self.kernel.call(file, request, response)
    }

    /// Encode and send a [`CallRequest`].
    pub fn call_typed(
        &self,
        file: Handle,
        request: &CallRequest<'_>,
        response: &mut [u8],
    ) -> Result<usize, Errno> {
        let mut bytes = [0u8; 640];
        let length = request.encode(&mut bytes).map_err(|_| Errno::EINVAL)?;
        self.call(file, &bytes[..length], response)
    }
}
