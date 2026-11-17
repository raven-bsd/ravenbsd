// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `trace`: map the shared span sink and print the recorded request tree.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![allow(unsafe_code)] // Mapping the sink and constructing the ring over the fixed address.

#[cfg(target_os = "none")]
mod program {
    use raven_abi::types::{Handle, prot};
    use raven_rt::entry;
    use raven_rt::print;
    use raven_rt::println;
    use raven_trace_core::{RingHeader, SpanRecord, SpanRing};
    use trace::{SINK_ADDR, SINK_PAGES, format_tree, read_spans};

    /// The sink memory object transferred to this process before its first thread.
    const SINK_HANDLE: Handle = Handle::new(3, 0);
    /// Page-aligned length of the sink mapping.
    const SINK_MAP_LEN: usize = SINK_PAGES * 4096;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        if raven_rt::sys::map_object_in(
            Handle::SELF_PROC,
            SINK_HANDLE,
            0,
            SINK_ADDR,
            SINK_MAP_LEN,
            prot::READ | prot::WRITE,
            1, // MAP_FIXED
        )
        .is_err()
        {
            println!("trace: failed sink map");
            return 1;
        }

        // SAFETY: the kernel mapped `SINK_BYTES` of the sink at `SINK_ADDR`; the layout is the
        // shared `RingHeader` followed by `CAPACITY` records.
        let (header, records) = unsafe { sink_view() };
        let ring = match SpanRing::new(header, records) {
            Ok(ring) => ring,
            Err(_) => {
                println!("trace: failed sink header");
                return 1;
            }
        };
        let (spans, gaps) = read_spans(&ring);
        print!("{}", format_tree(&spans, gaps));
        0
    }

    /// SAFETY: the caller must have mapped the sink at [`SINK_ADDR`].
    unsafe fn sink_view() -> (&'static RingHeader, &'static [SpanRecord]) {
        let header = SINK_ADDR as *const RingHeader;
        let records = (SINK_ADDR + core::mem::size_of::<RingHeader>()) as *const SpanRecord;
        // SAFETY: the sink layout is the header followed by the record array; the mapping lives
        // for the process.
        unsafe {
            (
                &*header,
                core::slice::from_raw_parts(records, trace::CAPACITY as usize),
            )
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
