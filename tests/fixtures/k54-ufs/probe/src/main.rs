// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot client for the read-only `fsd-ufs` qtest.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_abi::errno::Errno;
    use raven_proto_fs::{
        Call, CallRequest, CallResponse, DirEntryIter, MAX_PATH, O_RDONLY, O_WRONLY, S_IFMT,
        S_IFREG,
    };
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_scheme::Client;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("fsd-ufs: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("fsd-ufs: probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        direct_checks()?;
        vfs_checks()?;
        Ok(())
    }

    fn direct_checks() -> Result<(), String> {
        let client = Client::new();
        let root = client
            .open("ufs:/", 1)
            .map_err(|error| format!("open ufs:/: {}", error.description()))?;

        let mut byte = [0u8; 1];
        match client.read(root, 1, &mut byte) {
            Err(Errno::EINVAL) => {}
            other => return Err(format!("read with offset 1 returned {other:?}")),
        }
        match client.open("ufs:/ten.txt", 2) {
            Err(Errno::EROFS) => {}
            Err(error) => return Err(format!("write open returned {}", error.description())),
            Ok(_) => return Err("write open succeeded".to_string()),
        }
        match client.open("ufs:/missing", 1) {
            Err(Errno::ENOENT) => {}
            Err(error) => return Err(format!("missing open returned {}", error.description())),
            Ok(_) => return Err("missing open succeeded".to_string()),
        }

        let file = client
            .open("ufs:/ten.txt", 1)
            .map_err(|error| format!("direct open ten.txt: {}", error.description()))?;
        let stat = client
            .fstat(file)
            .map_err(|error| format!("direct fstat ten.txt: {}", error.description()))?;
        if stat.mode & S_IFMT != S_IFREG || stat.size != 10 {
            return Err(format!(
                "direct stat ten.txt mode={:o} size={}",
                stat.mode, stat.size
            ));
        }
        let mut data = [0u8; 16];
        let read = client
            .read(file, 0, &mut data)
            .map_err(|error| format!("direct read ten.txt: {}", error.description()))?;
        if read != 10 || &data[..10] != b"0123456789" {
            return Err(format!("direct read ten.txt returned {read} bytes"));
        }
        let _ = client.close(file);

        let mut directory = [0u8; 4096];
        let read = client
            .read(root, 0, &mut directory)
            .map_err(|error| format!("direct readdir: {}", error.description()))?;
        let mut entries = 0usize;
        for entry in DirEntryIter::new(&directory[..read]) {
            entry.map_err(|_| "decode direct readdir".to_string())?;
            entries += 1;
        }
        if entries < 2 {
            return Err(format!("direct readdir returned {entries} entries"));
        }

        let mut encoded = [0u8; 64];
        let mut response = [0u8; MAX_PATH + 16];
        let relative = CallRequest::new(Call::Readlink { path: b"fastlink" });
        let length = relative
            .encode(&mut encoded)
            .map_err(|_| "encode relative READLINK".to_string())?;
        let length = client
            .call(root, &encoded[..length], &mut response)
            .map_err(|error| format!("relative READLINK: {}", error.description()))?;
        match CallResponse::decode(relative.op(), &response[..length])
            .map_err(|_| "decode relative READLINK".to_string())?
        {
            CallResponse::Readlink { status: 0, target } if target == b"fast-target" => {}
            other => return Err(format!("relative READLINK returned {other:?}")),
        }
        let readlink = CallRequest::new(Call::Readlink { path: b"/fastlink" });
        let length = readlink
            .encode(&mut encoded)
            .map_err(|_| "encode READLINK".to_string())?;
        let length = client
            .call(root, &encoded[..length], &mut response)
            .map_err(|error| format!("READLINK: {}", error.description()))?;
        match CallResponse::decode(readlink.op(), &response[..length])
            .map_err(|_| "decode READLINK".to_string())?
        {
            CallResponse::Readlink { status: 0, target } if target == b"fast-target" => {}
            other => return Err(format!("READLINK returned {other:?}")),
        }

        let statfs = CallRequest::new(Call::Statfs);
        let length = statfs
            .encode(&mut encoded)
            .map_err(|_| "encode STATFS".to_string())?;
        let length = client
            .call(root, &encoded[..length], &mut response)
            .map_err(|error| format!("STATFS: {}", error.description()))?;
        match CallResponse::decode(statfs.op(), &response[..length])
            .map_err(|_| "decode STATFS".to_string())?
        {
            CallResponse::StatFs { status: 0, statfs } if statfs.block_size == 4096 => {}
            other => return Err(format!("STATFS returned {other:?}")),
        }

        let mkdir = CallRequest::new(Call::Mkdir {
            mode: 0o755,
            path: b"/new",
        });
        let length = mkdir
            .encode(&mut encoded)
            .map_err(|_| "encode MKDIR".to_string())?;
        let length = client
            .call(root, &encoded[..length], &mut response)
            .map_err(|error| format!("MKDIR: {}", error.description()))?;
        match CallResponse::decode(mkdir.op(), &response[..length])
            .map_err(|_| "decode MKDIR".to_string())?
        {
            CallResponse::Status(status) if status == -(Errno::EROFS.0 as i32) => {}
            other => return Err(format!("MKDIR returned {other:?}")),
        }
        let _ = client.close(root);
        raven_rt::println!("fsd-ufs: probe errors ok");
        Ok(())
    }

    fn vfs_checks() -> Result<(), String> {
        fs::mount("/ufs", "ufs:").map_err(|error| format!("mount: {}", error.description()))?;
        raven_rt::println!("fsd-ufs: probe mount ok");

        let ten = fs::read_to_vec("file:/ufs/ten.txt")
            .map_err(|error| format!("read ten.txt: {}", error.description()))?;
        if ten.as_slice() != b"0123456789" {
            return Err(format!("ten.txt has {} bytes", ten.len()));
        }
        raven_rt::println!("fsd-ufs: probe ten.txt=0123456789");

        let one = fs::read_to_vec("file:/ufs/dir1/one.txt")
            .map_err(|error| format!("read one.txt: {}", error.description()))?;
        if one.as_slice() != b"one\n" {
            return Err(format!("one.txt has {} bytes", one.len()));
        }
        raven_rt::println!("fsd-ufs: probe one.txt=one");

        let entries = fs::read_dir("file:/ufs")
            .map_err(|error| format!("readdir: {}", error.description()))?;
        for name in ["ten.txt", "dir1", "fastlink", "slowlink"] {
            if !entries.iter().any(|entry| entry.name == name) {
                return Err(format!("listing does not contain {name}"));
            }
        }
        raven_rt::println!("fsd-ufs: probe listing=dir1,fastlink,slowlink,ten.txt");

        let target = fs::readlink("/ufs/fastlink")
            .map_err(|error| format!("readlink: {}", error.description()))?;
        if target != "fast-target" {
            return Err(format!("readlink returned {target}"));
        }
        raven_rt::println!("fsd-ufs: probe readlink=fast-target");

        let stat = fs::stat("file:/ufs/empty")
            .map_err(|error| format!("stat: {}", error.description()))?;
        if stat.size != 0 || stat.mode & S_IFMT != S_IFREG {
            return Err("empty has the wrong stat".to_string());
        }
        raven_rt::println!("fsd-ufs: probe stat=empty,size=0");

        match fs::open("file:/ufs/ten.txt", O_WRONLY) {
            Err(Errno::EROFS) => {}
            Err(error) => {
                return Err(format!("vfs write open returned {}", error.description()));
            }
            Ok(_) => return Err("vfs write open succeeded".to_string()),
        }
        if fs::open("file:/ufs/missing", O_RDONLY).is_ok() {
            return Err("vfs missing open succeeded".to_string());
        }
        raven_rt::println!("fsd-ufs: probe vfs-errors ok");

        fs::umount("/ufs").map_err(|error| format!("umount: {}", error.description()))?;
        raven_rt::println!("fsd-ufs: probe umount ok");
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
