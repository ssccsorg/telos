//! Deterministic test platform over [`TestDispatcher`].
//!
//! Mirrors [`super::super::headless::HeadlessPlatform`], but drives a
//! [`TestDispatcher`] so tests control virtual time, and captures platform
//! effects (clipboard, opened URLs, system notifications, restarts) for
//! assertions. Windows are never created: the headless agent build has no
//! window surface, and tests that need one are out of scope for this tree.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use futures::channel::oneshot;

use super::super::*;
use crate::platform::headless::HeadlessTextSystem;
use crate::platform::keyboard::DummyKeyboardMapper;

/// Headless test platform. Effects that tests observe are recorded in the
/// fields below; the app-facing surface never touches an OS.
pub struct TestPlatform {
    background: BackgroundExecutor,
    foreground: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>,
    quit: AtomicBool,
    pub(crate) clipboard: RefCell<Option<ClipboardItem>>,
    pub(crate) opened_urls: RefCell<Vec<String>>,
    pub(crate) notifications: RefCell<Vec<SystemNotification>>,
    pub(crate) dismissed_notifications: RefCell<Vec<String>>,
    pub(crate) app_identity: RefCell<Option<(String, String)>>,
    pub(crate) restarts: RefCell<Vec<(Option<PathBuf>, Vec<std::ffi::OsString>)>>,
    expect_restart: RefCell<Option<oneshot::Sender<(Option<PathBuf>, Vec<std::ffi::OsString>)>>>,
    pub(crate) path_prompt_requests:
        RefCell<Vec<(PathPromptOptions, oneshot::Sender<anyhow::Result<Option<Vec<PathBuf>>>>)>>,
    pub(crate) new_path_prompt_senders:
        RefCell<Vec<oneshot::Sender<anyhow::Result<Option<PathBuf>>>>>,
}

impl TestPlatform {
    /// Create a test platform driven by the given dispatcher.
    pub fn new(dispatcher: TestDispatcher) -> Rc<Self> {
        let dispatcher = Arc::new(dispatcher);
        let background = BackgroundExecutor::new(dispatcher.clone());
        let foreground = ForegroundExecutor::new(dispatcher);
        Rc::new(Self {
            background,
            foreground,
            text_system: Arc::new(HeadlessTextSystem),
            quit: AtomicBool::new(false),
            clipboard: RefCell::new(None),
            opened_urls: RefCell::new(Vec::new()),
            notifications: RefCell::new(Vec::new()),
            dismissed_notifications: RefCell::new(Vec::new()),
            app_identity: RefCell::new(None),
            restarts: RefCell::new(Vec::new()),
            expect_restart: RefCell::new(None),
            path_prompt_requests: RefCell::new(Vec::new()),
            new_path_prompt_senders: RefCell::new(Vec::new()),
        })
    }

    /// Register a receiver for the next restart request.
    pub(crate) fn expect_restart(
        &self,
    ) -> oneshot::Receiver<(Option<PathBuf>, Vec<std::ffi::OsString>)> {
        let (tx, rx) = oneshot::channel();
        *self.expect_restart.borrow_mut() = Some(tx);
        rx
    }

    /// Resolve the first pending path-picker prompt with the given selection.
    pub(crate) fn simulate_path_prompt_response(
        &self,
        select: impl FnOnce(&PathPromptOptions) -> Option<Vec<PathBuf>>,
    ) {
        let mut requests = self.path_prompt_requests.borrow_mut();
        if requests.is_empty() {
            panic!("no pending path prompt");
        }
        let (options, sender) = requests.remove(0);
        let selection = select(&options);
        let _ = sender.send(Ok(selection));
    }

    /// Resolve the first pending new-path prompt with the given selection.
    #[allow(dead_code)]
    pub(crate) fn simulate_new_path_selection(
        &self,
        select: impl FnOnce(&Path) -> Option<PathBuf>,
    ) {
        let mut senders = self.new_path_prompt_senders.borrow_mut();
        if senders.is_empty() {
            panic!("no pending new path prompt");
        }
        let sender = senders.remove(0);
        let _ = sender.send(Ok(select(Path::new("."))));
    }
}

impl Platform for TestPlatform {
    fn background_executor(&self) -> BackgroundExecutor {
        self.background.clone()
    }

    fn foreground_executor(&self) -> ForegroundExecutor {
        self.foreground.clone()
    }

    fn text_system(&self) -> Arc<dyn PlatformTextSystem> {
        self.text_system.clone()
    }

    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) {
        // Tests drive the dispatcher directly; there is no run loop.
        on_finish_launching();
    }

    fn quit(&self) {
        self.quit.store(true, Ordering::SeqCst);
    }

    fn restart(&self, binary_path: Option<PathBuf>, arguments: Vec<std::ffi::OsString>) {
        self.restarts.borrow_mut().push((binary_path.clone(), arguments.clone()));
        if let Some(tx) = self.expect_restart.borrow_mut().take() {
            let _ = tx.send((binary_path, arguments));
        }
        self.quit.store(true, Ordering::SeqCst);
    }

    fn activate(&self, _ignoring_other_apps: bool) {}
    fn hide(&self) {}
    fn hide_other_apps(&self) {}
    fn unhide_other_apps(&self) {}

    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> {
        Vec::new()
    }

    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> {
        None
    }

    fn active_window(&self) -> Option<AnyWindowHandle> {
        None
    }

    fn open_window(
        &self,
        _handle: AnyWindowHandle,
        _options: WindowParams,
    ) -> anyhow::Result<Box<dyn PlatformWindow>> {
        anyhow::bail!("test platform does not create windows")
    }

    fn window_appearance(&self) -> WindowAppearance {
        WindowAppearance::Dark
    }

    fn open_url(&self, url: &str) {
        self.opened_urls.borrow_mut().push(url.to_string());
    }

    fn on_open_urls(&self, _callback: Box<dyn FnMut(Vec<String>)>) {}

    fn register_url_scheme(&self, _url: &str) -> Task<anyhow::Result<()>> {
        Task::ready(Ok(()))
    }

    fn prompt_for_paths(
        &self,
        options: PathPromptOptions,
    ) -> oneshot::Receiver<anyhow::Result<Option<Vec<PathBuf>>>> {
        let (tx, rx) = oneshot::channel();
        self.path_prompt_requests.borrow_mut().push((options, tx));
        rx
    }

    fn prompt_for_new_path(
        &self,
        _directory: &Path,
        _suggested_name: Option<&str>,
    ) -> oneshot::Receiver<anyhow::Result<Option<PathBuf>>> {
        let (tx, rx) = oneshot::channel();
        self.new_path_prompt_senders.borrow_mut().push(tx);
        rx
    }

    fn can_select_mixed_files_and_dirs(&self) -> bool {
        true
    }

    fn reveal_path(&self, _path: &Path) {}
    fn open_with_system(&self, _path: &Path) {}
    fn on_quit(&self, _callback: Box<dyn FnMut() -> bool>) {}
    fn on_reopen(&self, _callback: Box<dyn FnMut()>) {}
    fn on_system_wake(&self, _callback: Box<dyn FnMut()>) {}

    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {}
    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {}
    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {}
    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {}
    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {}

    fn thermal_state(&self) -> ThermalState {
        ThermalState::Nominal
    }

    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {}

    fn app_path(&self) -> anyhow::Result<PathBuf> {
        Ok(PathBuf::from("."))
    }

    fn path_for_auxiliary_executable(&self, name: &str) -> anyhow::Result<PathBuf> {
        Ok(PathBuf::from(name))
    }

    fn set_cursor_style(&self, _style: CursorStyle) {}
    fn hide_cursor_until_mouse_moves(&self) {}
    fn is_cursor_visible(&self) -> bool {
        true
    }

    fn should_auto_hide_scrollbars(&self) -> bool {
        false
    }

    fn read_from_clipboard(&self) -> Option<ClipboardItem> {
        self.clipboard.borrow().clone()
    }

    fn write_to_clipboard(&self, item: ClipboardItem) {
        *self.clipboard.borrow_mut() = Some(item);
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn read_from_primary(&self) -> Option<ClipboardItem> {
        self.clipboard.borrow().clone()
    }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn write_to_primary(&self, item: ClipboardItem) {
        *self.clipboard.borrow_mut() = Some(item);
    }

    #[cfg(target_os = "macos")]
    fn read_from_find_pasteboard(&self) -> Option<ClipboardItem> {
        self.clipboard.borrow().clone()
    }

    #[cfg(target_os = "macos")]
    fn write_to_find_pasteboard(&self, item: ClipboardItem) {
        *self.clipboard.borrow_mut() = Some(item);
    }

    fn write_credentials(
        &self,
        _url: &str,
        _username: &str,
        _password: &[u8],
    ) -> Task<anyhow::Result<()>> {
        Task::ready(Ok(()))
    }

    fn read_credentials(&self, _url: &str) -> Task<anyhow::Result<Option<(String, Vec<u8>)>>> {
        Task::ready(Ok(None))
    }

    fn delete_credentials(&self, _url: &str) -> Task<anyhow::Result<()>> {
        Task::ready(Ok(()))
    }

    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> {
        Box::new(TestKeyboardLayout)
    }

    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> {
        Rc::new(DummyKeyboardMapper)
    }

    fn on_keyboard_layout_change(&self, _callback: Box<dyn FnMut()>) {}

    fn show_system_notification(&self, notification: SystemNotification) {
        self.notifications.borrow_mut().push(notification);
    }

    fn dismiss_system_notification(&self, tag: &str) {
        self.dismissed_notifications.borrow_mut().push(tag.to_string());
    }
}

struct TestKeyboardLayout;
impl PlatformKeyboardLayout for TestKeyboardLayout {
    fn id(&self) -> &str {
        "test"
    }

    fn name(&self) -> &str {
        "Test"
    }
}
