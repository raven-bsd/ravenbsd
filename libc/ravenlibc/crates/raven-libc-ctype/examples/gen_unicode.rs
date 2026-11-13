// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Generate `src/unicode/tables.rs` from the vendored Unicode 15.1.0 data.
//!
//! `cargo run -p raven-libc-ctype --example gen_unicode -- --ucd xtask/data/ucd-15.1.0 --out <file>`

#[path = "../tests/support/gen.rs"]
mod ucdgen;

extern crate std;
use ::std as host;

fn main() -> host::process::ExitCode {
    let mut ucd = None;
    let mut out = None;
    let mut arguments = host::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--ucd" => ucd = arguments.next(),
            "--out" => out = arguments.next(),
            other => {
                eprintln!("gen_unicode: unknown argument `{other}`");
                return host::process::ExitCode::from(2);
            }
        }
    }
    let (Some(ucd), Some(out)) = (ucd, out) else {
        eprintln!("usage: gen_unicode --ucd <dir> --out <file>");
        return host::process::ExitCode::from(2);
    };
    let generated = match ucdgen::generate(host::path::Path::new(&ucd)) {
        Ok(generated) => generated,
        Err(error) => {
            eprintln!("gen_unicode: {error}");
            return host::process::ExitCode::FAILURE;
        }
    };
    if let Err(error) = host::fs::write(&out, generated.text.as_bytes()) {
        eprintln!("gen_unicode: cannot write {out}: {error}");
        return host::process::ExitCode::FAILURE;
    }
    println!("property blocks: {}", generated.props_blocks);
    println!("property stage2 bytes: {}", generated.props_stage2_bytes);
    println!("case blocks: {}", generated.case_blocks);
    println!("case pairs: {}", generated.case_pairs);
    println!("TABLE_BYTES: {}", generated.table_bytes);
    for (name, hash) in &generated.hashes {
        println!("fnv1a64({name}) = 0x{hash:016x}");
    }
    host::process::ExitCode::SUCCESS
}
