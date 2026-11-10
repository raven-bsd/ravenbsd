#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Minimal UFS1/UFS2 reader used to check a golden image against a manifest.

usage: check-ufs.py <image> <manifest>

It reads the superblock at SBLOCK_UFS1 (8192), walks the tree from inode 2,
maps direct, single, double and triple indirect blocks, and compares every
path with the manifest produced by tests/ufs-vectors/gen-manifest.sh.
This is a test oracle, not a mount implementation: it is deliberately
small, single-threaded and free of OS interfaces.
"""

import hashlib
import struct
import sys

SBLOCK_UFS1 = 8192
FS_UFS1_MAGIC = 0x011954
FS_UFS2_MAGIC = 0x19540119
UFS_ROOTINO = 2

DT_DIR = 4
DT_REG = 8
DT_LNK = 10
DT_FIFO = 1
DT_CHR = 2
DT_BLK = 6
DT_SOCK = 12

TYPE_BY_MODE = {
    0o100000: "file",
    0o040000: "dir",
    0o120000: "symlink",
    0o020000: "char",
    0o060000: "block",
    0o010000: "fifo",
    0o140000: "socket",
}


class Image:
    def __init__(self, path):
        with open(path, "rb") as fh:
            self.d = fh.read()
        base = SBLOCK_UFS1
        magic = self.u32(base + 0x55C)
        if magic == FS_UFS1_MAGIC:
            self.ufs2 = False
            self.inode_size = 128
        elif magic == FS_UFS2_MAGIC:
            self.ufs2 = True
            self.inode_size = 256
        else:
            raise SystemExit("bad or unsupported superblock magic 0x%08x" % magic)
        self.bsize = self.i32(base + 48)
        self.fsize = self.i32(base + 52)
        self.frag = self.i32(base + 56)
        self.ipg = self.u32(base + 184)
        self.fpg = self.i32(base + 188)
        self.inopb = self.u32(base + 120)
        self.iblkno = self.i32(base + 16)
        self.cgoffset = self.i32(base + 24)
        self.cgmask = self.i32(base + 28)
        if self.bsize <= 0 or self.fsize <= 0 or self.bsize % self.fsize:
            raise SystemExit("bad block/fragment sizes")
        if self.bsize // self.fsize != self.frag:
            raise SystemExit("inconsistent fragments per block")

    def u32(self, off):
        return struct.unpack_from("<I", self.d, off)[0]

    def i32(self, off):
        return struct.unpack_from("<i", self.d, off)[0]

    def inode(self, ino):
        cg = ino // self.ipg
        if self.ufs2:
            cgstart = self.fpg * cg
        else:
            cgstart = self.fpg * cg + self.cgoffset * (cg & ~self.cgmask)
        blk = ((ino % self.ipg) // self.inopb) * self.frag
        fsba = cgstart + self.iblkno + blk
        fsbo = (ino % self.ipg) % self.inopb
        off = fsba * self.fsize + fsbo * self.inode_size
        raw = self.d[off:off + self.inode_size]
        if len(raw) != self.inode_size:
            raise SystemExit("inode %d outside image" % ino)
        mode = struct.unpack_from("<H", raw, 0)[0]
        nlink = struct.unpack_from("<h", raw, 2)[0]
        if self.ufs2:
            uid, gid = struct.unpack_from("<II", raw, 4)
            size, = struct.unpack_from("<Q", raw, 16)
            db = struct.unpack_from("<12Q", raw, 112)
            ib = struct.unpack_from("<3Q", raw, 208)
        else:
            raw_size, = struct.unpack_from("<Q", raw, 8)
            size = raw_size
            uid, gid = struct.unpack_from("<II", raw, 112)
            db = struct.unpack_from("<12I", raw, 40)
            ib = struct.unpack_from("<3I", raw, 88)
        return {
            "mode": mode, "nlink": nlink, "uid": uid, "gid": gid,
            "size": size, "db": list(db), "ib": list(ib), "raw": raw,
        }

    def block(self, fragno):
        off = fragno * self.fsize
        return self.d[off:off + self.bsize]

    def ptr(self, fragno, index):
        if fragno == 0:
            return 0
        block = self.block(fragno)
        fmt = "<Q" if self.ufs2 else "<I"
        width = 8 if self.ufs2 else 4
        return struct.unpack_from(fmt, block, index * width)[0]

    def bmap(self, ip, lbn):
        nindir = self.bsize // (8 if self.ufs2 else 4)
        if lbn < 12:
            return ip["db"][lbn]
        lbn -= 12
        if lbn < nindir:
            return self.ptr(ip["ib"][0], lbn)
        lbn -= nindir
        if lbn < nindir * nindir:
            return self.ptr(self.ptr(ip["ib"][1], lbn // nindir), lbn % nindir)
        lbn -= nindir * nindir
        if lbn < nindir ** 3:
            first = self.ptr(ip["ib"][2], lbn // (nindir * nindir))
            second = self.ptr(first, (lbn // nindir) % nindir)
            return self.ptr(second, lbn % nindir)
        raise SystemExit("logical block %d beyond triple indirect" % lbn)

    def readfile(self, ip):
        size = ip["size"]
        out = bytearray()
        nblocks = (size + self.bsize - 1) // self.bsize
        for lbn in range(nblocks):
            fragno = self.bmap(ip, lbn)
            if fragno == 0:
                out += b"\0" * self.bsize
            else:
                chunk = self.block(fragno)
                out += chunk.ljust(self.bsize, b"\0")
        return bytes(out[:size])

    def link_target(self, ip):
        if ip["size"] <= (120 if self.ufs2 else 60):
            return ip["raw"][(112 if self.ufs2 else 40):][:ip["size"]]
        return self.readfile(ip)

    def dirents(self, ip):
        data = self.readfile(ip)
        off = 0
        while off + 8 <= len(data):
            fileno, reclen, dtype, namlen = struct.unpack_from("<IHBB", data, off)
            if reclen < 8 or off + reclen > len(data):
                raise SystemExit("bad directory record at %d" % off)
            if fileno != 0:
                name = data[off + 8:off + 8 + namlen].decode("utf-8", "replace")
                yield name, fileno, dtype
            off += reclen


def image_tree(img):
    """Return path -> (type, mode, uid, gid, size, sha256, link, nlink)."""
    out = {}
    seen_dirs = set()

    def walk(ino, path):
        ip = img.inode(ino)
        ftype = TYPE_BY_MODE.get(ip["mode"] & 0o170000)
        if ftype is None:
            raise SystemExit("unknown inode type for %s" % path)
        mode = "%04o" % (ip["mode"] & 0o7777)
        if ftype == "file":
            data = img.readfile(ip)
            size = str(len(data))
            sha = hashlib.sha256(data).hexdigest()
            link = "-"
        elif ftype == "symlink":
            target = img.link_target(ip)
            size = str(len(target))
            sha = "-"
            link = target.decode("utf-8", "replace")
        elif ftype == "dir":
            size = "-"
            sha = "-"
            link = "-"
        else:
            size = "0"
            sha = "-"
            link = "-"
        out[path] = (ftype, mode, str(ip["uid"]), str(ip["gid"]),
                     size, sha, link, str(ip["nlink"]))
        if ftype != "dir":
            return
        if ino in seen_dirs:
            return
        seen_dirs.add(ino)
        for name, child, _dtype in img.dirents(ip):
            if name in (".", ".."):
                continue
            base = "/" if path == "/" else path + "/"
            walk(child, base + name)

    walk(UFS_ROOTINO, "/")
    return out


def load_manifest(path):
    out = {}
    with open(path, "r", encoding="utf-8") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            if not line:
                continue
            fields = line.split("|")
            if len(fields) != 9:
                raise SystemExit("%s:%d: expected 9 fields" % (path, lineno))
            out[fields[0]] = tuple(fields[1:])
    return out


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: check-ufs.py <image> <manifest>")
    img = Image(sys.argv[1])
    got = image_tree(img)
    want = load_manifest(sys.argv[2])

    problems = []
    for path in sorted(set(got) | set(want)):
        if path not in got:
            problems.append("missing in image: %s" % path)
        elif path not in want:
            problems.append("extra in image: %s" % path)
        elif got[path] != want[path]:
            problems.append("%s: image %r != manifest %r" % (path, got[path], want[path]))

    if problems:
        for line in problems[:40]:
            print("FAIL:", line)
        print("FAIL: %d paths checked, %d problems" % (len(want), len(problems)))
        return 1
    print("OK: %d paths match (%s, bsize=%d, fsize=%d)" % (
        len(want), "UFS2" if img.ufs2 else "UFS1", img.bsize, img.fsize))
    return 0


if __name__ == "__main__":
    sys.exit(main())
