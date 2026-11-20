#!/usr/bin/env python3
"""Avoid iOS resource-loader thread exhaustion in the CI checkout.

Horizon reserves a 136 MiB guest-capable stack for each named scoped loader.
On iOS, a later resource load from the integrated server can fail to create
another such thread (EAGAIN / OS error 35), which named_scope treats as a panic.
Use Horizon's existing non-parallel Step::Later path for iOS resource loading
instead. This keeps the world/job worker configuration and non-iOS builds
unchanged, and the patch is applied only to the ephemeral iOS CI checkout.
"""
from pathlib import Path

path = Path("crates/horizon-client/src/game/runtime/resource_load.rs")
source = path.read_text()
original = "    let parallel = threads > 1;\n"
replacement = (
    "    // Nova iOS: a scoped resource loader reserves a 136 MiB guest stack.\n"
    "    // During integrated-server startup, creating another such thread can\n"
    "    // fail with EAGAIN (iOS error 35), which otherwise panics in named_scope.\n"
    "    // Step::Later already runs every stage when parallel is false; keep\n"
    "    // resource loads sequential on iOS without changing world job pools.\n"
    '    let parallel = threads > 1 && !cfg!(target_os = "ios");\n'
)
assert source.count(original) == 1, (
    "Upstream resource loading concurrency changed; review iOS thread budget patch"
)
path.write_text(source.replace(original, replacement, 1))
print("Nova iOS: resource stages use the existing serial loader on iOS")
