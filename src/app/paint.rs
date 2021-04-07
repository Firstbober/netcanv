use std::collections::HashSet;
use std::{borrow::BorrowMut, collections::VecDeque, str::FromStr};

use std::path::PathBuf;
use std::time::{Duration, Instant};

use native_dialog::FileDialog;
use skulpin::skia_safe::paint as skpaint;
use skulpin::skia_safe::*;

use crate::net::{Message, Peer, Timer};
use crate::paint_canvas::*;
use crate::ui::*;
use crate::util::*;
use crate::viewport::Viewport;
use crate::{
    app::*,
    wallhackd::{self, WHDPaintFunctions},
};
use crate::{assets::*, ui};

use std::time::SystemTime;

extern crate image;
use image::{GenericImage, GenericImageView, Pixel, SubImage};

#[derive(PartialEq, Eq)]
enum PaintMode {
    None,
    Paint,
    Erase,
    WHDCustomImage,
}

type Log = Vec<(String, Instant)>;

pub enum WHDCIDrawingDirection {
    ToLeft,
    ToRight,
}

pub struct WHDState {
    custom_image_path: String,
    drawing_direction: WHDCIDrawingDirection,
    printed_room_id: bool,

    previous_chunk_data_timestamp: Option<SystemTime>,
}

pub struct State {
    assets: Assets,

    ui: Ui,
    paint_canvas: PaintCanvas<'static>,
    peer: Peer,
    update_timer: Timer,

    paint_mode: PaintMode,
    paint_color: Color4f,
    brush_size_slider: Slider,
    stroke_buffer: Vec<StrokePoint>,

    server_side_chunks: HashSet<(i32, i32)>,
    requested_chunks: HashSet<(i32, i32)>,
    downloaded_chunks: HashSet<(i32, i32)>,
    needed_chunks: HashSet<(i32, i32)>,
    deferred_message_queue: VecDeque<Message>,

    save_to_file: Option<PathBuf>,

    error: Option<String>,
    log: Log,

    panning: bool,
    viewport: Viewport,

    whd: WHDState,
}

const COLOR_PALETTE: &'static [u32] = &[
    0x100820ff, 0xff003eff, 0xff7b00ff, 0xffff00ff, 0x2dd70eff, 0x03cbfbff, 0x0868ebff, 0xa315d7ff, 0xffffffff,
];

macro_rules! log {
    ($log:expr, $($arg:tt)*) => {
        {
            $log.push((format!($($arg)*), Instant::now()));
            println!("[netcanv] {}", format!($($arg)*));
        }
    };
}

macro_rules! ok_or_log {
    ($log:expr, $exp:expr) => {
        match $exp {
            Ok(x) => x,
            Err(e) => log!($log, "{}", e),
        }
    };
}

impl wallhackd::WHDPaintFunctions for State {
    fn whd_process_canvas_start(&mut self, _canvas: &mut Canvas, _input: &Input) {
        if self.assets.whd_commandline.headless_client {
            let sc = self.assets.whd_commandline.save_canvas.clone();

            if sc.is_some() && self.whd.previous_chunk_data_timestamp.is_some() {
                match self.whd.previous_chunk_data_timestamp.unwrap().elapsed() {
                    Ok(time) => {
                        if time.as_secs() > 120 {
                            self.save_to_file = Some(PathBuf::from(sc.unwrap()));
                        }
                    }
                    Err(_err) => std::process::exit(1),
                }
            }
        }
    }

    fn whd_process_canvas_end(&mut self, _canvas: &mut Canvas, _input: &Input) {}

    fn whd_process_canvas_custom_image(&mut self, input: &Input) {
        log!(self.log, "[WallhackD] [Custom Image] Started!");

        // get image from file

        let mut trollage = image::open(self.whd.custom_image_path.as_str()).unwrap();
        let dm = trollage.dimensions();

        // calculate parts

        let width_parts = if dm.0 % 256 != 0 { (dm.0 / 256) + 1 } else { dm.0 / 256 };

        let height_parts = if dm.1 % 256 != 0 { (dm.1 / 256) + 1 } else { dm.1 / 256 };

        log!(
            self.log,
            "[WallhackD] [Custom Image] {} parts will be needed",
            width_parts * height_parts
        );

        // get offset for chunks

        let x_off = ((input.mouse_position().x + self.viewport.pan().x) / 256.0) as i32;
        let y_off = ((input.mouse_position().y + self.viewport.pan().y) / 256.0) as i32;

        log!(
            self.log,
            "[WallhackD] [Custom Image] Starting on chunks {}, {}",
            x_off,
            y_off
        );

        // process everything

        let mut image_to_insert: image::RgbaImage = Default::default();

        for x in 0..width_parts {
            for y in 0..height_parts {
                if y == height_parts - 1 && x == width_parts - 1 {
                    let part = SubImage::new(trollage.borrow_mut(), x * 256, y * 256, dm.0 - x * 256, dm.1 - y * 256);
                    image_to_insert = image::ImageBuffer::new(256, 256);
                    image_to_insert.copy_from(&part.to_image(), 0, 0).unwrap();
                } else if y == height_parts - 1 {
                    let part = SubImage::new(trollage.borrow_mut(), x * 256, y * 256, 256, dm.1 - y * 256);
                    image_to_insert = image::ImageBuffer::new(256, 256);
                    image_to_insert.copy_from(&part.to_image(), 0, 0).unwrap();
                } else if x == width_parts - 1 {
                    let part = SubImage::new(trollage.borrow_mut(), x * 256, y * 256, dm.0 - x * 256, 256);
                    image_to_insert = image::ImageBuffer::new(256, 256);
                    image_to_insert.copy_from(&part.to_image(), 0, 0).unwrap();
                } else {
                    let part = SubImage::new(trollage.borrow_mut(), x * 256, y * 256, 256, 256);
                    image_to_insert = part.to_image();
                }

                // change to bgra

                for px in image_to_insert.pixels_mut() {
                    let bgra = px.to_bgra();
                    let channels = px.channels_mut();

                    channels[0] = bgra[0];
                    channels[1] = bgra[1];
                    channels[2] = bgra[2];
                    channels[3] = bgra[3];
                }

                let pos = match self.whd.drawing_direction {
                    WHDCIDrawingDirection::ToLeft => {
                        ((x as i32 + x_off as i32) - width_parts as i32, y_off as i32 + y as i32)
                    }
                    WHDCIDrawingDirection::ToRight => (x as i32 + x_off as i32, y_off as i32 + y as i32),
                };

                self.paint_canvas.ensure_chunk_exists(pos);
                let chk = self.paint_canvas.chunks.get_mut(&pos).unwrap();
                let mut chunk_image = chk.as_image_buffer_mut();

                let sb = image_to_insert.view(0, 0, 256, 256);
                chunk_image.copy_from(&sb, 0, 0).unwrap();

                for addr in self.peer.mates() {
                    self.peer
                        .send_canvas_data(*addr.0, pos, chk.png_data().unwrap().to_vec())
                        .unwrap();
                }
            }
        }

        log!(self.log, "[WallhackD] [Custom Image] Completed!");
    }

    fn whd_process_overlay(&mut self, canvas: &mut Canvas, input: &Input) {
        self.ui
            .push_group((self.ui.width(), self.ui.height()), Layout::VerticalRev);
        self.ui.pad((32.0, 32.0));

        self.whd_overlay_window_begin(
            canvas,
            input,
            (300.0, 200.0),
            0.0,
            "Teleport to chunk",
            wallhackd::OverlayWindowPos::Middle,
        );
        self.whd_overlay_window_end();

        self.ui.pop_group();
    }

    fn whd_overlay_window_begin(
        &mut self,
        canvas: &mut Canvas,
        input: &Input,
        size: (f32, f32),
        margin: f32,
        title: &str,
        pos: wallhackd::OverlayWindowPos,
    ) {
        let g_height = size.1 + 32.0;

        let mut tg_height = g_height;
        let mut tg_layout = Layout::HorizontalRev;

        match pos {
            wallhackd::OverlayWindowPos::Top => {
                tg_height = self.ui.remaining_height();
                self.ui.pad((self.ui.remaining_width() - size.0, 0.0));
            }
            wallhackd::OverlayWindowPos::TopLeft => {
                tg_height = self.ui.remaining_height();
                tg_layout = Layout::Horizontal
            }
            wallhackd::OverlayWindowPos::TopRight => {
                tg_height = self.ui.remaining_height();
            }

            wallhackd::OverlayWindowPos::Middle => {
                tg_height = (self.ui.height() / 2.0) + (g_height / 2.0);
                self.ui.pad((self.ui.remaining_width() - size.0, 0.0));
            }
            wallhackd::OverlayWindowPos::MiddleLeft => {
                tg_height = (self.ui.height() / 2.0) + (g_height / 2.0);
                tg_layout = Layout::Horizontal
            }
            wallhackd::OverlayWindowPos::MiddleRight => {
                tg_height = (self.ui.height() / 2.0) + (g_height / 2.0);
            }

            wallhackd::OverlayWindowPos::Bottom => {
                self.ui.pad((self.ui.remaining_width() - size.0, 0.0));
            }
            wallhackd::OverlayWindowPos::BottomLeft => tg_layout = Layout::Horizontal,
            wallhackd::OverlayWindowPos::BottomRight => {}
        }

        self.ui.push_group((self.ui.width(), tg_height), tg_layout);

        self.ui.pad((margin, 0.0));

        self.ui.push_group((size.0, g_height), Layout::Vertical);
        self.ui.fill(canvas, Color::BLACK.with_a(200));

        self.ui.push_group((size.0, 32.0), Layout::HorizontalRev);
        self.ui.fill(canvas, Color::BLACK);

        self.ui
            .text(canvas, title, self.assets.colors.text, (AlignH::Center, AlignV::Middle));
        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.close,
            "Close".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {}

        self.ui.pop_group();
        self.ui.pop_group();

        self.ui.push_group(size, Layout::Vertical);
    }

    fn whd_overlay_window_end(&mut self) {
        self.ui.pop_group();
        self.ui.pop_group();
    }

    fn whd_bar_end_buttons(&mut self, canvas: &mut Canvas, input: &Input) {
        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.draw_it_again,
            "Draw again".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.paint_mode = PaintMode::WHDCustomImage;
        }

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.load_image,
            "Draw image".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            let path = FileDialog::new()
                .set_location(std::env::current_dir().unwrap().as_path())
                .add_filter("Image", &["png", "jpg", "jpeg", "webp"])
                .show_open_single_file()
                .unwrap();

            match path {
                Some(path) => {
                    log!(self.log, "[WallhackD] [Custom Image] Got image path");

                    self.paint_mode = PaintMode::WHDCustomImage;
                    self.whd.custom_image_path = String::from_str(path.to_str().unwrap()).unwrap();
                }
                None => log!(self.log, "[WallhackD] U selected nothing"),
            };
        }

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            match self.whd.drawing_direction {
                WHDCIDrawingDirection::ToLeft => &self.assets.icons.whd.backwards,
                WHDCIDrawingDirection::ToRight => &self.assets.icons.whd.forward,
            },
            format!(
                "Drawing direction ({})",
                match self.whd.drawing_direction {
                    WHDCIDrawingDirection::ToLeft => "To left",
                    WHDCIDrawingDirection::ToRight => "To right",
                }
            ),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.drawing_direction = match self.whd.drawing_direction {
                WHDCIDrawingDirection::ToLeft => WHDCIDrawingDirection::ToRight,
                WHDCIDrawingDirection::ToRight => WHDCIDrawingDirection::ToLeft,
            }
        }

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.pin_drop,
            "(WIP) Teleport to chunk".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {}
    }
}

impl State {
    const BAR_SIZE: f32 = 32.0;
    const TIME_PER_UPDATE: Duration = Duration::from_millis(50);

    pub fn new(assets: Assets, peer: Peer, image_path: Option<PathBuf>) -> Self {
        let mut this = Self {
            assets,

            ui: Ui::new(),
            paint_canvas: PaintCanvas::new(),
            peer,
            update_timer: Timer::new(Self::TIME_PER_UPDATE),

            paint_mode: PaintMode::None,
            paint_color: hex_color4f(COLOR_PALETTE[0]),
            brush_size_slider: Slider::new(4.0, 1.0, 64.0, SliderStep::Discrete(1.0)),
            stroke_buffer: Vec::new(),

            server_side_chunks: HashSet::new(),
            requested_chunks: HashSet::new(),
            downloaded_chunks: HashSet::new(),
            needed_chunks: HashSet::new(),
            deferred_message_queue: VecDeque::new(),

            save_to_file: None,

            error: None,
            log: Log::new(),

            panning: false,
            viewport: Viewport::new(),

            whd: WHDState {
                drawing_direction: WHDCIDrawingDirection::ToRight,
                custom_image_path: "".to_owned(),
                printed_room_id: false,
                previous_chunk_data_timestamp: None,
            },
        };
        if this.peer.is_host() {
            log!(this.log, "Welcome to your room!");
            log!(
                this.log,
                "To invite friends, send them the room ID shown in the bottom right corner of your screen."
            );
        }
        if let Some(image_path) = image_path {
            ok_or_log!(this.log, this.paint_canvas.load_from_image_file(&image_path));
        }
        this
    }

    fn fellow_stroke(canvas: &mut PaintCanvas, points: &[StrokePoint]) {
        if points.is_empty() {
            return;
        } // failsafe

        let mut from = points[0].point;
        let first_index = if points.len() > 1 { 1 } else { 0 };
        for point in &points[first_index..] {
            canvas.stroke(from, point.point, &point.brush);
            from = point.point;
        }
    }

    fn canvas_data(log: &mut Log, canvas: &mut PaintCanvas, chunk_position: (i32, i32), png_image: &[u8]) {
        ok_or_log!(log, canvas.decode_png_data(chunk_position, png_image));
    }

    fn process_log(&mut self, canvas: &mut Canvas) {
        self.log
            .retain(|(_, time_created)| time_created.elapsed() < Duration::from_secs(5));
        self.ui.draw_on_canvas(canvas, |canvas| {
            let mut paint = Paint::new(Color4f::from(Color::WHITE.with_a(192)), None);
            paint.set_blend_mode(BlendMode::Difference);
            let mut y = self.ui.height() - (self.log.len() as f32 - 1.0) * 16.0 - 8.0;
            for (entry, _) in &self.log {
                canvas.draw_str(&entry, (8.0, y), &self.assets.sans.borrow(), &paint);
                y += 16.0;
            }
        });
    }

    fn process_canvas(&mut self, canvas: &mut Canvas, input: &Input) {
        self.whd_process_canvas_start(canvas, input);

        self.ui
            .push_group((self.ui.width(), self.ui.height() - Self::BAR_SIZE), Layout::Freeform);
        let canvas_size = self.ui.size();

        //
        // input
        //

        // drawing

        if self.ui.has_mouse(input) {
            if input.mouse_button_just_pressed(MouseButton::Left) {
                if self.paint_mode != PaintMode::WHDCustomImage {
                    self.paint_mode = PaintMode::Paint;
                } else {
                    self.whd_process_canvas_custom_image(input);
                }
            } else if input.mouse_button_just_pressed(MouseButton::Right) {
                self.paint_mode = PaintMode::Erase;
            }
        }
        if input.mouse_button_just_released(MouseButton::Left) || input.mouse_button_just_released(MouseButton::Right) {
            self.paint_mode = PaintMode::None;
        }

        let brush_size = self.brush_size_slider.value();
        let from = input.previous_mouse_position() + self.viewport.pan();
        let to = input.mouse_position() + self.viewport.pan();
        loop {
            // give me back my labelled blocks
            let brush = match self.paint_mode {
                PaintMode::None => break,
                PaintMode::WHDCustomImage => break,
                PaintMode::Paint => Brush::Draw {
                    color: self.paint_color.clone(),
                    stroke_width: brush_size,
                },
                PaintMode::Erase => Brush::Erase {
                    stroke_width: brush_size,
                },
            };
            self.paint_canvas.stroke(from, to, &brush);
            if self.stroke_buffer.is_empty() {
                self.stroke_buffer.push(StrokePoint {
                    point: from,
                    brush: brush.clone(),
                });
            } else if to != self.stroke_buffer.last().unwrap().point {
                self.stroke_buffer.push(StrokePoint { point: to, brush });
            }
            break;
        }

        // panning

        if self.ui.has_mouse(input) && input.mouse_button_just_pressed(MouseButton::Middle) {
            self.panning = true;
        }
        if input.mouse_button_just_released(MouseButton::Middle) {
            self.panning = false;
        }

        if self.panning {
            let delta_pan = input.previous_mouse_position() - input.mouse_position();
            self.viewport.pan_around(delta_pan);
        }

        //
        // rendering
        //

        let paint_canvas = &self.paint_canvas;
        self.ui.draw_on_canvas(canvas, |canvas| {
            canvas.save();
            canvas.translate(-self.viewport.pan());

            let mut paint = Paint::new(Color4f::from(Color::WHITE.with_a(192)), None);
            paint.set_anti_alias(true);
            paint.set_blend_mode(BlendMode::Difference);

            paint_canvas.draw_to(canvas, &self.viewport, canvas_size);
            for (_, mate) in self.peer.mates() {
                let text_position =
                    mate.cursor + Point::new(mate.brush_size, mate.brush_size) * 0.5 + Point::new(0.0, 14.0);
                paint.set_style(skpaint::Style::Fill);
                canvas.draw_str(&mate.nickname, text_position, &self.assets.sans.borrow(), &paint);
                paint.set_style(skpaint::Style::Stroke);
                canvas.draw_circle(mate.cursor, mate.brush_size * 0.5, &paint);
            }

            canvas.restore();

            let mouse = self.ui.mouse_position(&input);
            paint.set_style(skpaint::Style::Stroke);
            canvas.draw_circle(mouse, self.brush_size_slider.value() * 0.5, &paint);
        });
        if self.panning {
            let pan = self.viewport.pan();
            let position = format!("{}, {}", (pan.x / 256.0).floor(), (pan.y / 256.0).floor());
            self.ui.push_group(self.ui.size(), Layout::Freeform);
            self.ui.pad((32.0, 32.0));
            self.ui.push_group((72.0, 62.0), Layout::Vertical);
            self.ui.fill(canvas, Color::BLACK.with_a(128));
            self.ui.pad((0.0, 8.0));

            self.ui.push_group((self.ui.width(), 20.0), Layout::Vertical);
            self.ui
                .text(canvas, &position, Color::WHITE, (AlignH::Center, AlignV::Middle));
            self.ui.pop_group();

            self.ui.space(2.0);

            let last_fs = self.ui.font_size();
            self.ui.set_font_size(12.0);

            self.ui.push_group((self.ui.width(), 16.0), Layout::Vertical);
            self.ui.text(
                canvas,
                &format!("L: {}", self.paint_canvas.chunks.len()),
                Color::WHITE.with_a(128),
                (AlignH::Center, AlignV::Middle),
            );

            self.ui.pop_group();

            self.ui.push_group((self.ui.width(), 16.0), Layout::Vertical);
            self.ui.text(
                canvas,
                &format!("T: {}", self.server_side_chunks.len()),
                Color::WHITE.with_a(128),
                (AlignH::Center, AlignV::Middle),
            );

            self.ui.pop_group();

            self.ui.set_font_size(last_fs);
            self.ui.pop_group();
            self.ui.pop_group();
        }

        self.whd_process_overlay(canvas, input);

        self.process_log(canvas);

        self.ui.pop_group();

        //
        // networking
        //

        for _ in self.update_timer.tick() {
            // mouse / drawing
            if input.previous_mouse_position() != input.mouse_position() {
                ok_or_log!(self.log, self.peer.send_cursor(to, brush_size));
            }
            if !self.stroke_buffer.is_empty() {
                ok_or_log!(self.log, self.peer.send_stroke(self.stroke_buffer.drain(..)));
            }
            // chunk downloading
            if self.save_to_file.is_some() {
                if self.requested_chunks.len() < self.server_side_chunks.len() {
                    self.needed_chunks
                        .extend(self.server_side_chunks.difference(&self.requested_chunks));
                } else if self.downloaded_chunks.len() == self.server_side_chunks.len() {
                    ok_or_log!(self.log, self.paint_canvas.save(&self.save_to_file.as_ref().unwrap()));
                    self.save_to_file = None;

                    if self.assets.whd_commandline.headless_client {
                        log!(self.log, "Saved canvas to file!");
                        std::process::exit(0);
                    }
                }
            } else {
                for chunk_position in self.viewport.visible_tiles(Chunk::SIZE, canvas_size) {
                    if self.server_side_chunks.contains(&chunk_position)
                        && !self.requested_chunks.contains(&chunk_position)
                    {
                        self.needed_chunks.insert(chunk_position);
                    }
                }
            }
        }

        self.whd_process_canvas_end(canvas, input);
    }

    fn process_bar(&mut self, canvas: &mut Canvas, input: &mut Input) {
        if self.paint_mode != PaintMode::None {
            input.lock_mouse_buttons();
        }

        self.ui
            .push_group((self.ui.width(), self.ui.remaining_height()), Layout::Horizontal);
        self.ui.fill(canvas, self.assets.colors.panel);
        self.ui.pad((16.0, 0.0));

        // palette

        for hex_color in COLOR_PALETTE {
            let color = hex_color4f(*hex_color);
            self.ui.push_group((16.0, self.ui.height()), Layout::Freeform);
            let y_offset = self.ui.height()
                * if self.paint_color == color {
                    0.5
                } else if self.ui.has_mouse(&input) {
                    0.7
                } else {
                    0.8
                };
            if self.ui.has_mouse(&input) && input.mouse_button_just_pressed(MouseButton::Left) {
                self.paint_color = color.clone();
            }
            self.ui.draw_on_canvas(canvas, |canvas| {
                let paint = Paint::new(color, None);
                let rect = Rect::from_point_and_size((0.0, y_offset), self.ui.size());
                canvas.draw_rect(rect, &paint);
            });
            self.ui.pop_group();
        }
        self.ui.space(16.0);

        // brush size

        self.ui.push_group((80.0, self.ui.height()), Layout::Freeform);
        self.ui.text(
            canvas,
            "Brush size",
            self.assets.colors.text,
            (AlignH::Center, AlignV::Middle),
        );
        self.ui.pop_group();

        self.ui.space(8.0);
        self.brush_size_slider.process(
            &mut self.ui,
            canvas,
            input,
            SliderArgs {
                width: 192.0,
                color: self.assets.colors.slider,
            },
        );
        self.ui.space(8.0);

        let brush_size_string = self.brush_size_slider.value().to_string();
        self.ui
            .push_group((self.ui.height(), self.ui.height()), Layout::Freeform);
        self.ui.set_font(self.assets.sans_bold.clone());
        self.ui.text(
            canvas,
            &brush_size_string,
            self.assets.colors.text,
            (AlignH::Center, AlignV::Middle),
        );
        self.ui.pop_group();

        //
        // right side
        //

        // room ID

        self.ui
            .push_group((self.ui.remaining_width(), self.ui.height()), Layout::HorizontalRev);
        // note that the elements go from right to left
        // the save button
        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.file.save,
            "Save canvas".to_owned(),
            WHDTooltipPos::TopLeft,
        )
        .clicked()
        {
            match FileDialog::new()
                .set_filename("canvas.png")
                .add_filter("PNG image", &["png"])
                .show_save_single_file()
            {
                Ok(Some(path)) => {
                    self.paint_canvas.cleanup_empty_chunks();
                    self.save_to_file = Some(path);
                }
                Err(error) => log!(self.log, "Error while selecting file: {}", error),
                _ => (),
            }
        }

        // [WHD] Inject buttons
        self.whd_bar_end_buttons(canvas, input);

        if self.peer.is_host() {
            if !self.whd.printed_room_id {
                println!("Created room with id {:04}!", self.peer.room_id().unwrap());
                self.whd.printed_room_id = true;
            }

            // the room ID itself
            let id_text = format!("{:04}", self.peer.room_id().unwrap());
            self.ui.push_group((64.0, self.ui.height()), Layout::Freeform);
            self.ui.set_font(self.assets.sans_bold.clone());
            self.ui.text(
                canvas,
                &id_text,
                self.assets.colors.text,
                (AlignH::Center, AlignV::Middle),
            );
            self.ui.pop_group();

            // "Room ID" text
            self.ui.push_group((64.0, self.ui.height()), Layout::Freeform);
            self.ui.text(
                canvas,
                "Room ID",
                self.assets.colors.text,
                (AlignH::Center, AlignV::Middle),
            );
            self.ui.pop_group();
        }
        self.ui.pop_group();

        self.ui.pop_group();

        input.unlock_mouse_buttons();
    }
}

impl AppState for State {
    fn process(
        &mut self,
        StateArgs {
            canvas,
            coordinate_system_helper,
            input,
        }: StateArgs,
    ) {
        canvas.clear(Color::WHITE);

        // network

        match self.peer.tick() {
            Ok(messages) => {
                for message in messages {
                    match message {
                        Message::Error(error) => self.error = Some(error),
                        Message::Connected => unimplemented!(
                            "Message::Connected shouldn't be generated after connecting to the matchmaker"
                        ),
                        Message::Left(nickname) => log!(self.log, "{} left the room", nickname),
                        Message::Stroke(points) => Self::fellow_stroke(&mut self.paint_canvas, &points),
                        Message::ChunkPositions(mut positions) => {
                            eprintln!("received {} chunk positions", positions.len());
                            self.server_side_chunks = positions.drain(..).collect();
                        }
                        Message::Chunks(chunks) => {
                            eprintln!("received {} chunks", chunks.len());
                            for (chunk_position, png_data) in chunks {
                                self.whd.previous_chunk_data_timestamp = Some(SystemTime::now());
                                Self::canvas_data(&mut self.log, &mut self.paint_canvas, chunk_position, &png_data);
                                self.downloaded_chunks.insert(chunk_position);
                            }
                        }
                        message => self.deferred_message_queue.push_back(message),
                    }
                }
            }
            Err(error) => {
                eprintln!("{}", error);
            }
        }

        for message in self.deferred_message_queue.drain(..) {
            match message {
                Message::Joined(nickname, addr) => {
                    log!(self.log, "{} joined the room", nickname);
                    if let Some(addr) = addr {
                        let positions = self.paint_canvas.chunk_positions();
                        ok_or_log!(self.log, self.peer.send_chunk_positions(addr, positions));
                    }
                }
                Message::GetChunks(addr, positions) => {
                    eprintln!("got request from {} for {} chunks", addr, positions.len());
                    let paint_canvas = &mut self.paint_canvas;
                    for (i, chunks) in positions.chunks(32).enumerate() {
                        eprintln!("  sending packet #{} containing {} chunks", i, chunks.len());
                        let packet: Vec<((i32, i32), Vec<u8>)> = chunks
                            .iter()
                            .filter_map(|position| {
                                paint_canvas
                                    .png_data(*position)
                                    .map(|slice| (*position, Vec::from(slice)))
                            })
                            .collect();
                        ok_or_log!(self.log, self.peer.send_chunks(addr, packet));
                    }
                    eprintln!("  all packets sent");
                }
                _ => unreachable!("unhandled peer message type"),
            }
        }

        if self.needed_chunks.len() > 0 {
            for chunk in &self.needed_chunks {
                self.requested_chunks.insert(*chunk);
            }
            ok_or_log!(
                self.log,
                self.peer.download_chunks(self.needed_chunks.drain().collect())
            );
        }

        // UI setup
        self.ui
            .begin(get_window_size(&coordinate_system_helper), Layout::Vertical);
        self.ui.set_font(self.assets.sans.clone());
        self.ui.set_font_size(14.0);

        // canvas
        self.process_canvas(canvas, input);

        // bar
        self.process_bar(canvas, input);
    }

    fn next_state(self: Box<Self>) -> Box<dyn AppState> {
        if let Some(error) = self.error {
            Box::new(lobby::State::new(self.assets, Some(&error)))
        } else {
            self
        }
    }
}
