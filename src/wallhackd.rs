use crate::ui::{self, ButtonProcessResult};

use skulpin::skia_safe as skia;

pub const WALLHACKD_VERSION: &str = "1.1";

pub struct WHDCommandLine {
    pub headless_client: bool,
    pub headless_host: bool,

    pub username: Option<String>,
    pub matchmaker_addr: Option<String>,
    pub roomid: Option<String>,

    pub save_canvas: Option<String>,
    pub load_canvas: Option<String>,
}

pub enum OverlayWindowPos {
    TopLeft,
    Top,
    TopRight,

    MiddleLeft,
    Middle,
    MiddleRight,

    BottomLeft,
    Bottom,
    BottomRight,
}

pub trait WHDPaintFunctions {
    fn whd_process_canvas_start(&mut self, canvas: &mut skia::Canvas, input: &ui::Input);
    fn whd_process_canvas_end(&mut self, canvas: &mut skia::Canvas, input: &ui::Input);
    fn whd_process_canvas_custom_image(&mut self, input: &ui::Input);

    fn whd_process_overlay(&mut self, canvas: &mut skia::Canvas, input: &mut ui::Input);
    fn whd_overlay_window_begin(
        &mut self,
        canvas: &mut skia::Canvas,
        input: &mut ui::Input,
        size: (f32, f32),
        margin: f32,
        title: &str,
        pos: OverlayWindowPos,
    ) -> bool;
    fn whd_overlay_window_end(&mut self, input: &mut ui::Input);

    fn whd_bar_end_buttons(&mut self, canvas: &mut skia::Canvas, input: &ui::Input);
}

pub trait WHDLobbyFunctions {
    fn whd_process_menu_start(&mut self, canvas: &mut skia::Canvas, input: &ui::Input);
    fn whd_process_menu_expands(&mut self, canvas: &mut skia::Canvas, input: &ui::Input);

    fn whd_process_right_bar(&mut self, canvas: &mut skia::Canvas, input: &ui::Input);
}
