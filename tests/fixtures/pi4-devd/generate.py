# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Generate the minimal Pi 4 platform-bus fixtures used by `devd` and the `pi4-devd` qtest.

The trees are our own constructions. Only numeric facts (addresses, sizes, interrupt numbers and
the compatible strings) are transcribed from the Raspberry Pi 4 firmware DTB and the B42 spike; no
donor DTS text or comment is copied. Run from the repository root:

    python3 tests/fixtures/pi4-devd/generate.py
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

OUT = Path(__file__).resolve().parent


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
        17,  # version
        16,  # last compatible version
        0,   # boot CPU
        len(string_blob),
        len(structure),
    )
    return header + bytes(RSVMAP_BYTES) + bytes(structure) + bytes(string_blob)


GIC_PHANDLE = 1


def gic_node() -> Node:
    return Node(
        "interrupt-controller@ff841000",
        [
            ("compatible", text_list(["arm,gic-400"])),
            ("reg", be32([0x0, 0xFF84_1000, 0x0, 0x1000])),
            ("interrupt-controller", b""),
            ("#interrupt-cells", be32([3])),
            ("phandle", be32([GIC_PHANDLE])),
        ],
    )


def soc_node() -> Node:
    return Node(
        "soc",
        [
            ("compatible", text_list(["simple-bus"])),
            ("#address-cells", be32([1])),
            ("#size-cells", be32([1])),
            ("ranges", be32([0x7E00_0000, 0x0, 0xFE00_0000, 0x0180_0000])),
        ],
        [
            Node(
                "pwm@7e20c800",
                [
                    ("compatible", text_list(["brcm,bcm2835-pwm"])),
                    ("reg", be32([0x7E20_C800, 0x28])),
                    ("status", text("okay")),
                ],
            ),
            Node(
                "rng@7e104000",
                [
                    ("compatible", text_list(["brcm,bcm2711-rng200"])),
                    ("reg", be32([0x7E10_4000, 0x28])),
                    ("status", text("okay")),
                ],
            ),
        ],
    )


def scb_node() -> Node:
    return Node(
        "scb",
        [
            ("compatible", text_list(["simple-bus"])),
            ("#address-cells", be32([2])),
            ("#size-cells", be32([2])),
            ("ranges", be32([0x0, 0x7D50_0000, 0x0, 0xFD50_0000, 0x0, 0x0110_0000])),
        ],
        [
            Node(
                "ethernet@7d580000",
                [
                    ("compatible", text_list(["brcm,bcm2711-genet-v5"])),
                    ("reg", be32([0x0, 0x7D58_0000, 0x0, 0x0001_0000])),
                    ("interrupt-parent", be32([GIC_PHANDLE])),
                    ("interrupts", be32([0, 157, 4, 0, 158, 4])),
                    ("status", text("okay")),
                ],
            ),
        ],
    )


def emmc2bus_node(bad_ranges: bool) -> Node:
    size = 0x0010_0000 if bad_ranges else 0x0180_0000
    return Node(
        "emmc2bus",
        [
            ("compatible", text_list(["simple-bus"])),
            ("#address-cells", be32([2])),
            ("#size-cells", be32([1])),
            ("ranges", be32([0x0, 0x7E00_0000, 0x0, 0xFE00_0000, size])),
            ("dma-ranges", be32([0x0, 0xC000_0000, 0x0, 0x0, 0x4000_0000])),
        ],
        [
            Node(
                "mmc@7e340000",
                [
                    ("compatible", text_list(["brcm,bcm2711-emmc2"])),
                    ("reg", be32([0x0, 0x7E34_0000, 0x100])),
                    ("interrupt-parent", be32([GIC_PHANDLE])),
                    ("interrupts", be32([0, 126, 4])),
                    ("status", text("okay")),
                ],
            ),
        ],
    )


def tree(bad_ranges: bool = False) -> Node:
    return Node(
        "",
        [
            ("compatible", text_list(["raspberrypi,4-model-b", "brcm,bcm2711"])),
            ("model", text("Raspberry Pi 4 Model B")),
            ("#address-cells", be32([2])),
            ("#size-cells", be32([2])),
        ],
        [soc_node(), scb_node(), emmc2bus_node(bad_ranges), gic_node()],
    )


def dts(bad_ranges: bool) -> str:
    size = 0x100000 if bad_ranges else 0x1800000
    name = "pi4-devd-bad-ranges" if bad_ranges else "pi4-devd"
    return f"""// SPDX-License-Identifier: BSD-3-Clause
// Generated by tests/fixtures/pi4-devd/generate.py; fixture {name}.
/dts-v1/;

/ {{
    compatible = "raspberrypi,4-model-b", "brcm,bcm2711";
    model = "Raspberry Pi 4 Model B";
    #address-cells = <0x2>;
    #size-cells = <0x2>;
    soc {{
        compatible = "simple-bus";
        #address-cells = <0x1>;
        #size-cells = <0x1>;
        ranges = <0x7e000000 0x0 0xfe000000 0x01800000>;
        pwm@7e20c800 {{
            compatible = "brcm,bcm2835-pwm";
            reg = <0x7e20c800 0x28>;
            status = "okay";
        }};
        rng@7e104000 {{
            compatible = "brcm,bcm2711-rng200";
            reg = <0x7e104000 0x28>;
            status = "okay";
        }};
    }};
    scb {{
        compatible = "simple-bus";
        #address-cells = <0x2>;
        #size-cells = <0x2>;
        ranges = <0x0 0x7d500000 0x0 0xfd500000 0x0 0x1100000>;
        ethernet@7d580000 {{
            compatible = "brcm,bcm2711-genet-v5";
            reg = <0x0 0x7d580000 0x0 0x10000>;
            interrupt-parent = <0x1>;
            interrupts = <0x0 0x9d 0x4 0x0 0x9e 0x4>;
            status = "okay";
        }};
    }};
    emmc2bus {{
        compatible = "simple-bus";
        #address-cells = <0x2>;
        #size-cells = <0x1>;
        ranges = <0x0 0x7e000000 0x0 0xfe000000 0x{size:x}>;
        dma-ranges = <0x0 0xc0000000 0x0 0x0 0x40000000>;
        mmc@7e340000 {{
            compatible = "brcm,bcm2711-emmc2";
            reg = <0x0 0x7e340000 0x100>;
            interrupt-parent = <0x1>;
            interrupts = <0x0 0x7e 0x4>;
            status = "okay";
        }};
    }};
    interrupt-controller@ff841000 {{
        compatible = "arm,gic-400";
        reg = <0x0 0xff841000 0x0 0x1000>;
        interrupt-controller;
        #interrupt-cells = <0x3>;
        phandle = <0x1>;
    }};
}};
"""


def write(name: str, bad_ranges: bool = False) -> None:
    binary = encode_tree(tree(bad_ranges))
    (OUT / f"{name}.dtb").write_bytes(binary)
    (OUT / f"{name}.dts").write_text(dts(bad_ranges))


def main() -> None:
    write("pi4-devd")
    write("pi4-devd-bad-ranges", bad_ranges=True)


if __name__ == "__main__":
    main()
