//! Behavior checks for the headless run-loop primitives on
//! `ThreadedDispatcher`.
//!
//! These live as an integration test (not inline in the crate) because the
//! gpui lib test target embeds font assets that the telos prune removed
//! (`crates/gpui/src/svg_renderer.rs`), so `cargo test -p gpui --lib` cannot
//! compile in this tree. Integration tests link the lib without that cfg.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use gpui::{BackgroundExecutor, ForegroundExecutor, ThreadedDispatcher};

/// A headless main loop: drain queued main work, otherwise park until a
/// notification arrives (new work, a timer or background handoff, or an
/// external wake). Returns when `should_quit` turns true.
fn run_main_loop(dispatcher: &ThreadedDispatcher, should_quit: &AtomicBool) {
    loop {
        if should_quit.load(Ordering::SeqCst) {
            return;
        }
        if dispatcher.run_ready_main_tasks() {
            continue;
        }
        if should_quit.load(Ordering::SeqCst) {
            return;
        }
        dispatcher.wait_for_main_work();
    }
}

#[test]
fn headless_loop_parks_and_exits_on_external_quit() {
    let dispatcher = Arc::new(ThreadedDispatcher::new());
    let quit = Arc::new(AtomicBool::new(false));
    {
        let quit = quit.clone();
        let dispatcher = dispatcher.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            quit.store(true, Ordering::SeqCst);
            dispatcher.wake_main();
        });
    }

    let started = Instant::now();
    run_main_loop(&dispatcher, &quit);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the headless loop should wake on wake_main instead of spinning or hanging"
    );
}

#[test]
fn headless_loop_completes_background_to_main_handoff() {
    let dispatcher = Arc::new(ThreadedDispatcher::new());
    let background = BackgroundExecutor::new(dispatcher.clone());
    let foreground = ForegroundExecutor::new(dispatcher.clone());

    let (sender, receiver) = futures::channel::oneshot::channel();
    background
        .spawn(async move {
            std::thread::sleep(Duration::from_millis(10));
            sender.send(()).ok();
        })
        .detach();

    let completed = Arc::new(AtomicBool::new(false));
    foreground
        .spawn({
            let completed = completed.clone();
            async move {
                receiver.await.ok();
                completed.store(true, Ordering::SeqCst);
            }
        })
        .detach();

    let quit = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    while !completed.load(Ordering::SeqCst) {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "background to main handoff should complete through the headless loop"
        );
        if dispatcher.run_ready_main_tasks() {
            continue;
        }
        dispatcher.wait_for_main_work();
    }
    let _ = quit;
}
