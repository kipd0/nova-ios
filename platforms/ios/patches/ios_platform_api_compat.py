#!/usr/bin/env python3
"""Adapt desktop-only Horizon startup and keyboard APIs for an iOS checkout.

The upstream game stays unchanged in Git. This runner-only patch leaves the
desktop codepaths intact and uses the APIs available in winit/UIKit on iOS.
"""

from pathlib import Path


def replace_once(filename: str, old: str, new: str) -> None:
    path = Path(filename)
    source = path.read_text(encoding="utf-8")
    count = source.count(old)
    assert count == 1, (
        f"{filename}: upstream API changed; expected one {old[:90]!r}, got {count}"
    )
    path.write_text(source.replace(old, new, 1), encoding="utf-8")


entry = "crates/horizon-client/src/entry.rs"
# The runtime holder is a desktop process/lease protocol and is intentionally
# not exported by horizon-mod-package on iOS. The iOS host prepares its own
# private resources and starts the game in-process instead.
replace_once(
    entry,
    "    horizon_mod_package::holder::enter();",
    '    #[cfg(not(target_os = "ios"))]\n'
    "    horizon_mod_package::holder::enter();",
)
replace_once(
    entry,
    "    let capture = horizon_mod_package::holder::acquire(&launch);",
    '    #[cfg(not(target_os = "ios"))]\n'
    "    let capture = horizon_mod_package::holder::acquire(&launch);\n"
    '    #[cfg(target_os = "ios")]\n'
    "    let capture = None;",
)

handler = "crates/horizon-client/src/game/runtime/window_handler.rs"
replace_once(
    handler,
    "            use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;",
    '            #[cfg(not(target_os = "ios"))]\n'
    "            use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;",
)
replace_once(
    handler,
    """            let layout_key = horizon_client::keybinds::layout_key_name(
                event.physical_key,
                &event.key_without_modifiers(),
            );""",
    """            #[cfg(target_os = "ios")]
            let layout_key = horizon_client::keybinds::layout_key_name(
                event.physical_key,
                &event.logical_key,
            );
            #[cfg(not(target_os = "ios"))]
            let layout_key = horizon_client::keybinds::layout_key_name(
                event.physical_key,
                &event.key_without_modifiers(),
            );""",
)

# The game facade retains the desktop holder capability marker for launcher
# detection. iOS has no holder module and starts in-process from Nova's host.
replace_once(
    "crates/horizon-game/src/lib.rs",
    "    std::hint::black_box(&horizon_mod_package::holder::CAPABILITY);",
    '    #[cfg(not(target_os = "ios"))]\n'
    "    std::hint::black_box(&horizon_mod_package::holder::CAPABILITY);",
)

# Only desktop builds provide the process-holder used by lease_units.
# Keep Horizon's committed sources intact; hide this desktop-only function
# from the iOS compiler alongside the holder module it depends on.
replace_once(
    "crates/horizon-mod-package/src/runtime_leases.rs",
    "pub fn lease_units(",
    '#[cfg(any(target_os = "linux", target_os = "macos", windows))]\n'
    "pub fn lease_units(",
)

print("Nova iOS: desktop-only holder, lease, and modifier APIs guarded for iOS")
