#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Generate the QEMU `virt` platform tree used by the aarch64 `devd dtb launch` path.

The tree is a construction of this fixture, not a copy of a donor DTS. It carries the two nodes
the launch needs: the GIC (interrupt parent) and the first virtio-mmio slot QEMU `virt` exposes
for `-device virtio-net-device` (0x0a000000, 0x200, SPI 16). Run from the repository root:

    python3 tests/fixtures/virt-dtb/generate.py
"""

from __future__ import annotations

import struct
from pathlib import Path

FDT_MAGIC = 0xD00D_FEED
FDT_BEGIN_NODE = 1
FDT_END_NODE = 2
FDT_PROP = 3
FDT_END = 9
HEADER_BYTES = 40
RSVMAP_BYTES = 16

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / "build/fixtures/virt-dtb/virt.dtb"
GIC_PHANDLE = 1


class Node:
    def __init__(self, name: str, props=None, children=None):
        self.name = name
        self.props = props or []
        self.children = children or []


def be32(values) -> bytes:
    return b"".join(struct.pack(">I", value & 0xFFFF_FFFF) for value in values)


def text(value: str) -> bytes:
    return value.encode() + b"\0"


def text_list(values) -> bytes:
    return b"".join(text(value) for value in values)


def pad4(block: bytearray) -> None:
    while len(block) % 4:
        block.append(0)


def encode_node(node: Node, strings: dict[str, int], string_blob: bytearray,
                structure: bytearray) -> None:
    structure += struct.pack(">I", FDT_BEGIN_NODE)
    structure += node.name.encode() + b"\0"
    pad4(structure)
    for name, value in node.props:
        if name not in strings:
            strings[name] = len(string_blob)
            string_blob += name.encode() + b"\0"
        structure += struct.pack(">III", FDT_PROP, len(value), strings[name])
        structure += value
        pad4(structure)
    for child in node.children:
        encode_node(child, strings, string_blob, structure)
    structure += struct.pack(">I", FDT_END_NODE)


def encode_tree(root: Node) -> bytes:
    strings: dict[str, int] = {}
    string_blob = bytearray()
    structure = bytearray()
    encode_node(root, strings, string_blob, structure)
    structure += struct.pack(">I", FDT_END)
    struct_offset = HEADER_BYTES + RSVMAP_BYTES
    strings_offset = struct_offset + len(structure)
    total = strings_offset + len(string_blob)
    header = struct.pack(
        ">10I",
        FDT_MAGIC,
        total,
        struct_offset,
        strings_offset,
        HEADER_BYTES,
        17,
        16,
        0,
        len(string_blob),
        len(structure),
    )
    return header + bytes(RSVMAP_BYTES) + bytes(structure) + bytes(string_blob)


def gic_node() -> Node:
    return Node(
        "intc@8000000",
        [
            ("compatible", text_list(["arm,cortex-a15-gic", "arm,gic-400"])),
            ("reg", be32([0x0, 0x0800_0000, 0x0, 0x1_0000, 0x0, 0x0801_0000, 0x0, 0x1_0000])),
            ("interrupt-controller", b""),
            ("#interrupt-cells", be32([3])),
            ("phandle", be32([GIC_PHANDLE])),
        ],
    )


def virtio_node() -> Node:
    return Node(
        "virtio_mmio@a000000",
        [
            ("compatible", text_list(["virtio,mmio"])),
            ("reg", be32([0x0, 0x0A00_0000, 0x0, 0x200])),
            ("interrupt-parent", be32([GIC_PHANDLE])),
            ("interrupts", be32([0x0, 16, 1])),
            ("status", text("okay")),
        ],
    )


def main() -> int:
    root = Node(
        "",
        [
            ("#address-cells", be32([2])),
            ("#size-cells", be32([2])),
            ("compatible", text_list(["raven,qemu-virt"])),
            ("model", text("RavenBSD QEMU virt")),
            ("interrupt-parent", be32([GIC_PHANDLE])),
        ],
        [gic_node(), virtio_node()],
    )
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(encode_tree(root))
    print(f"virt-dtb: wrote {OUT} ({OUT.stat().st_size} bytes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
