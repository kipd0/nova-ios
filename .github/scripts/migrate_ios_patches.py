#!/usr/bin/env python3
"""One-shot Nova iOS migration: bake the existing CI patches into tracked Rust.

Runs ONLY on test-touch-screen, using the exact ordered patch steps from the
working iOS build workflow. The application/renderer behavior is unchanged.
"""
from __future__ import annotations

import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
os.chdir(ROOT)
WORKFLOW = Path(".github/workflows/ios-runtime-probe.yml")
EXPECTED_BRANCH = "test-touch-screen"

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True).strip()

def run(*args: str) -> None:
    subprocess.run(list(args), check=True)

assert git("branch", "--show-current") == EXPECTED_BRANCH, "Refusing to modify an unexpected branch"
assert not git("status", "--porcelain"), "Start from a clean checkout"

for script in sorted(Path("platforms/ios/patches").glob("*.py")):
    run("python3", "-m", "py_compile", str(script))

lines = WORKFLOW.read_text(encoding="utf-8").splitlines(keepends=True)
step_indices = [
    i for i, line in enumerate(lines)
    if line.startswith("      - name: ")
]
blocks = []
for index, begin in enumerate(step_indices):
    end = step_indices[index + 1] if index + 1 < len(step_indices) else len(lines)
    name = lines[begin].split("- name: ", 1)[1].strip()
    blocks.append((name, begin, end))

first_name = "Apply minimal iOS-only allocator pointer compatibility patch"
last_name = "Enable Pulley interpreter for iOS WebAssembly mods"
start = next(i for i, (name, _, _) in enumerate(blocks) if name == first_name)
finish = next(i for i, (name, _, _) in enumerate(blocks) if name == last_name)
assert finish > start and finish - start >= 12
assert blocks[finish + 1][0] == "Trace desktop Wayland dependency on iOS"

# Every migration step is identical to its pre-migration CI counterpart,
# including the inline compatibility and renderer diagnostics patches.
for name, begin, end in blocks[start:finish + 1]:
    block = lines[begin:end]
    body_idx = next(
        (i for i, line in enumerate(block) if line == "        run: |\n"),
        None,
    )
    assert body_idx is not None, f"Missing script body: {name}"
    raw = block[body_idx + 1:]
    script_lines = [
        line[10:] if line.startswith("          ") else line
        for line in raw
    ]
    command = "".join(script_lines)
    assert command.strip(), f"Empty patch step: {name}"
    print(f"Applying: {name}", flush=True)
    run("bash", "-e", "-o", "pipefail", "-c", command)

# None of these disabled/experimental modifications were ever applied by
# the actual iOS workflow. In particular, do NOT add the 136MiB stack bypass.
assert not any("ios_guest_stack_startup.py" in s for s in [
    "".join(lines[begin:end]) for _, begin, end in blocks[start:finish + 1]
]), "Disabled guest stack bypass should not be activated"

# All effective changes are now in tracked sources and iOS-only modules.
# Remove ONLY the patch steps from the real build workflow.
new_lines = (
    lines[:blocks[start][1]]
    + [
        "      # iOS compatibility, renderer diagnostics, input and WASM support\n",
        "      # are now compiled directly from tracked Rust sources.\n",
        "\n",
    ]
    + lines[blocks[finish][2]:]
)
new_workflow = "".join(new_lines)
assert "python3 platforms/ios/patches/" not in new_workflow
assert "Apply minimal iOS-only allocator pointer compatibility patch" not in new_workflow
new_workflow = new_workflow.replace(
    "      - 'platforms/ios/patches/ios_guest_stack_startup.py'\n"
    "      - 'platforms/ios/patches/ios_resource_handoff.py'\n",
    "",
)
WORKFLOW.write_text(new_workflow, encoding="utf-8")

# The old adapter is now compiled from the real game's iOS-only source.
source_adapter = Path("crates/horizon-client/src/game/runtime/ios_control_key.inc")
assert source_adapter.is_file(), "Missing compiled iOS keyboard adapter"
assert Path("crates/horizon-client/src/game/interface/ios_keyboard.rs").is_file()
assert Path("crates/horizon-client-input/src/ios_input.rs").is_file()
assert "ios_control_key.inc" in Path("crates/horizon-client/src/game/runtime/control.rs").read_text()
run("git", "rm", "platforms/ios/adapters/ios_control_key.rs")
run("git", "rm", "-r", "platforms/ios/patches")

# Record why the intentionally dormant iOS stack bypass was not imported.
readme = Path("platforms/ios/README.md")
readme.write_text(
    readme.read_text(encoding="utf-8").rstrip()
    + "\n\n### Tracked iOS integration (test-touch-screen)\n\n"
    + "The CI-time Python source patches were migrated into tracked Rust and "
    + "Cargo source files. The iOS build now compiles those changes directly; "
    + "no patch scripts are applied during the build. iOS-specific paths are "
    + "guarded with target_os where practical.\n\n"
    + "The older 136 MiB main-thread stack-probe bypass was disabled in the "
    + "original workflow and was deliberately not enabled during migration.\n",
    encoding="utf-8",
)

run("git", "add", "-A", "crates", "platforms/ios", ".github/workflows/ios-runtime-probe.yml")
run("git", "diff", "--cached", "--check")
diff = git("diff", "--cached", "--stat")
print("Migration staged diff:\n" + diff, flush=True)
changed = git("diff", "--cached", "--name-only").splitlines()
assert any(path.startswith("crates/") for path in changed)
assert not any(path.startswith(".github/workflows/") and path != ".github/workflows/ios-runtime-probe.yml" for path in changed)
assert len(changed) > 20, "Unexpectedly small migration diff"
git_user = "github-actions[bot]"
run("git", "config", "user.name", git_user)
run("git", "config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com")
run("git", "commit", "-m", "iOS: migrate temporary build patches into tracked source")
print("Pushing completed migration to " + EXPECTED_BRANCH, flush=True)
run("git", "push", "origin", f"HEAD:refs/heads/{EXPECTED_BRANCH}")
print("Migration pushed: " + git("rev-parse", "HEAD"), flush=True)
