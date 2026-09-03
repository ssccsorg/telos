# gpui runtime usage verification

Date: 2026-09-03
Branch: 3-gpui-coupling-scope
Issue: ssccsorg/telos#3

## Question

Issue #3 proposed verifying whether `settings_content` and `terminal` are
UI-only crates that could be gated or pruned from the headless telos
binary.

## Result

Both crates are load-bearing in the agent runtime. Pruning or gating them
would remove agent features.

### terminal is the agent shell tool

- `acp_thread/src/terminal.rs` implements the ACP terminal tool
  (`create_terminal`, `kill_terminal`, `release_terminal`,
  `terminal_output`, `wait_for_terminal_exit`) over
  `Entity<terminal::Terminal>` with OS-level sandboxing (seccomp /
  Bubblewrap on Linux, Seatbelt on macOS).
- `agent_servers/src/acp.rs` builds terminals through
  `terminal::TerminalBuilder` and reads
  `terminal::terminal_settings::{AlternateScroll, CursorShape}`.
- `project/src/terminals.rs` keeps the project-local terminal handles.

The terminal UI-ish gpui symbols (MouseButton, Modifiers, Keystroke,
Pixels, ScrollDelta) belong to emulator input handling that the tool
path exercises, not to dead rendering code.

### settings_content is the settings data model

- `settings/src/settings_store.rs` parses user and global settings JSON
  through `settings_content` types (`CommandAliasTarget`, `ParseStatus`,
  `PlatformOverrides`, `ReleaseChannelOverrides`) at store
  construction (`from_settings_content`).
- `migrator/src/migrations.rs` uses `settings_content` overrides to
  migrate older settings.

The gpui value types seen in `settings_content` (Rgba, Pixels,
FontFeatures, WindowButtonLayout) are settings values encoded as gpui
types, which is the data-model coupling described as Stream 2 in issue
#3, not removable UI.

## Conclusion

Stream 1 of issue #3 (gate or prune settings_content and terminal) is
void: there is nothing to prune. The remaining gpui coupling in the
headless graph is the runtime primitive layer (App, Entity, Task,
AsyncApp, Context, Subscription, SharedString) plus the data-model type
leakage into language/theme/syntax_theme/project/settings (Stream 2).
Stream 2 work is gated on the upstream-sync posture decision, per issue
#2 and issue #3.
