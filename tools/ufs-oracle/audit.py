#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Per-file licence audit of the NetBSD UFS donor.

Reads the donor tree read-only and writes, into --out:

  licence-audit.tsv    one row per file (tab separated)
  LICENCE-AUDIT.md     the same data as a readable document
  NOTICE-UFS.txt       distinct copyright statements for NOTICE

The audit is an engineering inventory, not legal advice.  It classifies the
notice text found in the first 400 lines of each file; it does not read
included headers recursively and it does not decide whether a clause is
enforceable.
"""

import argparse
import glob
import os
import re
import sys

SPDX = (
    "<!-- SPDX-License-Identifier: BSD-3-Clause -->\n"
    "<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n"
)

CORE_FILES = {"sys/kern/vfs_wapbl.c", "sys/sys/wapbl.h"}
CORE_DIRS = ("sys/ufs/ffs/", "sys/ufs/ufs/")
TOOL_DIRS = ("sbin/fsck_ffs/", "sbin/newfs/", "usr.sbin/makefs/")


def header_lines(path, limit=400):
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            return fh.read().splitlines()[:limit]
    except OSError as exc:
        sys.exit("audit: cannot read %s: %s" % (path, exc))


def clean_comment(raw):
    """Strip the comment decoration from one line, keep the words."""
    s = raw.strip()
    if s.startswith("/*"):
        s = s[2:].lstrip("!- ")
    elif s.startswith("*"):
        s = s[1:].lstrip()
    elif s.startswith("//"):
        s = s[2:].lstrip()
    return s.rstrip()


def is_comment(raw):
    s = raw.lstrip()
    return s.startswith("/*") or s.startswith("*") or s.startswith("//")


def is_continuation(raw):
    """NetBSD style: ' *\\tThe Regents ...' or ' *    The ...'."""
    if not is_comment(raw):
        return False
    return re.match(r"^\s*\*\s*(\t| {4,})\S", raw) is not None


def continues_statement(previous, raw):
    """True when a comment line is a wrapped holder/notice continuation."""
    if not is_comment(raw):
        return False
    text = clean_comment(raw)
    if not text:
        return False
    if re.match(r"^(Copyright|Redistribution|This software|This code|Written by|"
                r"Further information|All rights reserved|Redistributions)",
                text, re.I):
        return False
    if is_continuation(raw):
        return True
    # A line that does not end in a full stop is clearly wrapped.
    return not previous.rstrip().endswith(".")


def copyright_statements(lines):
    out = []
    i = 0
    while i < len(lines):
        raw = lines[i]
        if is_comment(raw) and "Copyright" in raw:
            stmt = [clean_comment(raw)]
            j = i + 1
            while j < len(lines) and "Copyright" not in lines[j] and continues_statement(stmt[-1], lines[j]):
                stmt.append(clean_comment(lines[j]))
                j += 1
            out.append(" ".join(part for part in stmt if part))
            i = j
        else:
            i += 1
    return out


def advertising_ack(lines):
    """Return the acknowledgement text of a 4-clause notice, or ''."""
    for i, raw in enumerate(lines):
        if "must display the following acknowledgement" not in raw:
            continue
        parts = []
        for nxt in lines[i + 1:i + 6]:
            text = clean_comment(nxt)
            if not text or re.match(r"^\d+\.", text):
                break
            parts.append(text)
            if text.endswith("."):
                break
        return " ".join(parts)
    return ""


def classify(lines):
    head = "\n".join(lines)
    adv = "All advertising materials" in head
    source_clause = "Redistributions of source code must retain" in head
    binary_clause = "Redistributions in binary form must reproduce" in head
    endorse = (
        re.search(r"Neither the name of .*? may be used to endorse", head, re.S)
        or re.search(r"The name of the author may not be used to endorse", head, re.S)
        or re.search(r"may not be used to endorse or promote products derived\s+from\s+this\s+software", head, re.S)
    )
    isc = "Permission to use, copy, modify" in head and "with or without fee" in head

    if adv:
        licence = "BSD-4-Clause"
    elif endorse:
        licence = "BSD-3-Clause"
    elif source_clause and binary_clause:
        licence = "BSD-2-Clause"
    elif isc:
        licence = "ISC"
    elif "Redistribution and use in source and binary forms" in head:
        licence = "other"
    else:
        licence = "NOASSERTION"
    return licence, adv, bool(endorse), binary_clause


def group_of(rel):
    if rel in CORE_FILES or rel.startswith(CORE_DIRS):
        return "ufs-core"
    if rel.startswith("sbin/fsck_ffs/"):
        return "fsck_ffs"
    if rel.startswith("sbin/newfs/"):
        return "newfs"
    if rel.startswith("usr.sbin/makefs/"):
        return "makefs"
    return "other"


def intended_use(rel, licence):
    # BSD-4-Clause is not on the project allow-list (architecture/12 and the
    # minion handbook): read it, do not copy it.
    if licence == "BSD-4-Clause":
        return "reference-only"
    if licence in ("BSD-2-Clause", "BSD-3-Clause", "ISC"):
        # Only the portable core is recommended for porting; the newfs,
        # makefs and fsck_ffs sources are oracle/reference code.
        return "port" if group_of(rel) == "ufs-core" else "reference-only"
    return "rewrite-from-definitions"


def notes_for(rel, lines, licence, endorse, binary_clause):
    notes = []
    statements = copyright_statements(lines)
    if not statements:
        notes.append("no-copyright-notice")
    if licence == "BSD-4-Clause":
        ack = advertising_ack(lines)
        if ack:
            notes.append("four-clause-ack:%s" % ack)
    if endorse and not binary_clause and licence == "BSD-3-Clause":
        notes.append("three-clause-variant:endorsement-without-binary-clause")
    if any("Copyright " in s and "(c)" not in s.lower() for s in statements):
        notes.append("copyright-without-(c)")
    return notes


def collect(donor):
    rels = []
    for d in ("sys/ufs/ffs", "sys/ufs/ufs", "sbin/fsck_ffs", "sbin/newfs"):
        for ext in ("*.c", "*.h"):
            rels += [os.path.relpath(p, donor) for p in glob.glob(os.path.join(donor, d, ext))]
    rels += list(CORE_FILES)
    for root, dirs, files in os.walk(os.path.join(donor, "usr.sbin/makefs")):
        dirs.sort()
        for name in sorted(files):
            if name.endswith((".c", ".h")):
                rels.append(os.path.relpath(os.path.join(root, name), donor))
    return sorted(set(rels))


def donor_revision(donor):
    lock = os.path.join(os.path.dirname(os.path.abspath(donor)), "donors.lock")
    try:
        with open(lock, "r", encoding="utf-8") as fh:
            for line in fh:
                fields = line.split()
                if len(fields) >= 2 and fields[0] == "netbsd":
                    return fields[1]
    except OSError:
        pass
    return "unknown"


def md_escape(text):
    return text.replace("|", "\\|")


def main():
    ap = argparse.ArgumentParser(description="NetBSD UFS donor licence audit")
    ap.add_argument("--donor", required=True, help="path to the netbsd donor checkout")
    ap.add_argument("--out", required=True, help="output directory (tests/ufs-vectors)")
    args = ap.parse_args()

    donor = os.path.abspath(args.donor)
    out = os.path.abspath(args.out)
    if not os.path.isdir(donor):
        sys.exit("audit: donor directory not found: %s" % donor)
    os.makedirs(out, exist_ok=True)

    rows = []
    for rel in collect(donor):
        path = os.path.join(donor, rel)
        lines = header_lines(path)
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            text = fh.read()
        nlines = len(text.splitlines())
        licence, adv, endorse, binary_clause = classify(lines)
        statements = copyright_statements(lines)
        copyright_text = " || ".join(statements) if statements else "-"
        notes = notes_for(rel, lines, licence, endorse, binary_clause)
        rows.append({
            "path": rel,
            "lines": nlines,
            "licence": licence,
            "copyright": copyright_text,
            "advertising": "yes" if adv else "no",
            "use": intended_use(rel, licence),
            "group": group_of(rel),
            "notes": "; ".join(notes) if notes else "-",
        })

    rev = donor_revision(donor)

    # --- TSV -------------------------------------------------------------
    tsv_path = os.path.join(out, "licence-audit.tsv")
    with open(tsv_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("path\tlines\tlicence\tcopyright\tadvertising-clause\tintended-use\tgroup\tnotes\n")
        for row in rows:
            fields = [
                row["path"], str(row["lines"]), row["licence"], row["copyright"],
                row["advertising"], row["use"], row["group"], row["notes"],
            ]
            # Tabs and newlines cannot appear in a TSV field.
            fields = [f.replace("\t", " ").replace("\n", " ") for f in fields]
            fh.write("\t".join(fields) + "\n")

    # --- counts ----------------------------------------------------------
    by_licence = {}
    by_group = {}
    for row in rows:
        by_licence[row["licence"]] = by_licence.get(row["licence"], 0) + 1
        by_group.setdefault(row["group"], {}).setdefault(row["licence"], 0)
        by_group[row["group"]][row["licence"]] += 1
    four = [row for row in rows if row["licence"] == "BSD-4-Clause"]
    flagged = [row for row in rows if row["licence"] in ("other", "NOASSERTION")
               or "three-clause-variant" in row["notes"] or "copyright-without-(c)" in row["notes"]]

    group_order = ["ufs-core", "fsck_ffs", "newfs", "makefs"]
    licence_order = ["BSD-2-Clause", "BSD-3-Clause", "BSD-4-Clause", "ISC", "other", "NOASSERTION"]

    def group_recommendation(group):
        if group == "ufs-core":
            return ("port the BSD-2/3 files with their notices, SPDX lines and PROVENANCE entries; "
                    "there are no BSD-4 files in this group")
        if group == "fsck_ffs":
            return "reference-only: target checker/oracle code, not part of the portable ufs-core crate"
        if group == "newfs":
            return ("reference-only: both files carry the 4-clause advertising notice, which is not "
                    "on the project allow-list; rewrite from format definitions if ufs-mkfs needs it")
        if group == "makefs":
            return ("reference-only: oracle build inputs; the BSD-4-Clause files may not be copied, "
                    "the BSD-2/3 files are tool code outside the portable core")
        return "reference-only"

    # --- Markdown --------------------------------------------------------
    md = []
    md.append(SPDX)
    md.append("# UFS donor licence audit\n")
    md.append("Generated by `tools/ufs-oracle/audit.py`; do not edit by hand.\n")
    md.append("Donor: `netbsd` revision `%s` (from `ravenbsd-resources/donors.lock`).\n" % rev)
    md.append("This is an engineering inventory, not legal advice (UNVERIFIED by counsel). "
              "The classifier reads the first 400 lines of each file and does not recurse into "
              "included headers.\n")
    md.append("## Scope\n")
    md.append("Every `*.c` and `*.h` in `sys/ufs/ffs`, `sys/ufs/ufs`, `sbin/fsck_ffs`, `sbin/newfs` "
              "and `usr.sbin/makefs` (recursive), plus `sys/kern/vfs_wapbl.c` and `sys/sys/wapbl.h`: "
              "**%d files**.\n" % len(rows))
    md.append("## Counts by licence\n")
    md.append("| Licence | Files |\n|---|---:|\n")
    for lic in licence_order:
        if lic in by_licence:
            md.append("| %s | %d |\n" % (lic, by_licence[lic]))
    md.append("\n## Counts by group\n")
    md.append("| Group | " + " | ".join(licence_order) + " | Total |\n")
    md.append("|---|" + "---:|" * (len(licence_order) + 1) + "\n")
    for group in group_order:
        counts = by_group.get(group, {})
        cells = [str(counts.get(lic, 0)) for lic in licence_order]
        md.append("| %s | %s | %d |\n" % (group, " | ".join(cells), sum(counts.values())))
    md.append("\n## Recommendation by group\n")
    for group in group_order:
        md.append("* **%s**: %s.\n" % (group, group_recommendation(group)))
    md.append("\n## The 4-clause advertising files\n")
    md.append("The 4-clause notice adds a third condition, *before* the endorsement condition:\n")
    md.append("> All advertising materials mentioning features or use of this software must "
              "display the following acknowledgement: *<text>*\n")
    md.append("It binds anyone who redistributes the code or a binary containing it: their advertising "
              "must carry the named acknowledgement. It is not satisfied by keeping the source notice. "
              "`BSD-4-Clause` is **not** in the project allow-list (architecture/12 section 2: "
              "MIT, Apache-2.0, BSD-2/3, ISC, 0BSD, Zlib, Unicode-3.0, CC0), so these files are "
              "`reference-only` until the owner accepts the obligation after a legal review or obtains "
              "permission. Do not copy them into `crates/ufs-core` or any scaffold crate.\n")
    if four:
        md.append("| File | Copyright holders (verbatim) | Acknowledgement required |\n|---|---|---|\n")
        for row in four:
            ack = "-"
            m = re.search(r"four-clause-ack:([^;]*)", row["notes"])
            if m:
                ack = md_escape(m.group(1).strip())
            md.append("| `%s` | %s | %s |\n" % (row["path"], md_escape(row["copyright"]), ack))
    else:
        md.append("No 4-clause files were found in the audited scope.\n")
    md.append("\n## Flagged and unusual notices\n")
    if flagged:
        md.append("| File | Licence | Flag |\n|---|---|---|\n")
        for row in flagged:
            flags = []
            if row["licence"] == "other":
                flags.append("non-standard BSD text")
            if row["licence"] == "NOASSERTION":
                flags.append("no copyright/licence notice")
            if "three-clause-variant" in row["notes"]:
                flags.append("3-clause variant without the binary-redistribution clause")
            if "copyright-without-(c)" in row["notes"]:
                flags.append("copyright statement without `(c)`")
            md.append("| `%s` | %s | %s |\n" % (row["path"], row["licence"], "; ".join(flags)))
    else:
        md.append("None.\n")
    md.append("\n## All files\n")
    md.append("| Path | Lines | Licence | Advertising | Intended use | Copyright (verbatim) | Notes |\n")
    md.append("|---|---:|---|---:|---|---|---|\n")
    for row in rows:
        md.append("| `%s` | %d | %s | %s | %s | %s | %s |\n" % (
            row["path"], row["lines"], row["licence"], row["advertising"], row["use"],
            md_escape(row["copyright"]), md_escape(row["notes"])))
    md.append("\n## NOTICE\n")
    md.append("`tests/ufs-vectors/NOTICE-UFS.txt` lists the distinct copyright statements. "
              "Section A is what `NOTICE` must carry if the recommended `port` files are copied. "
              "Section B is listed for completeness only; those files are not recommended for copying.\n")

    md_path = os.path.join(out, "LICENCE-AUDIT.md")
    with open(md_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(md))

    # --- NOTICE additions ------------------------------------------------
    def distinct(group_filter):
        seen = set()
        for row in rows:
            if not group_filter(row):
                continue
            for stmt in row["copyright"].split(" || "):
                if stmt and stmt != "-":
                    seen.add(stmt.strip())
        return sorted(seen)

    core_statements = distinct(lambda r: r["group"] == "ufs-core" and r["use"] == "port")
    tool_statements = distinct(lambda r: r["group"] != "ufs-core" and r["use"] == "reference-only")

    notice = []
    notice.append("# RavenBSD NOTICE additions for NetBSD UFS code\n")
    notice.append("# Generated by tools/ufs-oracle/audit.py; do not edit by hand.\n")
    notice.append("# Donor: netbsd revision %s\n\n" % rev)
    notice.append("# Section A - core files recommended for porting (sys/ufs/ffs, sys/ufs/ufs, WAPBL).\n")
    notice.append("# NOTICE must carry these statements if those files are ported.\n")
    for stmt in core_statements:
        notice.append("%s\n" % stmt)
    notice.append("\n# Section B - oracle/tool files, recommended reference-only.\n")
    notice.append("# Not required for the core port. If one of these files is ever copied, its\n")
    notice.append("# statement (and, for BSD-4-Clause, the advertising acknowledgement) applies.\n")
    for stmt in tool_statements:
        notice.append("%s\n" % stmt)
    notice_path = os.path.join(out, "NOTICE-UFS.txt")
    with open(notice_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(notice))

    print("audit: %d files, %d four-clause, %d flagged" % (len(rows), len(four), len(flagged)))
    print("audit: wrote %s, %s, %s" % (tsv_path, md_path, notice_path))


if __name__ == "__main__":
    main()
