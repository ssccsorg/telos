# gpui surface measurement

Answers: which parts of the vendored `gpui` crate does the headless agent
graph actually use in production (cfg(test) excluded)?

## Usage

    cargo tree -p telos --target x86_64-unknown-linux-gnu --prefix none \
      | grep -oE '/crates/[a-z0-9_]+' | sed 's#.*/##' | sort -u \
      > /tmp/telos-reachable.txt
    python3 tooling/gpui-surface/scan.py < /tmp/telos-reachable.txt \
      > tooling/gpui-surface/manifest.txt
    python3 tooling/gpui-surface/contacts.py < /tmp/telos-reachable.txt \
      > tooling/gpui-surface/contacts.txt

Both outputs are committed; regenerate them after every dependency change.

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

## Wave 3 contact surface (measured 2026-09)

`contacts.txt` records the consumer side of the Wave 3 gate set: for each
reachable crate file, the gpui UI-cluster items it references and the gpui
module that defines them. It answers, per UI-cluster module, which
consumer sites keep it compiled. Contact removal happens at these
consumer sites (the gpui contact surface), so the gpui-side change for a
module is a whole-module gate instead of item-level surgery.

Measured contact by module:

- window.rs: feature_flags, language_model, project (image_store,
  project), session (WindowId), terminal, theme_settings, zed_actions
- view.rs: language_model, zed_actions
- styled.rs: theme (Styled)
- element.rs: settings/editable_setting_control (RenderOnce)
- elements/list.rs: acp_thread, agent (ListOffset)
- elements/text.rs: language/buffer (StyledText)
- elements/img.rs: project/image_store (Img, ImageSource,
  ImageCacheError)
- assets.rs / asset_cache.rs: project/image_store, prompt_store, theme
  (AssetSource, Asset, AssetLogger, RenderImage, size)
- shared_uri.rs: client (SharedUri)
- interactive.rs: terminal (input events, kept for the emulator input
  path per the 2026-09-03 usage note)
- keymap/ and action leftovers: settings/keymap_file, zed_actions
- text_system and style.rs rows are Stream 2 value types (Font*,
  HighlightStyle, TextStyle, ObjectFit) that stay as data; only the
  layout/render half of these modules is in the gate set.

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
   Execution order is contact-driven: for each module in the gate set,
   remove the consumer contacts in `contacts.txt` first (consumer-side
   edits), then gate the whole module behind the `ui` feature. Progress
   is tracked per commit on the wave branch.
4. Wave 4: value-type extraction (Hsla/Rgba/HighlightStyle/font/geometry)
   to a telos-owned crate, gpui re-exports during transition.
5. Wave 5: rename the remaining crate (runtime primitives only) from
   `gpui` to a telos-owned name via a dependency alias.

## UI cluster cut design (Wave 3)

Boundary facts (measured 2026-09):

- `gpui` itself does not depend on the platform crates. The windowing
  backends enter the graph through `gpui_platform`, which compiles
  `gpui_macos` on mac (unconditional) and `gpui_linux` on linux.
  `current_platform(headless=true)` still compiles the full windowed
  backend crate, so window/scene/elements/text/svg stay reachable in every
  telos build.
- Existing headless paths do not cover a runtime: `current_headless_renderer`
  is a Metal-based text-shaping test helper on mac, not a runtime platform.

Design: gate the UI cluster behind a gpui `ui` feature (default off) and
stop compiling the windowed platform crates for telos.

1. gpui: add `ui` feature; cfg-gate `window`, `scene`, `elements/`,
   `styled`, `view`, `spring` (+`elements/animation`), `text_system`
   (render side), `svg_renderer`, `style` layout half; gate their Cargo
   deps (`resvg`/`usvg`, `image` codecs, `fontdb`, `taffy`).
2. Platform gate: `gpui_platform` gains `ui` = ["gpui/ui",
   "gpui_macos", "gpui_linux"]; the platform crate deps become optional.
3. Headless runtime for telos: implement a minimal `Platform` (executor
   only) in gpui_platform or gpui core, driven by gpui_tokio; window/menu
   methods panic or no-op. Reference skeleton:
   `crates/gpui/src/platform/test/platform.rs`. The `Platform` trait has
   163 required fns; most can no-op, the load-bearing ones are
   `background_executor`, `foreground_executor`, `run`, `quit`,
   `now`/timers.
4. telos switches from `current_platform(true)` to the headless platform.
5. Verify ui=off: `cargo tree -p telos` shows no resvg/usvg/image/fontdb/
   taffy; gate image drops `libfontconfig-dev`.

Open question for the spike: how the foreground executor is driven without
an OS event loop (`Platform::run` + `threaded_dispatcher` vs gpui_tokio
driving). Resolve before step 1.
