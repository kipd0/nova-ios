#!/usr/bin/env python3
"""iOS-only startup handoff synchronization for Horizon's world-rest tables.

The initial core loader publishes Loaded while its 'world-rest-loader' worker
continues filling Resources.rest. On iOS, startup setup can access rest()
before that worker publishes the tables and panic. Keep the title/loading
screen active until the worker finishes, then join and propagate its outcome
before Game construction. This modifies only the CI runner checkout.
"""
from pathlib import Path

path = Path("crates/horizon-client/src/game/runtime/loading_screen.rs")
source = path.read_text()

def replace_once(old: str, new: str) -> None:
    global source
    assert source.count(old) == 1, (
        f"Upstream loading screen changed; review iOS handoff patch: {old[:75]!r}"
    )
    source = source.replace(old, new, 1)

replace_once(
    "    pub(super) fn finish(&mut self, mut loaded: Loaded) -> Result<Option<split::Split<'static>>> {\n"
    "        let handoff_at = Instant::now();",
    "    pub(super) fn finish(&mut self, mut loaded: Loaded) -> Result<Option<split::Split<'static>>> {\n"
    "        // iOS: the initial world-rest worker must publish its tables before\n"
    "        // Game setup reads them. The loading loop waited for completion below,\n"
    "        // so joining here should not block the UIKit event loop.\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        if let Some(loader) = loaded.rest_loader.take() {\n"
    "            loader.finish()?;\n"
    "            eprintln!(\"Nova iOS: world-rest tables joined before game handoff\");\n"
    "        }\n"
    "        let handoff_at = Instant::now();"
)

replace_once(
    "                match if loading.menu.reload_packs || !loading.ready_for_handoff() {",
    "                // Keep the iOS title/loading screen responsive while the\n"
    "                // world-rest loader finishes. Joining an unfinished loader on\n"
    "                // UIKit's main thread would freeze the event loop.\n"
    "                #[cfg(target_os = \"ios\")]\n"
    "                let rest_pending = loading.loaded.as_ref()\n"
    "                    .and_then(|result| result.as_ref().ok())\n"
    "                    .and_then(|loaded| loaded.rest_loader.as_ref())\n"
    "                    .is_some_and(|worker| !worker.handle.is_finished());\n"
    "                #[cfg(not(target_os = \"ios\"))]\n"
    "                let rest_pending = false;\n"
    "                match if loading.menu.reload_packs || rest_pending || !loading.ready_for_handoff() {"
)

path.write_text(source)
print("Nova iOS: wait for and join world-rest loader before game handoff")
