// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for the uudecode path.

use uudecode::{decode, safe_output_name};

#[test]
fn decodes_a_real_file_name_and_mode() {
    let encoded = b"begin 750 payload\n&:&5L;&\\*\n`\nend\n";
    let decoded = decode(encoded).unwrap();
    assert_eq!(decoded.name, "payload");
    assert_eq!(decoded.mode, 0o750);
    assert_eq!(decoded.data, b"hello\n");
    assert!(safe_output_name(&decoded.name));
    let path = std::env::temp_dir().join(format!("raven-uudecode-{}", std::process::id()));
    std::fs::write(&path, &decoded.data).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"hello\n");
    let _ = std::fs::remove_file(path);
}
