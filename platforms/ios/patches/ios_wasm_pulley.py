#!/usr/bin/env python3
"""Opt the iOS game into Pulley without changing Horizon's cross-platform code.

Apply only in the disposable CI checkout. On iOS, running a separate
--compile-wasm-module process is prohibited, so interpret wasm through
Wasmtime's pulley64 target and compile Pulley bytecode inside Nova.

Experimental limitation: in-process compilation cannot be terminated by
Horizon's subprocess-based compilation deadline. Guest execution still uses
the existing epoch, memory, and stack checks. Never apply this on desktop.
"""

from pathlib import Path


def replace_once(path: Path, old: str, new: str) -> None:
    source = path.read_text()
    count = source.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one patch anchor, found {count}: {old!r}")
    path.write_text(source.replace(old, new, 1))


runtime = Path("crates/horizon-mod-wasm/src/runtime.rs")
replace_once(
    runtime,
    "pub(crate) fn engine_config() -> wasmtime::Config {",
    "pub(crate) fn engine_config() -> wasmtime::Result<wasmtime::Config> {",
)
replace_once(
    runtime,
    "    config.epoch_interruption(ENGINE_CONFIG.1);",
    '    #[cfg(target_os = "ios")]\n'
    '    config.target("pulley64")?;\n'
    "    config.epoch_interruption(ENGINE_CONFIG.1);",
)
replace_once(
    runtime,
    "    config\n}\n\npub(crate) fn engine()",
    "    Ok(config)\n}\n\npub(crate) fn engine()",
)
replace_once(
    runtime,
    "    let config = engine_config();\n    let engine = Engine::new(&config)?;",
    '    let config = engine_config()?;\n'
    '    let engine = Engine::new(&config)?;\n'
    '    #[cfg(target_os = "ios")]\n'
    '    {\n'
    '        if !engine.is_pulley() {\n'
    '            return Err(wasmtime::Error::msg("Nova iOS: Pulley interpreter was not selected"));\n'
    '        }\n'
    '        log::info!("Nova iOS wasm runtime: Pulley interpreter (pulley64)");\n'
    '    }',
)

cache = Path("crates/horizon-mod-wasm/src/module_cache.rs")
replace_once(
    cache,
    "        let child = if cfg!(test) {",
    '        // iOS cannot spawn a new instance of the app as a wasm compiler.\n'
    '        // Do not change the bounded child-compiler path on desktop.\n'
    '        let child = if cfg!(any(test, target_os = "ios")) {',
)
print("Nova iOS: Pulley engine and in-process wasm preparation enabled")
