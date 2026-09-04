#!/usr/bin/env python3
"""Measure the gpui surface used by the headless agent graph.

For every workspace crate reachable from `crates/telos` (Linux target),
collect `gpui::Item` and `use gpui::{...}` references, strip cfg(test)
blocks, and bucket each item by the gpui module that defines it.

Usage:
    cargo tree -p telos --target x86_64-unknown-linux-gnu --prefix none \
        | grep -oE '/crates/[a-z0-9_]+' | sort -u   # -> reachable set
    python3 tooling/gpui-surface/scan.py < reachable.txt
"""
import collections
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
CRATES = ROOT / "crates"
GPUI = CRATES / "gpui" / "src"
SKIP = {
    "gpui", "gpui_macros", "gpui_macos", "gpui_linux", "gpui_apple",
    "gpui_windows", "gpui_web", "gpui_wgpu", "gpui_platform", "assets",
}
TEST_FIXTURES = re.compile(r"(^|/)fixtures/")

def reachable():
    return {line.strip() for line in sys.stdin if line.strip()}

def strip_cfg_test(text):
    # Naive cfg(test)/cfg(any(test, ...))/mod tests block removal by brace
    # depth. A skipped unit must open a brace before it may close, so
    # multi-line item signatures keep the whole item out of the scan.
    depth = 0
    skip = False
    opened = False
    out = []
    for line in text.splitlines():
        if re.search(r"#\[cfg\(test\)\]|#\[cfg\(any\(test", line):
            skip = True
            depth = 0
            opened = False
            continue
        if skip:
            depth += line.count("{") - line.count("}")
            if depth > 0:
                opened = True
            if opened and depth <= 0:
                skip = False
            continue
        if re.match(r"\s*mod tests\b", line):
            skip = True
            depth = 0
            opened = False
            continue
        out.append(line)
    return "\n".join(out)

def define_mod(item):
    hits = []
    pat = re.compile(r"\bpub (?:struct|enum|type|trait|fn|const|static|mod) %s\b" % re.escape(item))
    for f in GPUI.rglob("*.rs"):
        for ln in f.read_text(errors="ignore").splitlines():
            if pat.search(ln):
                hits.append(str(f.relative_to(GPUI)))
                break
    return hits

def main():
    roots = reachable()
    dirs = sorted(
        d for d in CRATES.iterdir()
        if d.is_dir() and d.name in roots and d.name not in SKIP
    )
    usage = collections.Counter()
    item_re = re.compile(r"\bgpui::([A-Za-z_][A-Za-z0-9_]*)\b")
    use_block = re.compile(r"use\s+gpui::\{([^}]*)\}")
    use_single = re.compile(r"use\s+gpui::([A-Za-z_][A-Za-z0-9_]*)")

    for d in dirs:
        for p in d.rglob("*.rs"):
            if TEST_FIXTURES.search(str(p)):
                continue
            text = strip_cfg_test(p.read_text(errors="ignore"))
            for m in item_re.finditer(text):
                usage[m.group(1)] += 1
            for block in use_block.findall(text):
                for name in block.split(","):
                    name = name.split(" as ")[0].strip().split("::")[0]
                    if name and name != "*":
                        usage[name] += 1
            for m in use_single.finditer(text):
                usage[m.group(1).split("::")[0]] += 1

    print("sourced crates: %d" % len(dirs))
    print("item\tcount\tdefined-in")
    for item, count in usage.most_common():
        mods = define_mod(item)
        print("%s\t%d\t%s" % (item, count, ",".join(mods) if mods else "?"))

if __name__ == "__main__":
    main()
