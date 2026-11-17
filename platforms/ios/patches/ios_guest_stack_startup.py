#!/usr/bin/env python3
"""iOS-only experiment: skip the initial main-thread stack-size probe.

Horizon checks for 136 MiB before creating the iOS winit/UIKit event loop.
Do not change the guest stack constants or checks for guest-capable workers.
This script only modifies the disposable CI checkout, not upstream in Git.
"""
from pathlib import Path

path = Path("crates/horizon-guest-stack/src/lib.rs")
source = path.read_text()
anchor = "pub fn prepare_main() {\n"
assert source.count(anchor) == 1, "Upstream guest stack startup changed; review iOS patch"
replacement = (
    '#[cfg(not(target_os = "ios"))]\n'
    + anchor
)
source = source.replace(anchor, replacement, 1)
insertion = (
    '#[cfg(target_os = "ios")]\n'
    'pub fn prepare_main() {\n'
    '    // UIKit/winit runs on the original iOS main thread, which has a small\n'
    '    // fixed stack. Preserve full-size stack checks for actual guest workers.\n'
    '    eprintln!("Nova iOS: skipping main-thread guest stack startup probe");\n'
    '}\n\n'
)
assert source.count(replacement) == 1
source = source.replace(replacement, insertion + replacement, 1)
path.write_text(source)
print("Nova iOS: bypassed main-thread startup stack probe only")
