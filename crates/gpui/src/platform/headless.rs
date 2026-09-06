//! Minimal headless platform over [`ThreadedDispatcher`].
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use super::*;
use crate::{BackgroundExecutor, ForegroundExecutor, ThreadedDispatcher};
/// Headless platform backed by a threaded dispatcher, used by headless apps and tests.
pub struct HeadlessPlatform {
    dispatcher: Arc<ThreadedDispatcher>,
    background: BackgroundExecutor, foreground: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>, quit: AtomicBool,
}
impl HeadlessPlatform {
    /// Create a headless platform with its own threaded dispatcher.
    pub fn new() -> Rc<Self> {
        let dispatcher = Arc::new(ThreadedDispatcher::new());
        let background = BackgroundExecutor::new(dispatcher.clone());
        let foreground = ForegroundExecutor::new(dispatcher.clone());
        Rc::new(Self { text_system: Arc::new(HeadlessTextSystem), dispatcher, background, foreground, quit: AtomicBool::new(false) })
    }
    fn run_loop(&self, on_finish_launching: Box<dyn FnOnce()>) {
        on_finish_launching();
        loop {
            if self.quit.load(Ordering::SeqCst) { break; }
            if self.dispatcher.run_ready_main_tasks() { continue; }
            if self.quit.load(Ordering::SeqCst) { break; }
            self.dispatcher.wait_for_main_work();
        }
    }
}
impl Platform for HeadlessPlatform {
    fn background_executor(&self) -> BackgroundExecutor { self.background.clone() }
    fn foreground_executor(&self) -> ForegroundExecutor { self.foreground.clone() }
    fn text_system(&self) -> Arc<dyn PlatformTextSystem> { self.text_system.clone() }
    fn run(&self, on_finish_launching: Box<dyn 'static + FnOnce()>) { self.run_loop(on_finish_launching) }
    fn quit(&self) { self.quit.store(true, Ordering::SeqCst); self.dispatcher.wake_main(); }
    fn restart(&self, _binary_path: Option<PathBuf>, _arguments: Vec<OsString>) {  }
    fn activate(&self, _ignoring_other_apps: bool) {  }
    fn hide(&self) {  }
    fn hide_other_apps(&self) {  }
    fn unhide_other_apps(&self) {  }
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> { Vec::new() }
    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> { None }
    fn active_window(&self) -> Option<AnyWindowHandle> { None }
    fn open_window( &self, _handle: AnyWindowHandle, _options: WindowParams, ) -> anyhow::Result<Box<dyn PlatformWindow>> { todo!("headless: open_window") }
    fn window_appearance(&self) -> WindowAppearance { WindowAppearance::Dark }
    fn open_url(&self, _url: &str) {  }
    fn on_open_urls(&self, _callback: Box<dyn FnMut(Vec<String>)>) {  }
    fn register_url_scheme(&self, _url: &str) -> Task<Result<()>> { todo!("headless: register_url_scheme") }
    fn prompt_for_paths( &self, _options: PathPromptOptions, ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> { todo!("headless: prompt_for_paths") }
    fn prompt_for_new_path( &self, _directory: &Path, _suggested_name: Option<&str>, ) -> oneshot::Receiver<Result<Option<PathBuf>>> { todo!("headless: prompt_for_new_path") }
    fn can_select_mixed_files_and_dirs(&self) -> bool { todo!("headless: can_select_mixed_files_and_dirs") }
    fn reveal_path(&self, _path: &Path) {  }
    fn open_with_system(&self, _path: &Path) {  }
    fn on_quit(&self, _callback: Box<dyn FnMut() -> bool>) {  }
    fn on_reopen(&self, _callback: Box<dyn FnMut()>) {  }
    fn on_system_wake(&self, _callback: Box<dyn FnMut()>) {  }
    fn set_menus(&self, _menus: Vec<Menu>, _keymap: &Keymap) {  }
    fn set_dock_menu(&self, _menu: Vec<MenuItem>, _keymap: &Keymap) {  }
    fn on_app_menu_action(&self, _callback: Box<dyn FnMut(&dyn Action)>) {  }
    fn on_will_open_app_menu(&self, _callback: Box<dyn FnMut()>) {  }
    fn on_validate_app_menu_command(&self, _callback: Box<dyn FnMut(&dyn Action) -> bool>) {  }
    fn thermal_state(&self) -> ThermalState { todo!("headless: thermal_state") }
    fn on_thermal_state_change(&self, _callback: Box<dyn FnMut()>) {  }
    fn app_path(&self) -> Result<PathBuf> { todo!("headless: app_path") }
    fn path_for_auxiliary_executable(&self, _name: &str) -> Result<PathBuf> { todo!("headless: path_for_auxiliary_executable") }
    fn set_cursor_style(&self, _style: CursorStyle) {  }
    fn hide_cursor_until_mouse_moves(&self) {  }
    fn is_cursor_visible(&self) -> bool { todo!("headless: is_cursor_visible") }
    fn should_auto_hide_scrollbars(&self) -> bool { false }
    fn read_from_clipboard(&self) -> Option<ClipboardItem> { todo!("headless: read_from_clipboard") }
    fn write_to_clipboard(&self, _item: ClipboardItem) {  }

    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn read_from_primary(&self) -> Option<ClipboardItem> { None }
    #[cfg(any(target_os = "linux", target_os = "freebsd"))]
    fn write_to_primary(&self, item: ClipboardItem) { _ = item; }

    #[cfg(target_os = "macos")]
    fn read_from_find_pasteboard(&self) -> Option<ClipboardItem> { todo!("headless: read_from_find_pasteboard") }
    #[cfg(target_os = "macos")]
    fn write_to_find_pasteboard(&self, item: ClipboardItem) { _ = item; }
    fn write_credentials(&self, _url: &str, _username: &str, _password: &[u8]) -> Task<Result<()>> { todo!("headless: write_credentials") }
    fn read_credentials(&self, _url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> { todo!("headless: read_credentials") }
    fn delete_credentials(&self, _url: &str) -> Task<Result<()>> { todo!("headless: delete_credentials") }
    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> { Box::new(HeadlessKeyboardLayout) }
    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> { Rc::new(DummyKeyboardMapper) }
    fn on_keyboard_layout_change(&self, _callback: Box<dyn FnMut()>) {  }
}
pub struct HeadlessTextSystem;
impl PlatformTextSystem for HeadlessTextSystem {
    fn add_fonts(&self, _fonts: Vec<Cow<'static, [u8]>>) -> Result<()> { todo!("headless text system: add_fonts") }
    fn all_font_names(&self) -> Vec<String> { todo!("headless text system: all_font_names") }
    fn font_id(&self, _descriptor: &Font) -> Result<FontId> { todo!("headless text system: font_id") }
    fn font_metrics(&self, _font_id: FontId) -> FontMetrics { todo!("headless text system: font_metrics") }
    fn typographic_bounds(&self, _font_id: FontId, _glyph_id: GlyphId) -> Result<Bounds<f32>> { todo!("headless text system: typographic_bounds") }
    fn advance(&self, _font_id: FontId, _glyph_id: GlyphId) -> Result<Size<f32>> { todo!("headless text system: advance") }
    fn glyph_for_char(&self, _font_id: FontId, _ch: char) -> Option<GlyphId> { todo!("headless text system: glyph_for_char") }
    fn glyph_raster_bounds(&self, _params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> { todo!("headless text system: glyph_raster_bounds") }
    fn rasterize_glyph( &self, _params: &RenderGlyphParams, _raster_bounds: Bounds<DevicePixels>, ) -> Result<(Size<DevicePixels>, Vec<u8>)> { todo!("headless text system: rasterize_glyph") }
    fn layout_line(&self, _text: &str, _font_size: Pixels, _runs: &[FontRun]) -> LineLayout { todo!("headless text system: layout_line") }
    fn recommended_rendering_mode(&self, _font_id: FontId, _font_size: Pixels) -> TextRenderingMode { todo!("headless text system: recommended_rendering_mode") }
}

struct HeadlessKeyboardLayout;
impl PlatformKeyboardLayout for HeadlessKeyboardLayout {
    fn id(&self) -> &str { "headless" }
    fn name(&self) -> &str { "Headless" }
}
