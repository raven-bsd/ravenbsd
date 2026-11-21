// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Asserts the crate is first-party BSD-3: no Redox/MIT-derived source remains.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read source directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn no_redox_derived_source_remains() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    assert!(
        !root.join("LICENSE-MIT-REDOX").exists(),
        "the Redox MIT licence file remains"
    );

    let mut sources = Vec::new();
    rust_sources(&root.join("src"), &mut sources);
    assert!(sources.len() >= 10, "source walk found too few files");

    for path in sources {
        let text = fs::read_to_string(&path).expect("source is UTF-8");
        let header: String = text.lines().take(5).collect::<Vec<_>>().join("\n");
        assert!(
            header.contains("SPDX-License-Identifier: BSD-3-Clause"),
            "{} is missing the BSD-3 SPDX line",
            path.display()
        );
        assert!(
            header.contains("Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors"),
            "{} is missing the RavenBSD copyright line",
            path.display()
        );
        assert!(
            !text.contains("Redox") && !text.contains("redox"),
            "{} still names Redox",
            path.display()
        );
        assert!(
            !text.contains("Jeremy Soller") && !text.contains("SPDX-License-Identifier: MIT"),
            "{} still carries the Redox/MIT header",
            path.display()
        );
    }
}
