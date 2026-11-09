// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::io::Write;

fn main() {
    let dir = std::env::args().nth(1).expect("usage: files <dir>");
    fs::create_dir_all(format!("{dir}/sub")).unwrap();
    let path = format!("{dir}/sub/a.txt");
    let mut f = fs::File::create(&path).unwrap();
    f.write_all(b"hello\n").unwrap();
    f.sync_all().unwrap();
    drop(f);
    fs::rename(&path, format!("{dir}/sub/b.txt")).unwrap();
    let md = fs::metadata(format!("{dir}/sub/b.txt")).unwrap();
    assert_eq!(md.len(), 6);
    let n = fs::read_dir(format!("{dir}/sub")).unwrap().count();
    assert_eq!(n, 1);
    fs::remove_file(format!("{dir}/sub/b.txt")).unwrap();
    fs::remove_dir(format!("{dir}/sub")).unwrap();
    println!("files ok");
}
