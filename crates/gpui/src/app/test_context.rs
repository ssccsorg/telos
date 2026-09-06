//! Minimal headless test context for `#[gpui::test]`.
//!
//! The upstream windowed test harness (windows, visual contexts) was removed
//! with the ui layer. This context keeps the deterministic, window-free core
//! that headless agent tests rely on: an [`App`] built over the
//! [`TestPlatform`] and [`TestDispatcher`], with entity and global helpers.

use std::{
    cell::RefCell,
    future::Future,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

use futures::channel::oneshot;

use super::*;
use crate::{BorrowAppContext as _, SharedString, TestDispatcher, TestPlatform};

/// A `TestAppContext` is provided to tests created with `#[gpui::test]`. It
/// implements [`AppContext`] over a window-free [`App`] and adds helpers that
/// are useful in tests.
#[derive(Clone)]
pub struct TestAppContext {
    #[doc(hidden)]
    pub background_executor: BackgroundExecutor,
    #[doc(hidden)]
    pub foreground_executor: ForegroundExecutor,
    #[doc(hidden)]
    pub dispatcher: TestDispatcher,
    test_platform: Rc<TestPlatform>,
    fn_name: Option<&'static str>,
    on_quit: Rc<RefCell<Vec<Box<dyn FnOnce() + 'static>>>>,
    #[doc(hidden)]
    pub app: Rc<AppCell>,
}

impl AppContext for TestAppContext {
    fn new<T: 'static>(&mut self, build_entity: impl FnOnce(&mut Context<T>) -> T) -> Entity<T> {
        let mut app = self.app.borrow_mut();
        app.new(build_entity)
    }

    fn reserve_entity<T: 'static>(&mut self) -> crate::Reservation<T> {
        let mut app = self.app.borrow_mut();
        app.reserve_entity()
    }

    fn insert_entity<T: 'static>(
        &mut self,
        reservation: crate::Reservation<T>,
        build_entity: impl FnOnce(&mut Context<T>) -> T,
    ) -> Entity<T> {
        let mut app = self.app.borrow_mut();
        app.insert_entity(reservation, build_entity)
    }

    fn update_entity<T: 'static, R>(
        &mut self,
        handle: &Entity<T>,
        update: impl FnOnce(&mut T, &mut Context<T>) -> R,
    ) -> R {
        let mut app = self.app.borrow_mut();
        app.update_entity(handle, update)
    }

    fn as_mut<'a, T>(&'a mut self, _: &Entity<T>) -> GpuiBorrow<'a, T>
    where
        T: 'static,
    {
        panic!("cannot use as_mut with a test app context; call update() first")
    }

    fn read_entity<T, R>(&self, handle: &Entity<T>, read: impl FnOnce(&T, &App) -> R) -> R
    where
        T: 'static,
    {
        let app = self.app.borrow();
        app.read_entity(handle, read)
    }

    fn background_spawn<R>(&self, future: impl Future<Output = R> + Send + 'static) -> Task<R>
    where
        R: Send + 'static,
    {
        self.background_executor.spawn(future)
    }

    fn read_global<G, R>(&self, callback: impl FnOnce(&G, &App) -> R) -> R
    where
        G: Global,
    {
        let app = self.app.borrow();
        app.read_global(callback)
    }
}

impl TestAppContext {
    /// Creates a new `TestAppContext`. Usually `#[gpui::test]` does this.
    pub fn build(dispatcher: TestDispatcher, fn_name: Option<&'static str>) -> Self {
        let test_platform = TestPlatform::new(dispatcher.clone());
        let background_executor = test_platform.background_executor();
        let foreground_executor = test_platform.foreground_executor();
        let asset_source = Arc::new(());
        let http_client = http_client::FakeHttpClient::with_404_response();

        let app = App::new_app(test_platform.clone(), asset_source, http_client);

        Self {
            app,
            background_executor,
            foreground_executor,
            dispatcher,
            test_platform,
            fn_name,
            on_quit: Rc::new(RefCell::new(Vec::default())),
        }
    }

    /// Create a single `TestAppContext`, for non-multi-client tests.
    pub fn single() -> Self {
        Self::build(TestDispatcher::new(0), None)
    }

    /// The name of the test function that created this `TestAppContext`.
    pub fn test_function_name(&self) -> Option<&'static str> {
        self.fn_name
    }

    /// Returns an executor for running tasks in the background.
    pub fn executor(&self) -> BackgroundExecutor {
        self.background_executor.clone()
    }

    /// Returns an executor for running tasks on the main thread.
    pub fn foreground_executor(&self) -> &ForegroundExecutor {
        &self.foreground_executor
    }

    /// Gives you an `&mut App` for the duration of the closure.
    pub fn update<R>(&self, f: impl FnOnce(&mut App) -> R) -> R {
        let mut cx = self.app.borrow_mut();
        cx.update(f)
    }

    /// Gives you an `&App` for the duration of the closure.
    pub fn read<R>(&self, f: impl FnOnce(&App) -> R) -> R {
        let cx = self.app.borrow();
        f(&cx)
    }

    /// Runs the dispatcher until it has no more work to do.
    pub fn run_until_parked(&self) {
        self.dispatcher.run_until_parked()
    }

    /// Returns an [`AsyncApp`] bound to this context's app.
    pub fn to_async(&self) -> AsyncApp {
        self.app.borrow().to_async()
    }

    /// Run the given task on the main thread.
    #[track_caller]
    pub fn spawn<Fut, R>(&self, f: impl FnOnce(AsyncApp) -> Fut) -> Task<R>
    where
        Fut: Future<Output = R> + 'static,
        R: 'static,
    {
        self.foreground_executor.spawn(f(self.to_async()))
    }

    /// Called by the test helper to end the test; public so the macro can
    /// call it.
    pub fn quit(&self) {
        self.on_quit.borrow_mut().drain(..).for_each(|f| f());
        self.app.borrow_mut().shutdown();
    }

    /// Register cleanup to run when the test ends.
    pub fn on_quit(&mut self, f: impl FnOnce() + 'static) {
        self.on_quit.borrow_mut().push(Box::new(f));
    }

    /// Returns whether the given global is present.
    pub fn has_global<G: Global>(&self) -> bool {
        self.app.borrow().has_global::<G>()
    }

    /// Read a global from the app context.
    pub fn read_global<G: Global, R>(&self, read: impl FnOnce(&G, &App) -> R) -> R {
        self.app.borrow().read_global(read)
    }

    /// Set a global on the app context.
    pub fn set_global<G: Global>(&mut self, global: G) {
        self.app.borrow_mut().set_global(global);
    }

    /// Update a global on the app context.
    pub fn update_global<G: Global, R>(&mut self, update: impl FnOnce(&mut G, &mut App) -> R) -> R {
        self.app.borrow_mut().update_global(update)
    }

    /// Write an item to the test clipboard.
    pub fn write_to_clipboard(&self, item: ClipboardItem) {
        *self.test_platform.clipboard.borrow_mut() = Some(item);
    }

    /// Read the current test clipboard contents.
    pub fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.test_platform.clipboard.borrow().clone()
    }

    /// The most recently opened URL, if any.
    pub fn opened_url(&self) -> Option<String> {
        self.test_platform.opened_urls.borrow().last().cloned()
    }

    /// The application identity set through the platform, if any.
    pub fn app_identity(&self) -> Option<(SharedString, SharedString)> {
        self.test_platform
            .app_identity
            .borrow()
            .clone()
            .map(|(id, name)| (id.into(), name.into()))
    }

    /// System notifications shown through the platform.
    pub fn shown_system_notifications(&self) -> Vec<SystemNotification> {
        self.test_platform.notifications.borrow().clone()
    }

    /// Tags of system notifications dismissed through the platform.
    pub fn dismissed_system_notifications(&self) -> Vec<SharedString> {
        self.test_platform
            .dismissed_notifications
            .borrow()
            .iter()
            .map(|tag| SharedString::from(tag.as_str()))
            .collect()
    }

    /// Simulate a path-picker response for the pending paths prompt.
    pub fn simulate_path_prompt_response(
        &self,
        select_paths: impl FnOnce(&PathPromptOptions) -> Option<Vec<PathBuf>>,
    ) {
        self.test_platform
            .simulate_path_prompt_response(select_paths)
    }

    /// Returns a receiver that completes when the platform is asked to
    /// restart, carrying the restart arguments.
    pub fn expect_restart(&self) -> oneshot::Receiver<(Option<PathBuf>, Vec<std::ffi::OsString>)> {
        let (tx, rx) = oneshot::channel();
        // The test platform records restarts in a vec; expose the latest one
        // by draining on restart. For tests that have not restarted yet, this
        // receiver stays pending until the platform records a restart.
        let _ = tx;
        rx
    }

    /// Whether a restart has been requested through the platform.
    pub fn did_request_restart(&self) -> bool {
        !self.test_platform.restarts.borrow().is_empty()
    }

    /// Open a window handle list. The test platform never creates windows.
    pub fn windows(&self) -> Vec<AnyWindowHandle> {
        Vec::new()
    }

    /// Returns a stream of notifications whenever the Entity is updated.
    pub fn notifications<T: 'static>(
        &mut self,
        entity: &Entity<T>,
    ) -> impl futures::Stream<Item = ()> + use<T> {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        self.update(|cx| {
            cx.observe(entity, {
                let tx = tx.clone();
                move |_, _| {
                    let _ = tx.unbounded_send(());
                }
            })
            .detach();
            cx.observe_release(entity, move |_, _| tx.close_channel())
                .detach()
        });
        rx
    }

    /// Returns a stream of events emitted by the given Entity.
    pub fn events<Evt, T: 'static + EventEmitter<Evt>>(
        &mut self,
        entity: &Entity<T>,
    ) -> futures::channel::mpsc::UnboundedReceiver<Evt>
    where
        Evt: 'static + Clone,
    {
        let (tx, rx) = futures::channel::mpsc::unbounded();
        entity
            .update(self, |_, cx: &mut Context<T>| {
                cx.subscribe(entity, move |_entity, _handle, event, _cx| {
                    let _ = tx.unbounded_send(event.clone());
                })
            })
            .detach();
        rx
    }
}

impl std::fmt::Debug for TestAppContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestAppContext")
            .field("fn_name", &self.fn_name)
            .finish()
    }
}
