# gpui

Headless application and entity runtime for the SSCCS agent stack.

This is a fork of Zed's GPUI with the visual UI layer removed. What
remains is the machinery a headless agent runs on: the reactive entity
graph (`Entity`, `Context`, `App`), foreground and background executors,
global settings and stores, and the platform abstraction reduced to
`HeadlessPlatform`, a real-time, window-free dispatcher. The agent
binary (`crates/telos`) is rooted on this runtime and never builds a
windowing backend.

## Runtime shape

A headless app starts with an explicit platform:

```rust,no_run
use gpui::{Application, HeadlessPlatform};

fn main() {
    let app = Application::with_platform(HeadlessPlatform::new());
    app.run(|cx| {
        // cx: &mut App
    });
}
```

Entities, contexts, observers, executors, and globals behave as in
upstream GPUI. Windows, scenes, elements, and styling are gone: their
source remains only behind the disabled `ui` feature and does not
compile into this tree.

## Tests

Unit tests run on a deterministic headless harness:

```rust
#[gpui::test]
async fn test_something(cx: &mut TestAppContext) { /* ... */ }
```

`TestAppContext` builds an `App` over `TestPlatform`, a window-free
platform driven by the virtual-clock `TestDispatcher`. The agent,
terminal core, and MCP suites in this workspace run on it.
