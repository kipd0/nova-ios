# Nova — Horizon iOS host

This directory provides Nova's iOS/iPadOS host for Horizon. On the
`test-touch-screen` branch, the iOS runtime changes are tracked in Rust and
Cargo sources instead of injected into Horizon at build time. Gameplay
behavior is intended to match upstream, but its tracked engine files are no
longer byte-for-byte identical to upstream.

## Current status

- The integrated iOS runtime can launch Horizon on a device with user-provided
  prepared resources; menus and gameplay have been tested during development.
- The Swift launcher/overlay supplies touch controls and account setup.
- A Rust static library links the actual Horizon game; the lightweight
  standalone `RustBridge` remains a scaffold for launcher-only builds.
- GitHub Actions builds an unsigned IPA; signing/sideloading is separate.
- The `test-touch-screen` source migration still requires a fresh IPA build and
  on-device testing, especially for mod keybinds.

## Build on a Mac

Requires Xcode, Rust, the aarch64-apple-ios Rust target, and XcodeGen.

```bash
rustup target add aarch64-apple-ios
cargo test --manifest-path platforms/ios/RustBridge/Cargo.toml
cargo build --manifest-path platforms/ios/RustBridge/Cargo.toml \
  --release --target aarch64-apple-ios \
  --target-dir platforms/ios/Build/rust-target
mkdir -p platforms/ios/Build/rust
cp platforms/ios/Build/rust-target/aarch64-apple-ios/release/libhorizon_ios_bridge.a \
   platforms/ios/Build/rust/
cd platforms/ios
xcodegen generate --spec project.yml
xcodebuild -project HorizonIOS.xcodeproj -scheme HorizonIOS \
  -configuration Release -sdk iphoneos -destination 'generic/platform=iOS' \
  -derivedDataPath Build/Derived CODE_SIGNING_ALLOWED=NO build
```

Unsigned IPAs produced by CI are build artifacts, not ready for installation
on standard devices. Signing requires an Apple development or distribution
identity and provisioning appropriate to the target device.

## Next engineering tasks

1. Rebuild the integrated IPA from `test-touch-screen` without CI patch steps.
2. Verify touch keyboard/mouse events in gameplay, menus and mod keybindings.
3. Check login, audio, resource loading, Metal rendering and app relaunches.
4. Keep iOS-specific code isolated and review merge conflicts with upstream
   Horizon before accepting later engine updates.

See root PLAN.txt for architectural constraints and upstream update policy.

## Game archive import

Nova can copy an existing ZIP file from Files.app into its sandbox at
`Documents/Nova/Imports/GameFiles.zip` without trying to run, extract, or
interpret its contents. The importer checks only the ZIP header; it does NOT
prove that the archive contains valid prepared Horizon assets. The original
game expects a prepared resource root containing `horizon-resources.json`
and resources under `assets/minecraft/`, and the current desktop entry
creates its own winit event loop. Do NOT enable Play simply because a ZIP
was imported. Examine the ZIP version and directory structure first. Do not
commit user-supplied Minecraft assets to this public repository.

## Metal surface integration diagnostic

Nova's "Test Metal display" screen creates an MTKView, presents a simple
GPU-cleared frame, and registers a **non-owning** UIView pointer through
`horizon_ios_set_metal_view` in either Rust bridge variant. It unregisters
the pointer when dismissed. This proves the SwiftUI -> Rust ABI and native
drawable creation can be exercised; it does NOT pass a wgpu surface to Horizon,
does NOT start Horizon's event loop, and is not game rendering.

## Player-owned resource ZIP extraction (no bundled assets)

Nova can import a player's prepared Horizon resources ZIP from Files.app,
then "Extract and verify game files" **locally**, using the shared Rust
`platforms/ios/AssetImporter` component (with no network calls). The ZIP may
contain the files at its root or inside one wrapper folder such as `26.3/`.

The installer rejects ZIP traversal paths, symlinks, duplicate names,
oversized entries and excessive expansion. It streams decompression into a
private temporary folder, checks `horizon-resources.json` as JSON and
requires `facts.json`, `registries.json`, and nonempty `assets/minecraft/`.
On validation success it replaces resources transactionally; failure preserves
the previous working copy. The prepared files live in
`Application Support/Nova/Resources` (excluded from iCloud backup).
Nova's temporary cached ZIP copy is deleted after successful preparation,
without touching the user's original selected ZIP.

**This is structural validation only**, not compatibility or runtime testing.
Neither an existing ZIP nor installed files enable the Play button. Horizon's
original winit/Metal lifecycle must still be connected.

**No game assets or downloaded ZIPs are committed or distributed.** Both IPA
build workflows check the final archive file listing to reject bundled assets.

## Single-lifecycle iOS game startup experiment

The integrated game-linked Nova app now has a **cold-start experiment**, not
a normal in-app Play transition. Because winit 0.30 on iOS runs
`UIApplicationMain`, it cannot start after SwiftUI has already initialized
UIKit. Nova has a single entrypoint (`NovaEntryPoint`) which chooses ONE:

1. Normal SwiftUI Nova launcher; prepare the player's own game assets locally.
2. If the user explicitly tapped "Try Horizon (next launch)" and then fully
   terminates and reopens Nova, run the ORIGINAL official Horizon client from
   the process main thread. That process is owned by winit, not SwiftUI.

The one-shot boot marker is cleared BEFORE the game starts, so a startup crash
doesn't leave Nova in an endless game-boot loop. Launcher-only builds ignore
this mode. Game-linked builds use a private resources path from
`Application Support/Nova/Resources` and game home from
`Application Support/Nova/Game`.

The tracked `crates/horizon-client/src/game/runtime/launch_options.rs`
contains the iOS resource-root handling through the private
`NOVA_IOS_RESOURCES` environment variable. This is no longer a runner-only patch.

**Status:** The integrated runtime has launched on an iPad in earlier builds,
although crashes and input compatibility issues remain under investigation.
The newly tracked-source version has not yet been compiled and tested on-device.

## Exportable black-screen diagnostics

The launcher now offers **View Logs** and **Export Logs**. Diagnostics persist
in the app's private `Application Support/Nova/Diagnostics/nova-diagnostics.log`
across crashes and cold launches, and are excluded from iCloud backup. The log
is capped to about 2 MB with a previous-log rollover when Swift next writes.
Only choosing Export Logs opens the system share sheet; there is no telemetry
upload. Don't share logs publicly without checking their contents.

The cold-start Swift entry records which branch was chosen; the full Rust
bridge writes stages before it enters the original Horizon startup. The iOS integration and diagnostic checkpoints are now tracked in source,
including Horizon's loading/window code. The build workflow no longer injects
these changes from Python scripts.

To capture a black screen, start an experiment, fully close and reopen Nova
to run it, then force-close Nova and reopen normally. The one-shot startup
flag is cleared before the game entry, so the launcher returns on the next
cold start. Open View Logs or use Export Logs to share the log. A log with
no renderer success is evidence of an early startup problem, not proof of a
Metal failure.

### Tracked iOS integration (test-touch-screen)

The active CI-time Python source patches were migrated into tracked Rust and
Cargo files. The iOS build now compiles those changes directly; no patch scripts
are applied during the build. iOS-specific paths are guarded with
`target_os = "ios"` where practical.

The older 136 MiB main-thread stack-probe bypass was disabled in the original
workflow and was deliberately not enabled during migration.
