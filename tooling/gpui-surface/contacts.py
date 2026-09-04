#!/usr/bin/env python3
"""Record which sourced crates touch which gpui UI-cluster modules.

For every workspace crate reachable from `crates/telos` (Linux target),
collect `gpui::Item` and `use gpui::{...}` references, strip cfg(test)
blocks, resolve each item to the gpui module that defines it, and keep
only items whose defining module belongs to the Wave 3 UI cluster gate
set (tooling/gpui-surface/README.md). The output is the contact surface:
the consumer sites that keep a UI-cluster module compiled.

Usage:
    cargo tree -p telos --target x86_64-unknown-linux-gnu --prefix none \
        | grep -oE '/crates/[a-z0-9_]+' | sort -u   # -> reachable set
    python3 tooling/gpui-surface/contacts.py < reachable.txt
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
# Wave 3 gate set: defining module paths that must leave the headless
# graph. `style.rs` and `text_system*` appear only for their layout or
# render half; value types defined in the same files are Stream 2 data
# and are excluded by the manual scan review.
GATE_PREFIXES = (
    "window.rs",
    "scene.rs",
    "elements/",
    "styled.rs",
    "view.rs",
    "spring.rs",
    "text_system",
    "svg_renderer",
    "style.rs",
    "taffy.rs",
    "element.rs",
    "assets.rs",
    "interactive.rs",
    "key_dispatch.rs",
    "keymap/",
    "gestures.rs",
    "tab_stop.rs",
    "shared_uri.rs",
    "asset_cache.rs",
)

def reachable():
    return {line.strip() for line in sys.stdin if line.strip()}

def strip_cfg_test(text):
    depth = 0
    skip = False
    out = []
    for line in text.splitlines():
        if re.search(r"#\[cfg\(test\)\]|#\[cfg\(any\(test", line):
            skip = True
            continue
        if skip:
            depth += line.count("{") - line.count("}")
            if depth <= 0:
                skip = False
            continue
        if re.match(r"\s*mod tests\b", line):
            skip = True
            depth = 0
            continue
        out.append(line)
    return "\n".join(out)

def define_mods():
    define = {}
    pat = re.compile(
        r"\bpub (?:struct|enum|type|trait|fn|const|static|mod) "
        r"([A-Za-z_][A-Za-z0-9_]*)\b"
    )
    for f in GPUI.rglob("*.rs"):
        for line in f.read_text(errors="ignore").splitlines():
            for m in pat.finditer(line):
                define.setdefault(m.group(1), str(f.relative_to(GPUI)))
    return define

def main():
    define = define_mods()
    roots = reachable()
    dirs = sorted(
        d for d in CRATES.iterdir()
        if d.is_dir() and d.name in roots and d.name not in SKIP
    )
    item_re = re.compile(r"\bgpui::([A-Za-z_][A-Za-z0-9_]*)\b")
    use_block = re.compile(r"use\s+gpui::\{([^}]*)\}")
    use_single = re.compile(r"use\s+gpui::([A-Za-z_][A-Za-z0-9_]*)")

    # crate file -> {(item, module)}
    contact = collections.defaultdict(set)
    for d in dirs:
        for p in d.rglob("*.rs"):
            if TEST_FIXTURES.search(str(p)):
                continue
            text = strip_cfg_test(p.read_text(errors="ignore"))
            items = set()
            for m in item_re.finditer(text):
                items.add(m.group(1))
            for block in use_block.findall(text):
                for name in block.split(","):
                    name = name.split(" as ")[0].strip().split("::")[0]
                    if name and name != "*":
                        items.add(name)
            for m in use_single.finditer(text):
                items.add(m.group(1).split("::")[0])
            for item in items:
                mod = define.get(item)
                if mod and mod.startswith(GATE_PREFIXES):
                    contact[str(p.relative_to(CRATES))].add((item, mod))

    print("contact files: %d" % len(contact))
    for rel in sorted(contact):
        print(rel)
        for item, mod in sorted(contact[rel]):
            print("\t%s <- %s" % (item, mod))

if __name__ == "__main__":
    main()
