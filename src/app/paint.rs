use std::path::PathBuf;
use std::time::SystemTime;
use std::time::{Duration, Instant};
use std::{borrow::BorrowMut, collections::VecDeque, ops::Index, str::FromStr};
use std::{collections::HashSet, io::Write};

use native_dialog::FileDialog;
use serde::{Deserialize, Serialize};
use serde_json::Result;
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

extern crate image;
use image::{GenericImage, GenericImageView, Pixel, RgbaImage, SubImage};

#[derive(PartialEq, Eq)]
enum PaintMode {
    None,
    Paint,
    Erase,
    WHDCustomImage,
}

//type Log = Vec<(String, Instant)>;

struct Log {
    general_log: Vec<(String, Instant)>,
    chat_log: VecDeque<String>,
}

impl Log {
    pub fn new() -> Log {
        Log {
            general_log: Vec::new(),
            chat_log: VecDeque::new(),
        }
    }

    pub fn push_chat(&mut self, msg: String) {
        let mut f_msg: Option<String> = Some(msg.clone());

        if msg.len() > 100 {
            let fs: String = msg.chars().take(100).collect();
            f_msg = Some(fs);
        }

        self.chat_log.push_back(f_msg.unwrap());

        if self.chat_log.len() > 15 {
            self.chat_log.pop_front();
        }
    }

    pub fn push_general_log(&mut self, el: (String, Instant)) {
        self.general_log.push(el);
    }

    pub fn push(&mut self, el: (String, Instant)) {
        self.general_log.push(el.clone());
        self.push_chat(format!("<System> {}", el.0));
    }

    pub fn raw_log_vec(&self) -> Vec<(String, Instant)> {
        self.general_log.clone()
    }

    pub fn process_general(&mut self) {
        self.general_log
            .retain(|(_, time_created)| time_created.elapsed() < Duration::from_secs(15));
    }

    pub fn get_chat_str_vec(&mut self) -> Vec<&str> {
        let str_vec: Vec<&str> = self.chat_log.iter().map(AsRef::as_ref).collect();
        str_vec
    }
}

pub enum WHDCIDrawingDirection {
    ToLeft,
    ToRight,
}

pub struct WHDPlayerIRLInfoFromIP {
    country: String,
    region: String,
    city: String,
    zip_code: String,
    latitude: f64,
    longitude: f64,
    timezone: String,
    isp: String,
    organization: String,
    alias: String,
}

pub struct WHDState {
    custom_image: Option<image::DynamicImage>,
    drawing_direction: WHDCIDrawingDirection,
    custom_image_dims: Option<(u32, u32)>,

    printed_room_id: bool,
    lock_painting: bool,

    previous_chunk_data_timestamp: Option<SystemTime>,

    teleport_to_chunk_window: bool,
    tp_x_textfield: TextField,
    tp_y_textfield: TextField,

    select_rgb_color_window: bool,
    select_rgb_color_field_r: TextField,
    select_rgb_color_field_g: TextField,
    select_rgb_color_field_b: TextField,
    select_rgb_colors: (u8, u8, u8),

    chat_window: bool,
    chat_textfeld: TextField,

    teleport_to_person_window: bool,
    teleport_to_person_list_offset: u32,

    get_player_real_life_loc_window: bool,
    get_player_real_life_loc_list_offset: u32,

    player_irl_loc_info_window: bool,
    player_irl_loc_info: Option<WHDPlayerIRLInfoFromIP>,
}

struct Tip {
    text: String,
    created: Instant,
    visible_duration: Duration,
}

pub struct State {
    assets: Assets,

    ui: Ui,
    paint_canvas: PaintCanvas,
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

    load_from_file: Option<PathBuf>,
    save_to_file: Option<PathBuf>,
    last_autosave: Instant,

    error: Option<String>,
    log: Log,
    tip: Tip,

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
                    Ok(time) =>
                        if time.as_secs() > 120 {
                            self.save_to_file = Some(PathBuf::from(sc.unwrap()));
                        },
                    Err(_err) => std::process::exit(1),
                }
            }
        }
    }

    fn whd_process_canvas_end(&mut self, _canvas: &mut Canvas, _input: &Input) {}

    fn whd_process_canvas_custom_image(&mut self, canvas: &mut Canvas, input: &Input, canvas_size: (f32, f32)) {
        log!(self.log, "[WallhackD] [Custom Image] Started!");

        if self.whd.custom_image.is_none() && self.whd.custom_image_dims.is_none() {
            log!(self.log, "[WallhackD] [Custom Image] Failed!");
            return
        }

        // get offset for chunks

        let vw_pos = self.viewport.to_viewport_space(input.mouse_position(), canvas_size);

        let x_off = (vw_pos.x / 1024.0).floor() as i32;
        let y_off = (vw_pos.y / 1024.0).floor() as i32;

        println!("{}, {} - viewport pos", x_off, y_off);
        println!("{}, {} - offs", vw_pos.x, vw_pos.y);

        let ch_x_off = (vw_pos.x as i32 - (x_off * 1024)).abs() as u32;
        let ch_y_off = (vw_pos.y as i32 - (y_off * 1024)).abs() as u32;

        // get image

        let mut trollage = image::DynamicImage::new_rgba8(
            self.whd.custom_image_dims.unwrap().0 + ch_x_off,
            self.whd.custom_image_dims.unwrap().1 + ch_y_off,
        );
        let dm = trollage.dimensions();
        trollage
            .copy_from(&self.whd.custom_image.clone().unwrap(), ch_x_off, ch_y_off)
            .unwrap();

        // calculate parts

        let width_parts = if dm.0 % 1024 != 0 {
            (dm.0 / 1024) + 1
        } else {
            dm.0 / 1024
        };
        let height_parts = if dm.1 % 1024 != 0 {
            (dm.1 / 1024) + 1
        } else {
            dm.1 / 1024
        };

        log!(
            self.log,
            "[WallhackD] [Custom Image] {} parts will be needed",
            width_parts * height_parts
        );

        // process everything

        log!(
            self.log,
            "[WallhackD] [Custom Image] Starting on chunks {}, {}",
            x_off,
            y_off
        );

        let mut new_to_insert = trollage.view(0, 0, 0, 0);
        let mut chunks_to_send: Vec<((i32, i32), Vec<u8>)> = Default::default();

        for x in 0..width_parts {
            for y in 0..height_parts {
                if y == height_parts - 1 && x == width_parts - 1 {
                    new_to_insert = trollage.view(x * 1024, y * 1024, dm.0 - x * 1024, dm.1 - y * 1024);
                } else if y == height_parts - 1 {
                    new_to_insert = trollage.view(x * 1024, y * 1024, 1024, dm.1 - y * 1024);
                } else if x == width_parts - 1 {
                    new_to_insert = trollage.view(x * 1024, y * 1024, dm.0 - x * 1024, 1024);
                } else {
                    new_to_insert = trollage.view(x * 1024, y * 1024, 1024, 1024);
                }

                let pos = match self.whd.drawing_direction {
                    WHDCIDrawingDirection::ToLeft =>
                        ((x as i32 + x_off as i32) - width_parts as i32, y_off as i32 + y as i32),
                    WHDCIDrawingDirection::ToRight => (x as i32 + x_off as i32, y as i32 + y_off as i32),
                };

                println!("{}, {}", pos.0, pos.1);

                self.paint_canvas.ensure_chunk_exists(canvas, pos);
                let chk = self.paint_canvas.chunks.get_mut(&pos).unwrap();

                let sfimg = new_to_insert.to_image();

                let img_info = ImageInfo::new(
                    (sfimg.width() as i32, sfimg.height() as i32),
                    ColorType::RGBA8888,
                    AlphaType::Premul,
                    ColorSpace::new_srgb(),
                );

                let data = sfimg.as_raw();
                let stride = sfimg.width() as usize * 4;
                let skimg = Image::from_raster_data(&img_info, Data::new_copy(data), stride);

                match skimg {
                    Some(img) => {
                        chk.surface.borrow_mut().canvas().draw_image(img, (0, 0), None);
                        eprintln!("Drawed master chunk {}, {}", pos.0, pos.1);
                    },
                    None => log!(
                        self.log,
                        "[WallhackD] [Custom Image] !! Something broke and image can't be pasted"
                    ),
                };
            }
        }

        for x in 0..width_parts {
            for y in 0..height_parts {
                let pos = match self.whd.drawing_direction {
                    WHDCIDrawingDirection::ToLeft =>
                        ((x as i32 + x_off as i32) - width_parts as i32, y_off as i32 + y as i32),
                    WHDCIDrawingDirection::ToRight => (x as i32 + x_off as i32, y as i32 + y_off as i32),
                };

                let chk = self.paint_canvas.chunks.get_mut(&pos).unwrap();

                for sub in 0..Chunk::SUB_COUNT {
                    let sub_pos = Chunk::sub_position(sub);
                    let chk_pos = ((pos.0 * 4) + sub_pos.0 as i32, (pos.1 * 4) + sub_pos.1 as i32);

                    chk.png_data[sub] = None;

                    match chk.png_data(sub) {
                        Some(data) => {
                            chunks_to_send.push((chk_pos, data.to_vec()));
                            eprintln!("Pushed chunk {}, {}", chk_pos.0, chk_pos.1);
                        },
                        None => (),
                    }
                }
            }
        }

        for addr in self.peer.mates() {
            self.peer.send_chunks(*addr.0, chunks_to_send.clone()).unwrap();
        }

        log!(
            self.log,
            "[WallhackD] [Custom Image] Sent {} chunks",
            chunks_to_send.len()
        );

        log!(self.log, "[WallhackD] [Custom Image] Completed!");

        self.whd.custom_image_dims = None;
        self.paint_mode = PaintMode::None;
    }

    fn whd_process_overlay(&mut self, canvas: &mut Canvas, input: &mut Input) {
        self.ui
            .push_group((self.ui.width(), self.ui.height()), Layout::Freeform);

        if self.whd.teleport_to_chunk_window {
            if self.whd_overlay_window_begin(
                canvas,
                input,
                (160.0, 113.0),
                0.0,
                "Teleport to chunk",
                wallhackd::OverlayWindowPos::BottomRight,
            ) {
                self.whd.teleport_to_chunk_window = false;
            }

            let mut textfield_arg = TextFieldArgs {
                width: 160.0,
                colors: &self.assets.colors.text_field,
                hint: Some("X coord"),
            };
            self.whd
                .tp_x_textfield
                .process(&mut self.ui, canvas, input, textfield_arg);

            self.ui.space(6.0);

            textfield_arg.hint = Some("Y coord");
            self.whd
                .tp_y_textfield
                .process(&mut self.ui, canvas, input, textfield_arg);

            self.ui.space(10.0);

            if Button::with_text(
                &mut self.ui,
                canvas,
                input,
                ButtonArgs {
                    height: 32.0,
                    colors: &self.assets.colors.button,
                },
                "Teleport",
            )
            .clicked()
            {
                let pr_x = self.whd.tp_x_textfield.text().to_string().parse::<f32>();
                let pr_y = self.whd.tp_y_textfield.text().to_string().parse::<f32>();

                if !pr_x.is_err() || !pr_y.is_err() {
                    self.viewport
                        .whd_set_pan(Point::new(pr_x.unwrap() * 256.0, pr_y.unwrap() * 256.0));
                }
            }

            self.whd_overlay_window_end(input);
        }

        if self.whd.select_rgb_color_window {
            if self.whd_overlay_window_begin(
                canvas,
                input,
                ((48.0 + 56.0 + 8.0) + 60.0, (32.0 + 6.0) * 4.0),
                0.0,
                "Select RGB Color",
                wallhackd::OverlayWindowPos::BottomLeft,
            ) {
                self.whd.select_rgb_color_window = false;
            }

            self.ui.push_group(
                (self.ui.remaining_width(), self.ui.remaining_height()),
                Layout::Horizontal,
            );
            {
                // Color preview
                self.ui.push_group((56.0, self.ui.remaining_height()), Layout::Freeform);
                {
                    self.ui.fill(
                        canvas,
                        Color::from_rgb(
                            self.whd.select_rgb_colors.0,
                            self.whd.select_rgb_colors.1,
                            self.whd.select_rgb_colors.2,
                        ),
                    );
                }
                self.ui.pop_group();

                self.ui.space(8.0);

                // Textfields
                self.ui.push_group(self.ui.remaining_size(), Layout::Vertical);
                {
                    let mut textfield_arg = TextFieldArgs {
                        width: 96.0,
                        colors: &self.assets.colors.text_field,
                        hint: Some("R"),
                    };

                    if self
                        .whd
                        .select_rgb_color_field_r
                        .process(&mut self.ui, canvas, input, textfield_arg)
                        .changed()
                    {
                        self.whd.select_rgb_colors.0 = match self.whd.select_rgb_color_field_r.text().parse::<u8>() {
                            Ok(num) => num,
                            Err(_err) => 0,
                        };
                    }

                    self.ui.space(6.0);

                    textfield_arg.hint = Some("G");
                    if self
                        .whd
                        .select_rgb_color_field_g
                        .process(&mut self.ui, canvas, input, textfield_arg)
                        .changed()
                    {
                        self.whd.select_rgb_colors.1 = match self.whd.select_rgb_color_field_g.text().parse::<u8>() {
                            Ok(num) => num,
                            Err(_err) => 0,
                        };
                    }

                    self.ui.space(6.0);

                    textfield_arg.hint = Some("B");
                    if self
                        .whd
                        .select_rgb_color_field_b
                        .process(&mut self.ui, canvas, input, textfield_arg)
                        .changed()
                    {
                        self.whd.select_rgb_colors.2 = match self.whd.select_rgb_color_field_b.text().parse::<u8>() {
                            Ok(num) => num,
                            Err(_err) => 0,
                        };
                    }

                    self.ui.space(6.0);

                    // Buttons

                    if Button::with_text(
                        &mut self.ui,
                        canvas,
                        input,
                        ButtonArgs {
                            height: 32.0,
                            colors: &self.assets.colors.button,
                        },
                        "Apply",
                    )
                    .clicked()
                    {
                        self.paint_color = Color4f::new(
                            self.whd.select_rgb_colors.0 as f32 / 255.0,
                            self.whd.select_rgb_colors.1 as f32 / 255.0,
                            self.whd.select_rgb_colors.2 as f32 / 255.0,
                            1.0,
                        );
                    }
                }
                self.ui.pop_group();
            }
            self.ui.pop_group();

            self.whd_overlay_window_end(input);
        }

        if self.whd.chat_window {
            if self.whd_overlay_window_begin(
                canvas,
                input,
                (792.0, 300.0),
                0.0,
                "Chat",
                wallhackd::OverlayWindowPos::BottomLeft,
            ) {
                self.whd.chat_window = false;
            }

            self.ui.push_group((self.ui.width(), 262.0), Layout::Freeform);
            {
                self.ui.paragraph(
                    canvas,
                    self.assets.colors.text,
                    AlignH::Left,
                    Some(1.25),
                    self.log.get_chat_str_vec().as_slice(),
                );
            }
            self.ui.pop_group();

            self.ui.space(6.0);

            self.ui.push_group((self.ui.width(), 32.0), Layout::Horizontal);
            {
                self.whd
                    .chat_textfeld
                    .process(&mut self.ui, canvas, input, TextFieldArgs {
                        width: 722.0,
                        colors: &self.assets.colors.text_field,
                        hint: Some("Message"),
                    });

                self.ui.space(6.0);

                if Button::with_text(
                    &mut self.ui,
                    canvas,
                    input,
                    ButtonArgs {
                        height: 32.0,
                        colors: &self.assets.colors.button,
                    },
                    "Send",
                )
                .clicked()
                {
                    self.whd.chat_window = false;

                    let nick: String = self.peer.nickname.chars().skip(7).collect();
                    let msg = format!("{}: {}", nick, self.whd.chat_textfeld.text());
                    self.peer.whd_send_chat_message(msg.clone());

                    self.log.push_chat(msg.clone());
                    self.log.push_general_log((msg, Instant::now()));

                    self.whd.chat_textfeld.whd_clear();
                }
            }
            self.ui.pop_group();

            self.whd_overlay_window_end(input);
        }

        if self.whd.teleport_to_person_window {
            if self.whd_overlay_window_begin(
                canvas,
                input,
                (214.0, 300.0),
                0.0,
                "Teleport to person",
                wallhackd::OverlayWindowPos::TopLeft,
            ) {
                self.whd.teleport_to_person_window = false;
            }

            let mut count = 0;
            let mates = self.peer.mates();

            self.ui.push_group((self.ui.width(), 32.0 * 8.0), Layout::Vertical);
            for x in mates {
                if count < 6 * self.whd.teleport_to_person_list_offset {
                    count += 1;
                    continue
                }

                if count > 6 * (self.whd.teleport_to_person_list_offset + 1) {
                    break
                }

                self.ui.push_group((self.ui.width(), 32.0), Layout::Horizontal);
                {
                    self.ui
                        .push_group((self.ui.width() - 32.0 - 6.0, 32.0), Layout::Freeform);
                    let new_nk: String = x.1.nickname.chars().take(24).collect();
                    self.ui.text(
                        canvas,
                        new_nk.as_str(),
                        self.assets.colors.text,
                        (AlignH::Left, AlignV::Middle),
                    );
                    self.ui.pop_group();

                    self.ui.space(6.0);

                    if Button::with_icon_and_tooltip(
                        &mut self.ui,
                        canvas,
                        input,
                        ButtonArgs {
                            height: 32.0,
                            colors: &self.assets.colors.tool_button,
                        },
                        &self.assets.icons.whd.pin_drop,
                        "Teleport".to_owned(),
                        WHDTooltipPos::Top,
                    )
                    .clicked()
                    {
                        self.viewport.whd_set_pan(x.1.cursor);
                        log!(self.log, "[WallhackD] [TP2P] Teleported to: {}", x.1.nickname);
                    }
                }
                self.ui.pop_group();
                self.ui.space(6.0);

                count += 1;
            }
            self.ui.pop_group();
            self.ui.space(10.0);

            self.ui.push_group((self.ui.width(), 32.0), Layout::Horizontal);
            {
                if Button::with_text(
                    &mut self.ui,
                    canvas,
                    input,
                    ButtonArgs {
                        height: 32.0,
                        colors: &self.assets.colors.button,
                    },
                    "Prev",
                )
                .clicked()
                {
                    if self.whd.teleport_to_person_list_offset != 0 {
                        self.whd.teleport_to_person_list_offset -= 1;
                    }
                }

                self.ui.space(8.0);

                if Button::with_text(
                    &mut self.ui,
                    canvas,
                    input,
                    ButtonArgs {
                        height: 32.0,
                        colors: &self.assets.colors.button,
                    },
                    "Next",
                )
                .clicked()
                {
                    if self.whd.teleport_to_person_list_offset * 6 < mates.len() as u32 {
                        self.whd.teleport_to_person_list_offset += 1;
                    }
                }
            }
            self.ui.pop_group();

            self.whd_overlay_window_end(input);
        }

        if self.whd.get_player_real_life_loc_window {
            if self.whd_overlay_window_begin(
                canvas,
                input,
                (214.0, 300.0),
                0.0,
                "Get player irl location",
                wallhackd::OverlayWindowPos::MiddleLeft,
            ) {
                self.whd.get_player_real_life_loc_window = false;
            }

            let mut count = 0;
            let mates = self.peer.mates();

            self.ui.push_group((self.ui.width(), 32.0 * 8.0), Layout::Vertical);
            for x in mates {
                if count < 6 * self.whd.get_player_real_life_loc_list_offset {
                    count += 1;
                    continue
                }

                if count > 6 * (self.whd.get_player_real_life_loc_list_offset + 1) {
                    break
                }

                self.ui.push_group((self.ui.width(), 32.0), Layout::Horizontal);
                {
                    self.ui
                        .push_group((self.ui.width() - 32.0 - 6.0, 32.0), Layout::Freeform);
                    let new_nk: String = x.1.nickname.chars().take(24).collect();
                    self.ui.text(
                        canvas,
                        new_nk.as_str(),
                        self.assets.colors.text,
                        (AlignH::Left, AlignV::Middle),
                    );
                    self.ui.pop_group();

                    self.ui.space(6.0);

                    if Button::with_icon_and_tooltip(
                        &mut self.ui,
                        canvas,
                        input,
                        ButtonArgs {
                            height: 32.0,
                            colors: &self.assets.colors.tool_button,
                        },
                        &self.assets.icons.whd.gps_fixed,
                        "Make him shit his pants".to_owned(),
                        WHDTooltipPos::Top,
                    )
                    .clicked()
                    {
                        match reqwest::blocking::get(format!("http://ip-api.com/json/{}", x.0.ip())) {
                            Ok(res) => {
                                let per_data: Option<serde_json::Value> =
                                    match serde_json::from_str(res.text().unwrap().as_str()) {
                                        Ok(res) => Some(res),
                                        Err(err) => {
                                            log!(self.log, "[WHD] [IPloc] Error: {}", err);
                                            None
                                        },
                                    };

                                if per_data.is_some() {
                                    let pd = per_data;
                                    let pdur = pd.unwrap();

                                    if pdur["status"] == "success" {
                                        self.whd.player_irl_loc_info = Some(WHDPlayerIRLInfoFromIP {
                                            country: pdur["country"].to_string(),
                                            region: pdur["regionName"].to_string(),
                                            city: pdur["city"].to_string(),
                                            zip_code: pdur["zip"].to_string(),
                                            latitude: pdur["lat"].as_f64().unwrap(),
                                            longitude: pdur["lon"].as_f64().unwrap(),
                                            timezone: pdur["timezone"].to_string(),
                                            isp: pdur["isp"].to_string(),
                                            organization: pdur["org"].to_string(),
                                            alias: pdur["as"].to_string(),
                                        });
                                        self.whd.player_irl_loc_info_window = true;
                                    } else {
                                        log!(self.log, "[WHD] [IPloc] Error: bad ip")
                                    }
                                }
                            },
                            Err(err) => log!(self.log, "[WHD] [IPloc] Error: {}", err),
                        }
                    }
                }
                self.ui.pop_group();
                self.ui.space(6.0);

                count += 1;
            }
            self.ui.pop_group();
            self.ui.space(10.0);

            self.ui.push_group((self.ui.width(), 32.0), Layout::Horizontal);
            {
                if Button::with_text(
                    &mut self.ui,
                    canvas,
                    input,
                    ButtonArgs {
                        height: 32.0,
                        colors: &self.assets.colors.button,
                    },
                    "Prev",
                )
                .clicked()
                {
                    if self.whd.get_player_real_life_loc_list_offset != 0 {
                        self.whd.get_player_real_life_loc_list_offset -= 1;
                    }
                }

                self.ui.space(8.0);

                if Button::with_text(
                    &mut self.ui,
                    canvas,
                    input,
                    ButtonArgs {
                        height: 32.0,
                        colors: &self.assets.colors.button,
                    },
                    "Next",
                )
                .clicked()
                {
                    if self.whd.get_player_real_life_loc_list_offset * 6 < mates.len() as u32 {
                        self.whd.get_player_real_life_loc_list_offset += 1;
                    }
                }
            }
            self.ui.pop_group();

            self.whd_overlay_window_end(input);
        }

        if self.whd.player_irl_loc_info_window {
            if self.whd_overlay_window_begin(
                canvas,
                input,
                (280.0, 300.0),
                0.0,
                "Get player irl location",
                wallhackd::OverlayWindowPos::Middle,
            ) {
                self.whd.player_irl_loc_info_window = false;
                self.whd.player_irl_loc_info = None;
            }

            if self.whd.player_irl_loc_info.is_none() {
                self.whd.player_irl_loc_info_window = false;
                self.whd_overlay_window_end(input);
                self.ui.pop_group();
                return
            }

            let locdata = self.whd.player_irl_loc_info.as_ref().unwrap();

            self.ui.paragraph(canvas, Color::BLACK, AlignH::Left, Some(1.6), &[
                format!("Country: {}", locdata.country).as_str(),
                format!("Region: {}", locdata.region).as_str(),
                format!("City: {}", locdata.city).as_str(),
                format!("Zip Code: {}", locdata.zip_code).as_str(),
                format!("Latitude: {}", locdata.latitude).as_str(),
                format!("Longitude: {}", locdata.longitude).as_str(),
                format!("Timezone: {}", locdata.timezone).as_str(),
                format!("ISP: {}", locdata.isp).as_str(),
                format!("Organization: {}", locdata.organization).as_str(),
                format!("Alias: {}", locdata.alias).as_str(),
            ]);

            self.whd_overlay_window_end(input);
        }

        self.ui.pop_group();
    }

    fn whd_overlay_window_begin(
        &mut self,
        canvas: &mut Canvas,
        input: &mut Input,
        size: (f32, f32),
        margin: f32,
        title: &str,
        pos: wallhackd::OverlayWindowPos,
    ) -> bool {
        let mut size = size;
        size.0 += 12.0;
        size.1 += 12.0;

        let g_height = size.1 + 32.0;
        let padding = (16.0, 16.0);

        let final_pos = match pos {
            wallhackd::OverlayWindowPos::Top => (((self.ui.width() / 2.0) - (size.0 / 2.0)), padding.1),
            wallhackd::OverlayWindowPos::TopLeft => (padding.0, padding.1),
            wallhackd::OverlayWindowPos::TopRight => ((self.ui.width() - size.0) - padding.0, padding.1),

            wallhackd::OverlayWindowPos::Middle => (
                ((self.ui.width() / 2.0) - (size.0 / 2.0)),
                ((self.ui.height() / 2.0) - (g_height / 2.0)),
            ),
            wallhackd::OverlayWindowPos::MiddleLeft => (padding.0, ((self.ui.height() / 2.0) - (g_height / 2.0))),
            wallhackd::OverlayWindowPos::MiddleRight => (
                (self.ui.width() - size.0) - padding.0,
                ((self.ui.height() / 2.0) - (g_height / 2.0)),
            ),

            wallhackd::OverlayWindowPos::Bottom => (
                ((self.ui.width() / 2.0) - (size.0 / 2.0)),
                (self.ui.height() - g_height) - padding.1,
            ),
            wallhackd::OverlayWindowPos::BottomLeft => (padding.0, (self.ui.height() - g_height) - padding.1),
            wallhackd::OverlayWindowPos::BottomRight => (
                (self.ui.width() - size.0) - padding.0,
                (self.ui.height() - g_height) - padding.1,
            ),
        };

        self.ui.set_absolute_position(final_pos);

        let mouse_pos = self.ui.mouse_position(input);
        let coll_padding = (16.0, 16.0);

        if (mouse_pos.x > -coll_padding.0 && mouse_pos.x < size.0 + coll_padding.0) &&
            (mouse_pos.y > -coll_padding.1 && mouse_pos.y < g_height + coll_padding.1)
        {
            self.paint_mode = PaintMode::None;
            self.whd.lock_painting = true;
        } else {
            self.whd.lock_painting = false;
        }

        self.ui.push_group((size.0, g_height), Layout::HorizontalRev);

        self.ui.pad((margin, 0.0));

        self.ui.push_group((size.0, g_height), Layout::Vertical);
        self.ui.fill(canvas, self.assets.colors.panel.with_a(200));

        self.ui.push_group((size.0, 32.0), Layout::HorizontalRev);
        self.ui.fill(canvas, self.assets.colors.text_field.text);

        self.ui.text(
            canvas,
            format!("   {}", title).as_str(),
            self.assets.colors.text_field.fill,
            (AlignH::Left, AlignV::Middle),
        );

        let mut res = false;
        let mut changed_colors = ButtonColors {
            outline: self.assets.colors.tool_button.outline,
            hover: self.assets.colors.text_field.fill.with_a(128),
            text: self.assets.colors.text_field.fill,
            pressed: self.assets.colors.tool_button.pressed,
            whd_tooltip_bg: self.assets.colors.tool_button.whd_tooltip_bg,
            whd_tooltip_text: self.assets.colors.tool_button.whd_tooltip_text,
        };

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &changed_colors,
            },
            &self.assets.icons.whd.close,
            "Close".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.lock_painting = false;
            res = true;
        }

        self.ui.pop_group();
        self.ui.push_group(size, Layout::Vertical);

        self.ui.pad((12.0, 12.0));

        res
    }

    fn whd_overlay_window_end(&mut self, _input: &mut Input) {
        self.ui.pop_group();
        self.ui.pop_group();
        self.ui.pop_group();
    }

    fn whd_bar_after_palette_buttons(&mut self, canvas: &mut Canvas, input: &Input) {
        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.palette,
            "RGB Color".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.select_rgb_color_window = !self.whd.select_rgb_color_window;
        }

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.message,
            "Chat".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.chat_window = !self.whd.chat_window;
        }
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
            if self.whd.custom_image.is_some() {
                self.paint_mode = PaintMode::WHDCustomImage;
                self.whd.custom_image_dims = Some(self.whd.custom_image.clone().unwrap().dimensions());
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

                    match image::open(path) {
                        Ok(img) => {
                            self.whd.custom_image = Some(img.clone());
                            self.whd.custom_image_dims = Some(img.dimensions());
                        },
                        Err(err) => log!(self.log, "[WallhackD] Got some error when loading image {}", err),
                    };
                },
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
            format!("Drawing direction ({})", match self.whd.drawing_direction {
                WHDCIDrawingDirection::ToLeft => "To left",
                WHDCIDrawingDirection::ToRight => "To right",
            }),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.drawing_direction = match self.whd.drawing_direction {
                WHDCIDrawingDirection::ToLeft => WHDCIDrawingDirection::ToRight,
                WHDCIDrawingDirection::ToRight => WHDCIDrawingDirection::ToLeft,
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
            &self.assets.icons.whd.pin_drop,
            "Teleport to chunk".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.teleport_to_chunk_window = !self.whd.teleport_to_chunk_window;
        }

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.person_pin_circle,
            "Teleport to person".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.teleport_to_person_window = !self.whd.teleport_to_person_window;
        }

        if Button::with_icon_and_tooltip(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            &self.assets.icons.whd.gps_fixed,
            "Get player real life location".to_owned(),
            WHDTooltipPos::Top,
        )
        .clicked()
        {
            self.whd.get_player_real_life_loc_window = !self.whd.get_player_real_life_loc_window;
        }
    }
}

impl State {
    // TODO: config
    const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(3 * 60);
    const BAR_SIZE: f32 = 32.0;
    pub const TIME_PER_UPDATE: Duration = Duration::from_millis(50);

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

            load_from_file: image_path,
            save_to_file: None,
            last_autosave: Instant::now(),

            error: None,
            log: Log::new(),
            tip: Tip {
                text: "".into(),
                created: Instant::now(),
                visible_duration: Default::default(),
            },

            panning: false,
            viewport: Viewport::new(),

            whd: WHDState {
                drawing_direction: WHDCIDrawingDirection::ToRight,
                custom_image: None,
                custom_image_dims: None,

                printed_room_id: false,
                lock_painting: false,
                previous_chunk_data_timestamp: None,

                teleport_to_chunk_window: false,
                tp_x_textfield: TextField::new(None),
                tp_y_textfield: TextField::new(None),

                select_rgb_color_window: false,
                select_rgb_color_field_r: TextField::new(Some("0")),
                select_rgb_color_field_g: TextField::new(Some("0")),
                select_rgb_color_field_b: TextField::new(Some("0")),
                select_rgb_colors: (0, 0, 0),

                chat_window: false,
                chat_textfeld: TextField::new(None),

                teleport_to_person_window: false,
                teleport_to_person_list_offset: 0,

                get_player_real_life_loc_window: false,
                get_player_real_life_loc_list_offset: 0,

                player_irl_loc_info_window: false,
                player_irl_loc_info: None,
            },
        };
        if this.peer.is_host() {
            log!(this.log, "Welcome to your room!");
            log!(
                this.log,
                "To invite friends, send them the room ID shown in the bottom right corner of your screen."
            );
        }
        this
    }

    fn show_tip(&mut self, text: &str, duration: Duration) {
        self.tip = Tip {
            text: text.into(),
            created: Instant::now(),
            visible_duration: duration,
        };
    }

    fn fellow_stroke(canvas: &mut Canvas, paint_canvas: &mut PaintCanvas, points: &[StrokePoint]) {
        if points.is_empty() {
            return
        } // failsafe

        let mut from = points[0].point;
        let first_index = if points.len() > 1 { 1 } else { 0 };
        for point in &points[first_index..] {
            paint_canvas.stroke(canvas, from, point.point, &point.brush);
            from = point.point;
        }
    }

    fn canvas_data(
        log: &mut Log,
        canvas: &mut Canvas,
        paint_canvas: &mut PaintCanvas,
        chunk_position: (i32, i32),
        png_image: &[u8],
    ) {
        ok_or_log!(log, paint_canvas.decode_png_data(canvas, chunk_position, png_image));
    }

    fn process_log(&mut self, canvas: &mut Canvas) {
        self.log.process_general();
        if !self.whd.chat_window {
            self.ui.draw_on_canvas(canvas, |canvas| {
                let mut paint = Paint::new(Color4f::from(Color::WHITE.with_a(192)), None);
                paint.set_blend_mode(BlendMode::Difference);
                let mut y = self.ui.height() - (self.log.raw_log_vec().len() as f32 - 1.0) * 16.0 - 8.0;
                for (entry, _) in &self.log.raw_log_vec() {
                    canvas.draw_str(&entry, (8.0, y), &self.assets.sans.borrow(), &paint);
                    y += 16.0;
                }
            });
        }
    }

    fn process_canvas(&mut self, canvas: &mut Canvas, input: &mut Input) {
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
                    self.whd_process_canvas_custom_image(canvas, input, canvas_size);
                    self.paint_mode = PaintMode::None;
                }
            } else if input.mouse_button_just_pressed(MouseButton::Right) {
                self.paint_mode = PaintMode::Erase;
                self.whd.custom_image_dims = None;
            }
        }
        if (input.mouse_button_just_released(MouseButton::Left) || input.mouse_button_just_released(MouseButton::Right)) &&
            (self.paint_mode == PaintMode::Paint || self.paint_mode == PaintMode::Erase)
        {
            self.paint_mode = PaintMode::None;
        }

        let brush_size = self.brush_size_slider.value();
        let from = self
            .viewport
            .to_viewport_space(input.previous_mouse_position(), canvas_size);
        let mouse_position = input.mouse_position();
        let to = self.viewport.to_viewport_space(mouse_position, canvas_size);
        if !self.whd.lock_painting {
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
                self.paint_canvas.stroke(canvas, from, to, &brush);
                if self.stroke_buffer.is_empty() {
                    self.stroke_buffer.push(StrokePoint {
                        point: from,
                        brush: brush.clone(),
                    });
                } else if to != self.stroke_buffer.last().unwrap().point {
                    self.stroke_buffer.push(StrokePoint { point: to, brush });
                }

                break
            }
        }

        // panning and zooming

        if self.ui.has_mouse(input) && input.mouse_button_just_pressed(MouseButton::Middle) {
            self.panning = true;
        }
        if input.mouse_button_just_released(MouseButton::Middle) {
            self.panning = false;
        }

        if self.panning {
            let delta_pan = input.previous_mouse_position() - input.mouse_position();
            self.viewport.pan_around(delta_pan);
            let pan = self.viewport.pan();
            let position = format!("{}, {}", (pan.x / 256.0).floor(), (pan.y / 256.0).floor());
            self.show_tip(&position, Duration::from_millis(100));
        }
        if input.mouse_scroll().y != 0.0 {
            self.viewport.zoom_in(input.mouse_scroll().y);
            self.show_tip(&format!("{:.0}%", self.viewport.zoom() * 100.0), Duration::from_secs(3));
        }

        //
        // rendering
        //

        let paint_canvas = &self.paint_canvas;
        self.ui.draw_on_canvas(canvas, |canvas| {
            canvas.save();
            canvas.translate((self.ui.width() / 2.0, self.ui.height() / 2.0));
            canvas.scale((self.viewport.zoom(), self.viewport.zoom()));
            canvas.translate(-self.viewport.pan());

            let mut paint = Paint::new(Color4f::from(Color::WHITE.with_a(240)), None);
            paint.set_anti_alias(true);
            paint.set_blend_mode(BlendMode::Difference);

            paint_canvas.draw_to(canvas, &self.viewport, canvas_size);

            canvas.restore();

            for (_, mate) in self.peer.mates() {
                let cursor = self.viewport.to_screen_space(mate.lerp_cursor(), canvas_size);
                let brush_radius = mate.brush_size * self.viewport.zoom() * 0.5;
                let text_position = cursor + Point::new(brush_radius, brush_radius) + Point::new(0.0, 14.0);
                paint.set_style(skpaint::Style::Fill);
                canvas.draw_str(&mate.nickname, text_position, &self.assets.sans.borrow(), &paint);
                paint.set_style(skpaint::Style::Stroke);
                canvas.draw_circle(cursor, brush_radius, &paint);
            }

            let zoomed_brush_size = brush_size * self.viewport.zoom();
            paint.set_style(skpaint::Style::Stroke);

            if self.whd.custom_image_dims.is_some() {
                let dims = self.whd.custom_image_dims.unwrap();
                let x_off = match self.whd.drawing_direction {
                    WHDCIDrawingDirection::ToLeft => dims.0 as f32,
                    WHDCIDrawingDirection::ToRight => 0.0,
                };

                match self.whd.drawing_direction {
                    WHDCIDrawingDirection::ToLeft => {
                        let x_off2 = ((input.mouse_position().x + self.viewport.pan().x) / 256.0) as i32;
                        let ch_x_off =
                            ((input.mouse_position().x + self.viewport.pan().x) as i32 - (x_off2 * 256)).abs() as u32;

                        canvas.draw_rect(
                            Rect::from_point_and_size(
                                (
                                    (mouse_position.x - (x_off * self.viewport.zoom())) -
                                        (ch_x_off as f32 * self.viewport.zoom()) as f32 -
                                        32.0,
                                    mouse_position.y,
                                ),
                                (
                                    ((dims.0 + ch_x_off) as f32 * self.viewport.zoom()) as i32 + 32,
                                    (dims.1 as f32 * self.viewport.zoom()) as i32,
                                ),
                            ),
                            &paint,
                        );
                    },
                    WHDCIDrawingDirection::ToRight => {
                        canvas.draw_rect(
                            Rect::from_point_and_size(
                                (mouse_position.x - x_off as f32, mouse_position.y),
                                (
                                    (dims.0 as f32 * self.viewport.zoom()) as i32,
                                    (dims.1 as f32 * self.viewport.zoom()) as i32,
                                ),
                            ),
                            &paint,
                        );
                    },
                };
            }

            canvas.draw_circle(mouse_position, zoomed_brush_size * 0.5, &paint);
        });
        if self.tip.created.elapsed() < self.tip.visible_duration {
            self.ui.push_group(self.ui.size(), Layout::Freeform);
            self.ui.pad((32.0, 32.0));
            self.ui.push_group((72.0, 62.0), Layout::Vertical);
            self.ui.fill(canvas, Color::BLACK.with_a(128));
            self.ui.pad((0.0, 8.0));

            self.ui.push_group((self.ui.width(), 20.0), Layout::Vertical);
            self.ui
                .text(canvas, &self.tip.text, Color::WHITE, (AlignH::Center, AlignV::Middle));
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
                    ok_or_log!(
                        self.log,
                        self.paint_canvas.save(Some(&self.save_to_file.as_ref().unwrap()))
                    );
                    self.last_autosave = Instant::now();
                    self.save_to_file = None;

                    if self.assets.whd_commandline.headless_client {
                        log!(self.log, "Saved canvas to file!");
                        std::process::exit(0);
                    }
                }
            } else {
                for chunk_position in self.viewport.visible_tiles(Chunk::SIZE, canvas_size) {
                    if self.server_side_chunks.contains(&chunk_position) &&
                        !self.requested_chunks.contains(&chunk_position)
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
            let y_offset = self.ui.height() *
                if self.paint_color == color {
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

        self.whd_bar_after_palette_buttons(canvas, input);

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
        self.brush_size_slider.process(&mut self.ui, canvas, input, SliderArgs {
            width: 192.0,
            color: self.assets.colors.slider,
        });
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
                .add_filter("NetCanv canvas", &["netcanv", "toml"])
                .show_save_single_file()
            {
                Ok(Some(path)) => {
                    self.save_to_file = Some(path);
                },
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

        // loading from file

        if self.load_from_file.is_some() {
            ok_or_log!(
                self.log,
                self.paint_canvas.load(canvas, &self.load_from_file.take().unwrap())
            );
        }

        // autosaving

        if self.paint_canvas.filename().is_some() && self.last_autosave.elapsed() > Self::AUTOSAVE_INTERVAL {
            eprintln!("autosaving chunks");
            ok_or_log!(self.log, self.paint_canvas.save(None));
            eprintln!("autosave complete");
            self.last_autosave = Instant::now();
        }

        // network

        match self.peer.tick() {
            Ok(messages) =>
                for message in messages {
                    match message {
                        Message::Error(error) => self.error = Some(error),
                        Message::Connected => unimplemented!(
                            "Message::Connected shouldn't be generated after connecting to the matchmaker"
                        ),
                        Message::Left(nickname) => log!(self.log, "{} left the room", nickname),
                        Message::Stroke(points) => Self::fellow_stroke(canvas, &mut self.paint_canvas, &points),
                        Message::ChunkPositions(mut positions) => {
                            eprintln!("received {} chunk positions", positions.len());
                            eprintln!("the positions are: {:?}", &positions);
                            self.server_side_chunks = positions.drain(..).collect();
                        },
                        Message::Chunks(chunks) => {
                            eprintln!("received {} chunks", chunks.len());
                            for (chunk_position, png_data) in chunks {
                                self.whd.previous_chunk_data_timestamp = Some(SystemTime::now());
                                Self::canvas_data(
                                    &mut self.log,
                                    canvas,
                                    &mut self.paint_canvas,
                                    chunk_position,
                                    &png_data,
                                );
                                self.downloaded_chunks.insert(chunk_position);
                            }
                        },
                        Message::WHDChatMessage(msg) => {
                            log!(self.log, "{}", msg);
                        },
                        message => self.deferred_message_queue.push_back(message),
                    }
                },
            Err(error) => {
                self.error = Some(format!("{}", error));
            },
        }

        for message in self.deferred_message_queue.drain(..) {
            match message {
                Message::Joined(nickname, addr) => {
                    log!(self.log, "{} joined the room", nickname);
                    if let Some(addr) = addr {
                        let positions = self.paint_canvas.chunk_positions();
                        ok_or_log!(self.log, self.peer.send_chunk_positions(addr, positions));
                    }
                },
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
                },
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
