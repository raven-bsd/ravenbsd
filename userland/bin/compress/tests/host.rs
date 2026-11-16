// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host interoperability check for the .Z stream.

use std::io::Write;
use std::process::{Command, Stdio};

use compress::encode;

#[test]
fn gzip_accepts_our_stream() {
    if Command::new("gzip").arg("--version").output().is_err() {
        return;
    }
    let data = b"the quick brown fox\njumps over the lazy dog\n";
    let compressed = encode(data);
    let mut child = Command::new("gzip")
        .arg("-dc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("gzip");
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(&compressed)
        .expect("write");
    let output = child.wait_with_output().expect("gzip output");
    assert!(output.status.success(), "gzip rejected .Z: {output:?}");
    assert_eq!(output.stdout, data);
}
