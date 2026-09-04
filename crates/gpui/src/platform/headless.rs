//! Minimal headless platform over [`ThreadedDispatcher`].
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use super::*;
use crate::{BackgroundExecutor, ForegroundExecutor, ThreadedDispatcher};
pub struct HeadlessPlatform {
    dispatcher: Arc<ThreadedDispatcher>,
    background: BackgroundExecutor, foreground: ForegroundExecutor,
    text_system: Arc<dyn PlatformTextSystem>, quit: AtomicBool,
}
impl HeadlessPlatform {
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
    fn restart(&self, binary_path: Option<PathBuf>, arguments: Vec<OsString>) {  }
    fn activate(&self, ignoring_other_apps: bool) {  }
    fn hide(&self) {  }
    fn hide_other_apps(&self) {  }
    fn unhide_other_apps(&self) {  }
    fn displays(&self) -> Vec<Rc<dyn PlatformDisplay>> { Vec::new() }
    fn primary_display(&self) -> Option<Rc<dyn PlatformDisplay>> { None }
    fn active_window(&self) -> Option<AnyWindowHandle> { None }
    fn open_window( &self, handle: AnyWindowHandle, options: WindowParams, ) -> anyhow::Result<Box<dyn PlatformWindow>> { todo!("headless: open_window") }
    fn window_appearance(&self) -> WindowAppearance { WindowAppearance::Dark }
    fn open_url(&self, url: &str) {  }
    fn on_open_urls(&self, callback: Box<dyn FnMut(Vec<String>)>) {  }
    fn register_url_scheme(&self, url: &str) -> Task<Result<()>> { todo!("headless: register_url_scheme") }
    fn prompt_for_paths( &self, options: PathPromptOptions, ) -> oneshot::Receiver<Result<Option<Vec<PathBuf>>>> { todo!("headless: prompt_for_paths") }
    fn prompt_for_new_path( &self, directory: &Path, suggested_name: Option<&str>, ) -> oneshot::Receiver<Result<Option<PathBuf>>> { todo!("headless: prompt_for_new_path") }
    fn can_select_mixed_files_and_dirs(&self) -> bool { todo!("headless: can_select_mixed_files_and_dirs") }
    fn reveal_path(&self, path: &Path) {  }
    fn open_with_system(&self, path: &Path) {  }
    fn on_quit(&self, callback: Box<dyn FnMut() -> bool>) {  }
    fn on_reopen(&self, callback: Box<dyn FnMut()>) {  }
    fn on_system_wake(&self, callback: Box<dyn FnMut()>) {  }
    fn set_menus(&self, menus: Vec<Menu>, keymap: &Keymap) {  }
    fn set_dock_menu(&self, menu: Vec<MenuItem>, keymap: &Keymap) {  }
    fn on_app_menu_action(&self, callback: Box<dyn FnMut(&dyn Action)>) {  }
    fn on_will_open_app_menu(&self, callback: Box<dyn FnMut()>) {  }
    fn on_validate_app_menu_command(&self, callback: Box<dyn FnMut(&dyn Action) -> bool>) {  }
    fn thermal_state(&self) -> ThermalState { todo!("headless: thermal_state") }
    fn on_thermal_state_change(&self, callback: Box<dyn FnMut()>) {  }
    fn app_path(&self) -> Result<PathBuf> { todo!("headless: app_path") }
    fn path_for_auxiliary_executable(&self, name: &str) -> Result<PathBuf> { todo!("headless: path_for_auxiliary_executable") }
    fn set_cursor_style(&self, style: CursorStyle) {  }
    fn hide_cursor_until_mouse_moves(&self) {  }
    fn is_cursor_visible(&self) -> bool { todo!("headless: is_cursor_visible") }
    fn should_auto_hide_scrollbars(&self) -> bool { false }
    fn read_from_clipboard(&self) -> Option<ClipboardItem> { todo!("headless: read_from_clipboard") }
    fn write_to_clipboard(&self, item: ClipboardItem) {  }
    fn read_from_find_pasteboard(&self) -> Option<ClipboardItem> { todo!("headless: read_from_find_pasteboard") }
    fn write_to_find_pasteboard(&self, item: ClipboardItem) {  }
    fn write_credentials(&self, url: &str, username: &str, password: &[u8]) -> Task<Result<()>> { todo!("headless: write_credentials") }
    fn read_credentials(&self, url: &str) -> Task<Result<Option<(String, Vec<u8>)>>> { todo!("headless: read_credentials") }
    fn delete_credentials(&self, url: &str) -> Task<Result<()>> { todo!("headless: delete_credentials") }
    fn keyboard_layout(&self) -> Box<dyn PlatformKeyboardLayout> { todo!("headless: keyboard_layout") }
    fn keyboard_mapper(&self) -> Rc<dyn PlatformKeyboardMapper> { todo!("headless: keyboard_mapper") }
    fn on_keyboard_layout_change(&self, callback: Box<dyn FnMut()>) {  }
}
pub struct HeadlessTextSystem;
impl PlatformTextSystem for HeadlessTextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> { todo!("headless text system: add_fonts") }
    fn all_font_names(&self) -> Vec<String> { todo!("headless text system: all_font_names") }
    fn font_id(&self, descriptor: &Font) -> Result<FontId> { todo!("headless text system: font_id") }
    fn font_metrics(&self, font_id: FontId) -> FontMetrics { todo!("headless text system: font_metrics") }
    fn typographic_bounds(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Bounds<f32>> { todo!("headless text system: typographic_bounds") }
    fn advance(&self, font_id: FontId, glyph_id: GlyphId) -> Result<Size<f32>> { todo!("headless text system: advance") }
    fn glyph_for_char(&self, font_id: FontId, ch: char) -> Option<GlyphId> { todo!("headless text system: glyph_for_char") }
    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> { todo!("headless text system: glyph_raster_bounds") }
    fn rasterize_glyph( &self, params: &RenderGlyphParams, raster_bounds: Bounds<DevicePixels>, ) -> Result<(Size<DevicePixels>, Vec<u8>)> { todo!("headless text system: rasterize_glyph") }
    fn layout_line(&self, text: &str, font_size: Pixels, runs: &[FontRun]) -> LineLayout { todo!("headless text system: layout_line") }
    fn recommended_rendering_mode(&self, _font_id: FontId, _font_size: Pixels) -> TextRenderingMode { todo!("headless text system: recommended_rendering_mode") }
}
