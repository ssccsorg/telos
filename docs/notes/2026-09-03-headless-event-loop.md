# Headless event loop research

Date: 2026-09
Branch: 3-gpui-coupling-scope
Issue: ssccsorg/telos#3

## Question

Replacing the OS event loop (`Platform::run`) with headless driving risks
changing task scheduling order, timer precision, and cancellation timing.
How much of gpui's scheduling actually depends on the OS loop, and can a
headless runtime be equal or better?

## Findings

### Scheduling architecture

- gpui executors (`ForegroundExecutor`, `BackgroundExecutor`) are built on
  the `scheduler` crate. Task semantics (priority, sessions, ordering,
  cancellation, LocalExecutor) live there and are dispatcher-independent.
- Production `PlatformScheduler` delegates the three mechanical jobs to a
  `PlatformDispatcher`:
  - `dispatch` (background): mac GCD global queue, linux thread pool
  - `dispatch_on_main_thread` (foreground): mac GCD main queue
  - `dispatch_after` (timers): platform timer (note in code: ~15.6 ms
    resolution on Windows)
  - plus `now` (clock)
- The OS loop contributes only these dispatch mechanics, nothing else. The
  whole OS dependency is one trait of about ten methods.

### An OS-free dispatcher already exists

`ThreadedDispatcher` (`crates/gpui/src/platform/threaded_dispatcher.rs`) is
a complete `PlatformDispatcher` with production concurrency and no OS loop:

- background: parallel worker pool
- timers: real time on a dedicated timer thread (`std::time::Instant`),
  no OS coalescing
- foreground: queued, drained by the owning thread via `run_until_idle`

It is the dispatcher behind `BenchAppContext`
(`crates/gpui/src/app/bench_context.rs`), which runs real gpui applications
with production concurrency and real timers, no run loop. That is an
existence proof for a headless runtime.

## Conclusion

The scheduling risk is bounded to the dispatcher seam, and a production
dispatcher without an OS loop already exists. A telos headless runtime
built on `ThreadedDispatcher` keeps the same `scheduler` semantics and is
expected to be neutral-to-better:

- foreground dispatch becomes a local queue pop instead of an OS event
  round trip
- timers run on a dedicated thread with plain `std::time`, avoiding OS
  timer coalescing (the 15.6 ms Windows note in the platform code is
  irrelevant)
- background pool semantics are unchanged

Not changed: task ordering and cancellation semantics, which come from the
`scheduler` crate above the dispatcher.

## Plan

1. Spike: prototype a telos runtime entry on `ThreadedDispatcher` in the
   style of `BenchAppContext` (a `Platform` whose window methods no-op or
   panic, foreground drained by a park-on-idle loop). Verify with
   `cargo check -p telos` and the actus fake conformance tier.
2. If the spike holds, gate the UI cluster behind the gpui `ui` feature
   (default on) and switch telos to the headless runtime; windowed platform
   crates (`gpui_macos`, `gpui_linux`) then leave the telos graph.
3. Measure before/after: binary size, task throughput, timer precision,
   conformance result. Claim no perf win without the measurement.

Open risks to check in the spike: long-running server loop must park when
idle (no busy spin), and `run_headless` must not depend on window events.
