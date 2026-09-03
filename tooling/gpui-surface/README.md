# gpui surface measurement

Answers: which parts of the vendored `gpui` crate does the headless agent
graph actually use in production (cfg(test) excluded)?

## Usage

    cargo tree -p telos --target x86_64-unknown-linux-gnu --prefix none \
      | grep -oE '/crates/[a-z0-9_]+' | sed 's#.*/##' | sort -u \
      | python3 tooling/gpui-surface/scan.py > tooling/gpui-surface/manifest.txt

The manifest is committed; regenerate it after every dependency change.

## Measured result (2026-09)

101 sourced crates, 107 distinct gpui items. Production usage concentrates
on the runtime layer and value types:

- runtime primitives: App, AppContext, AsyncApp, Context, Entity,
  WeakEntity, Task, BackgroundExecutor, Global, Subscription,
  EventEmitter, SubscriberSet, SharedString, px/point helpers
- value types (Stream 2 data-model leakage): Hsla, Rgba, HighlightStyle,
  FontStyle, FontWeight, Pixels, Point, Rems, rgb/red/green/... helpers
- exceptions to verify per wave: `StyledText` (used by
  `language/src/buffer.rs`), `Action`/keymap leftovers inside test-only
  code, platform crates (`gpui_linux`, `gpui_macos`) referencing
  window/scene/text types

No production use was found (direct-path scan) for: window, scene,
elements/div, text_system rendering, svg_renderer, img, interactive,
key_dispatch, gestures, input, tab_stop, path_builder, spring, arena,
bounds_tree, debug_overlay, inspector, queue, shared_uri, asset_cache,
action/keymap/register_action.

## Cut waves

Wave membership is confirmed at execution time by compile errors plus the
item manifest; each wave lands as one PR verified by the LLM-free gate.

1. Wave 1: delete interaction cluster with zero production sites (input,
   gestures, key_dispatch, tab_stop, path_builder, spring, arena,
   bounds_tree, debug_overlay, inspector, queue, shared_uri,
   asset_cache) after an internal-reference grep inside gpui.
2. Wave 2: action/keymap/actions macro surface, after relocating or
   cfg(test)-gating the test-only users in settings and zed_actions.
3. Wave 3: rendering stack (svg_renderer, img, scene, elements, styled,
   text_system raster path, taffy) plus their Cargo deps (resvg/usvg,
   image codecs, fontdb), once platform crates no longer reference them.
4. Wave 4: value-type extraction (Hsla/Rgba/HighlightStyle/font/geometry)
   to a telos-owned crate, gpui re-exports during transition.
5. Wave 5: rename the remaining crate (runtime primitives only) from
   `gpui` to a telos-owned name via a dependency alias.
