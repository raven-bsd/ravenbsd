// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded chunk loop for the vfsd proxy.

/// Failure while copying through a chunk callback.
#[allow(dead_code)] // the proxy caller is target-only; the host build omits it
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ChunkError<E> {
    /// The callback failed.
    Io(E),
    /// The callback reported more bytes than the slice it was given.
    Overrun,
}

/// Copy `out.len()` bytes through `scratch` using `read(done, slice)`.
///
/// The callback receives the number of bytes already copied and a slice no larger than `scratch`;
/// it returns the bytes written into that slice. A short read ends the copy, matching a file EOF or
/// a target that cannot fill the requested chunk.
#[allow(dead_code)] // the proxy caller is target-only; the host build omits it
pub(crate) fn copy_chunks<E>(
    out: &mut [u8],
    scratch: &mut [u8],
    mut read: impl FnMut(usize, &mut [u8]) -> Result<usize, E>,
) -> Result<usize, ChunkError<E>> {
    let mut done = 0usize;
    while done < out.len() {
        let capacity = (out.len() - done).min(scratch.len());
        let got = read(done, &mut scratch[..capacity]).map_err(ChunkError::Io)?;
        if got > capacity {
            return Err(ChunkError::Overrun);
        }
        out[done..done + got].copy_from_slice(&scratch[..got]);
        done += got;
        if got < capacity {
            break;
        }
    }
    Ok(done)
}

#[cfg(test)]
#[path = "chunk_ut.rs"]
mod tests;
