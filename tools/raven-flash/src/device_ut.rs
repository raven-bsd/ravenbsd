// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const GIB: u64 = 1024 * 1024 * 1024;

struct Fixture {
    dir: PathBuf,
    config: FilterConfig,
    image: ImageInfo,
}

impl Fixture {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "raven-flash-dev-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("sys/block")).unwrap();
        std::fs::create_dir_all(dir.join("dev")).unwrap();
        std::fs::create_dir_all(dir.join("images")).unwrap();
        std::fs::write(dir.join("images/disk.img"), vec![0u8; 16]).unwrap();
        let image = ImageInfo {
            path: dir.join("images/disk.img"),
            size: 8 * GIB,
            sha256: "0".repeat(64),
            build_id: Some("test".to_string()),
        };
        let mut fixture = Self {
            dir,
            config: FilterConfig {
                sys_root: PathBuf::new(),
                mounts: PathBuf::new(),
                dev_root: PathBuf::new(),
                root_device: Some("sda".to_string()),
            },
            image,
        };
        fixture.config.sys_root = fixture.dir.join("sys");
        fixture.config.mounts = fixture.dir.join("mounts");
        fixture.config.dev_root = fixture.dir.join("dev");
        std::fs::write(
            fixture.dir.join("mounts"),
            format!(
                "/dev/sda1 / ext4 rw 0 0\n\
                 /dev/sdd1 /mnt/card vfat rw 0 0\n\
                 /dev/sdg1 {} ext4 rw 0 0\n",
                fixture.dir.join("images").display()
            ),
        )
        .unwrap();
        fixture
    }

    fn disk(&self, name: &str, size_gib: u64, removable: bool, transport: &str) {
        let sys = self.dir.join("sys/block").join(name);
        std::fs::create_dir_all(sys.join("device")).unwrap();
        std::fs::create_dir_all(sys.join("holders")).unwrap();
        std::fs::write(sys.join("size"), format!("{}", size_gib * GIB / 512)).unwrap();
        std::fs::write(sys.join("removable"), if removable { "1" } else { "0" }).unwrap();
        std::fs::write(sys.join("device/uevent"), format!("DRIVER={transport}\n")).unwrap();
        std::fs::write(sys.join("device/model"), format!("{name} model\n")).unwrap();
        std::fs::write(sys.join("device/serial"), format!("{name}-serial\n")).unwrap();
        std::fs::write(self.dir.join("dev").join(name), b"").unwrap();
    }

    fn holder(&self, name: &str, holder: &str) {
        let path = self.dir.join("sys/block").join(name).join("holders");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join(holder), b"").unwrap();
    }

    fn candidates(&self) -> Vec<Candidate> {
        enumerate(&self.config, Some(&self.image)).unwrap()
    }

    fn names(&self, candidates: &[Candidate], safe: bool) -> Vec<String> {
        candidates
            .iter()
            .filter(|candidate| (candidate.reject.is_none()) == safe)
            .map(|candidate| candidate.device.name.clone())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn filter_keeps_only_removable_usb_mmc_targets() {
    let fixture = Fixture::new();
    fixture.disk("sda", 512, false, "ata");
    fixture.disk("sdb", 64, true, "usb");
    fixture.disk("sdc", 4, true, "usb");
    fixture.disk("sdd", 64, true, "usb");
    fixture.disk("mmcblk0", 32, true, "mmc");
    fixture.disk("loop0", 64, false, "loop");
    fixture.disk("dm-0", 64, false, "lvm");
    fixture.disk("md0", 64, false, "md");
    fixture.disk("sde", 64, true, "usb");
    fixture.disk("sdf", 64, true, "usb");
    fixture.disk("sdg", 64, true, "usb");
    fixture.holder("sde", "dm-0");
    let _ = std::fs::remove_file(fixture.dir.join("dev").join("sdf"));
    std::os::unix::fs::symlink("sdb", fixture.dir.join("dev").join("sdf")).unwrap();

    let candidates = fixture.candidates();
    let mut safe = fixture.names(&candidates, true);
    safe.sort();
    assert_eq!(safe, vec!["mmcblk0", "sdb", "sdd"], "{candidates:#?}");

    for name in ["sda", "sdc", "loop0", "dm-0", "md0", "sde", "sdf", "sdg"] {
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.device.name == name)
            .unwrap();
        assert!(candidate.reject.is_some(), "{name} must be refused");
    }
}

#[test]
fn reject_reasons_name_the_failure() {
    let fixture = Fixture::new();
    fixture.disk("sda", 512, false, "ata");
    fixture.disk("sdb", 64, true, "usb");
    fixture.disk("sdc", 4, true, "usb");
    fixture.disk("sdd", 64, true, "usb");
    fixture.disk("sde", 64, true, "usb");
    fixture.holder("sde", "md0");
    let candidates = fixture.candidates();
    let reason = |name: &str| {
        candidates
            .iter()
            .find(|candidate| candidate.device.name == name)
            .and_then(|candidate| candidate.reject.clone())
            .unwrap()
    };
    assert!(reason("sda").contains("root"));
    assert!(reason("sdc").contains("does not fit"));
    assert!(reason("sde").contains("holders"));
    assert!(
        candidates
            .iter()
            .find(|candidate| candidate.device.name == "sdd")
            .unwrap()
            .device
            .mounted
            .iter()
            .any(|mount| mount.target == "/mnt/card")
    );
}

#[test]
fn parse_mounts_ignores_non_device_sources() {
    let mounts = parse_mounts(
        "/dev/sdb1 /mnt/card vfat rw 0 0\n\
         proc /proc proc rw 0 0\n\
         /dev/mmcblk0p1 /run/media/card ext4 rw 0 0\n",
    );
    assert_eq!(mounts.len(), 2);
    assert_eq!(mounts[0].source, "/dev/sdb1");
    assert_eq!(mounts[1].source, "/dev/mmcblk0p1");
}
