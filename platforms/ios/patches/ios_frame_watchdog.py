#!/usr/bin/env python3
"""Add iOS-only, low-frequency runtime watchdog diagnostics to the CI checkout.

No gameplay changes are committed. The monitor lives on a separate Rust thread,
so it can report even when winit's main callback stops returning. Counts are
atomic, and the monitor writes only once every five seconds.
"""
from pathlib import Path


def change(path: str, old: str, new: str) -> None:
    file = Path(path)
    source = file.read_text()
    assert source.count(old) == 1, (
        f"{path}: upstream/patch order changed near {old[:90]!r}"
    )
    file.write_text(source.replace(old, new, 1))


loading = "crates/horizon-client/src/game/runtime/loading_screen.rs"
resources = "crates/horizon-client/src/game/runtime/resource_load.rs"

watchdog = r'''
// Nova iOS diagnostic counters. The independent monitor never calls UIKit,
// winit, or Metal, and does not wake or change the game's event loop.
#[cfg(target_os = "ios")]
static NOVA_IOS_WATCHDOG_ONCE: std::sync::Once = std::sync::Once::new();
#[cfg(target_os = "ios")]
static NOVA_IOS_PHASE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_IDLE_BEGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_IDLE_END: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_REDRAW_BEGIN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_REDRAW_END: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_LOADING_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(target_os = "ios")]
static NOVA_IOS_GAME_FRAMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(target_os = "ios")]
fn nova_ios_start_watchdog(progress: std::sync::Arc<std::sync::atomic::AtomicU32>) {
    use std::sync::atomic::Ordering::Relaxed;
    NOVA_IOS_WATCHDOG_ONCE.call_once(|| {
        let task = move || {
            let mut previous = [0u64; 6];
            loop {
                std::thread::sleep(std::time::Duration::from_secs(5));
                let current = [
                    NOVA_IOS_IDLE_BEGIN.load(Relaxed),
                    NOVA_IOS_IDLE_END.load(Relaxed),
                    NOVA_IOS_REDRAW_BEGIN.load(Relaxed),
                    NOVA_IOS_REDRAW_END.load(Relaxed),
                    NOVA_IOS_LOADING_FRAMES.load(Relaxed),
                    NOVA_IOS_GAME_FRAMES.load(Relaxed),
                ];
                eprintln!(
                    "[nova-watchdog] phase={} progress={}/9 idle={} (+{}) returned={} (+{}) redraw={} (+{}) completed={} (+{}) loading_frames={} (+{}) game_frames={} (+{})",
                    NOVA_IOS_PHASE.load(Relaxed),
                    progress.load(Relaxed),
                    current[0], current[0].saturating_sub(previous[0]),
                    current[1], current[1].saturating_sub(previous[1]),
                    current[2], current[2].saturating_sub(previous[2]),
                    current[3], current[3].saturating_sub(previous[3]),
                    current[4], current[4].saturating_sub(previous[4]),
                    current[5], current[5].saturating_sub(previous[5]),
                );
                previous = current;
            }
        };
        if let Err(error) = std::thread::Builder::new()
            .name("nova-ios-watchdog".into())
            .spawn(task)
        {
            eprintln!("[nova-watchdog] could not start: {error}");
        }
    });
}
'''
change(
    loading,
    "impl ApplicationHandler for App {\n",
    watchdog + "\nimpl ApplicationHandler for App {\n",
)
change(
    loading,
    "    pub(super) fn resumed(&mut self, event_loop: &ActiveEventLoop) {\n"
    "        if self.window.is_some() {",
    "    pub(super) fn resumed(&mut self, event_loop: &ActiveEventLoop) {\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        nova_ios_start_watchdog(self.progress.value.clone());\n"
    "        if self.window.is_some() {",
)
change(
    loading,
    "    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {\n"
    "        nova_ios_deep(|| self.window_event_here(event_loop, id, event));\n"
    "    }",
    "    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        let redraw = matches!(&event, WindowEvent::RedrawRequested);\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        if redraw { NOVA_IOS_REDRAW_BEGIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }\n"
    "        nova_ios_deep(|| self.window_event_here(event_loop, id, event));\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        if redraw { NOVA_IOS_REDRAW_END.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }\n"
    "    }",
)
change(
    loading,
    "    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {\n"
    "        nova_ios_deep(|| self.about_to_wait_here(event_loop));\n"
    "    }",
    "    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        NOVA_IOS_IDLE_BEGIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n"
    "        nova_ios_deep(|| self.about_to_wait_here(event_loop));\n"
    "        #[cfg(target_os = \"ios\")]\n"
    "        {\n"
    "            if let App::Ready(game) = self {\n"
    "                NOVA_IOS_GAME_FRAMES.store(game.host.frames as u64, std::sync::atomic::Ordering::Relaxed);\n"
    "            }\n"
    "            NOVA_IOS_IDLE_END.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n"
    "        }\n"
    "    }",
)
change(
    loading,
    "                Ok(true) if self.gui.is_some() && self.menu.pending.is_none() => {\n"
    "                    self.title_drawn = true;\n"
    "                }\n"
    "                Ok(_) => {}",
    "                Ok(true) if self.gui.is_some() && self.menu.pending.is_none() => {\n"
    "                    #[cfg(target_os = \"ios\")]\n"
    "                    NOVA_IOS_LOADING_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n"
    "                    self.title_drawn = true;\n"
    "                }\n"
    "                Ok(true) => {\n"
    "                    #[cfg(target_os = \"ios\")]\n"
    "                    NOVA_IOS_LOADING_FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n"
    "                }\n"
    "                Ok(_) => {}",
)
change(
    loading,
    "        if let Some(loader) = loaded.rest_loader.take() {\n"
    "            loader.finish()?;\n",
    "        if let Some(loader) = loaded.rest_loader.take() {\n"
    "            #[cfg(target_os = \"ios\")]\n"
    "            NOVA_IOS_PHASE.store(1, std::sync::atomic::Ordering::Relaxed);\n"
    "            eprintln!(\"[nova-watchdog] handoff.join.begin\");\n"
    "            loader.finish()?;\n"
    "            eprintln!(\"[nova-watchdog] handoff.join.done\");\n",
)
change(
    loading,
    "            Step::Ready(game) => *self = App::Ready(game),",
    "            Step::Ready(game) => {\n"
    "                #[cfg(target_os = \"ios\")]\n"
    "                NOVA_IOS_PHASE.store(2, std::sync::atomic::Ordering::Relaxed);\n"
    "                eprintln!(\"[nova-watchdog] handoff.game-ready\");\n"
    "                *self = App::Ready(game);\n"
    "            },",
)
change(
    resources,
    "            .spawn(move || {\n"
    "                let _step = startup::step(\"rest\");",
    "            .spawn(move || {\n"
    "                eprintln!(\"[nova-worker] world-rest.started\");\n"
    "                let _step = startup::step(\"rest\");",
)
change(
    resources,
    "                    true,\n"
    "                )\n"
    "                .map_err(|error| error.to_string())?;\n"
    "                resources",
    "                    true,\n"
    "                )\n"
    "                .map_err(|error| {\n"
    "                    eprintln!(\"[nova-worker] world-rest.failed: {error}\");\n"
    "                    error.to_string()\n"
    "                })?;\n"
    "                eprintln!(\"[nova-worker] world-rest.built\");\n"
    "                resources",
)
change(
    resources,
    "                progress.set(9);\n"
    "                log::info!(\n"
    "                    \"Horizon: rest-ready after {} ms\",",
    "                progress.set(9);\n"
    "                eprintln!(\"[nova-worker] world-rest.published\");\n"
    "                log::info!(\n"
    "                    \"Horizon: rest-ready after {} ms\",",
)
print("Nova iOS watchdog: installed 5s worker/progress/idle/redraw/frame diagnostics")
