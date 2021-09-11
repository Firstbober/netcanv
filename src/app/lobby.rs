use std::{borrow::Borrow, error::Error};
use std::fmt::Display;
use std::net::SocketAddr;
use std::path::PathBuf;

use native_dialog::FileDialog;
use skulpin::skia_safe::*;

use crate::{app::{paint, AppState, StateArgs}, wallhackd::{self, WHDLobbyFunctions}};
use crate::assets::{Assets, ColorScheme};
use crate::net::{Message, Peer};
use crate::ui::*;
use crate::util::get_window_size;


#[derive(Debug)]
enum Status {
    None,
    Info(String),
    Error(String),
}

impl<T: Display> From<T> for Status {
    fn from(error: T) -> Self {
        Self::Error(format!("{}", error))
    }
}

pub struct WHDState {
    host_custom_room_id_expand: Expand,
    room_id_field: TextField,

    last_status_text: String,

    headless_trying_to_host: bool,
    headless_trying_to_join: bool,

    whd_accent: u8,
}

pub struct State {
    assets: Assets,
    ui: Ui,

    // UI elements
    nickname_field: TextField,
    matchmaker_field: TextField,
    room_id_field: TextField,

    join_expand: Expand,
    host_expand: Expand,

    // net
    status: Status,
    peer: Option<Peer>,
    connected: bool,             // when this is true, the state is transitioned to paint::State
    image_file: Option<PathBuf>, // when this is Some, the canvas is loaded from a file

    // wallhackd

    whd: WHDState,
}

impl wallhackd::WHDLobbyFunctions for State {
    fn whd_process_menu_start(&mut self, _canvas: &mut Canvas, _input: &Input) {
        if self.assets.whd_commandline.headless_host {
            if !self.whd.headless_trying_to_host {
                self.whd.headless_trying_to_host = true;

                let whd_cmd = self.assets.whd_commandline.borrow();

                let lc = whd_cmd.load_canvas.clone();

                match lc {
                    Some(st) => self.image_file = Some(PathBuf::from(st)),
                    None => ()
                }

                if self.assets.whd_commandline.roomid.is_some() {
                    match Self::whd_host_room_with_custom_id(
                        whd_cmd.username.clone().unwrap_or("HeadlessServer".to_owned()).as_str(),
                        whd_cmd.matchmaker_addr.clone().unwrap().as_str(),
                        whd_cmd.roomid.clone().unwrap().as_str()
                    ) {
                        Ok(peer) => {
                            self.peer = Some(peer);
                            self.status = Status::None;
                        },
                        Err(status) => self.status = status,
                    }
                } else {
                    match Self::host_room(
                        whd_cmd.username.clone().unwrap_or("HeadlessServer".to_owned()).as_str(),
                        whd_cmd.matchmaker_addr.clone().unwrap().as_str()
                    ) {
                        Ok(peer) => {
                            self.peer = Some(peer);
                            self.status = Status::None;
                        },
                        Err(status) => self.status = status,
                    }
                }

                match &self.status {
                    Status::None => (),
                    Status::Info(info) => {
                        println!("[Info] {}", info);
                    },
                    Status::Error(error) => {
                        println!("[Error] {}", error);
                        std::process::exit(1);
                    }
                }
            }
        }

        if self.assets.whd_commandline.headless_client {
            if !self.whd.headless_trying_to_join {
                self.whd.headless_trying_to_join = true;

                let whd_cmd = self.assets.whd_commandline.borrow();

                match Self::join_room(
                    whd_cmd.username.clone().unwrap().as_str(),
                    whd_cmd.matchmaker_addr.clone().unwrap().as_str(),
                    whd_cmd.roomid.clone().unwrap().as_str()
                ) {
                    Ok(peer) => {
                        println!("Joined room with id {}!", whd_cmd.roomid.clone().unwrap());
                        self.peer = Some(peer);
                        self.status = Status::None;
                    },
                    Err(status) => self.status = status,
                }

                match &self.status {
                    Status::None => (),
                    Status::Info(info) => {
                        println!("[Info] {}", info);
                    },
                    Status::Error(error) => {
                        println!("[Error] {}", error);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
    fn whd_process_menu_expands(&mut self, canvas: &mut Canvas, input: &Input) {
        let expand = ExpandArgs {
            label: "",
            font_size: 22.0,
            icons: &self.assets.icons.expand,
            colors: &self.assets.colors.expand,
        };

        let button = ButtonArgs {
            height: 32.0,
            colors: &self.assets.colors.button,
        };

        let textfield = TextFieldArgs {
            width: 160.0,
            colors: &self.assets.colors.text_field,
            hint: None,
        };

        // wallhackd host room with custom id
        if self.whd.host_custom_room_id_expand.process(&mut self.ui, canvas, input, ExpandArgs {
            label: "[WHD] Host a new room with custom ID",
            .. expand
        })
            .mutually_exclude(&mut self.join_expand)
            .mutually_exclude(&mut self.host_expand)
            .expanded()
        {
            self.ui.push_group(self.ui.remaining_size(), Layout::Vertical);
            self.ui.offset((32.0, 8.0));

            self.ui.paragraph(canvas, self.assets.colors.text, AlignH::Left, None, &[
                "Create a blank canvas, or load one from file.",
                "WallhackD Matchmaker provides function for rooms with custom ID's,",
                "please enter one to start."
            ]);
            self.ui.space(16.0);
            self.ui.push_group((0.0, TextField::labelled_height(&self.ui)), Layout::Horizontal);
            self.whd.room_id_field.with_label(&mut self.ui, canvas, input, "Room ID", TextFieldArgs {
                hint: Some("1-9 digits"),
                .. textfield
            });
            self.ui.offset((16.0, 16.0));

            macro_rules! host_room {
                () => {
                    match Self::whd_host_room_with_custom_id(
                        self.nickname_field.text(),
                        self.matchmaker_field.text(),
                        self.whd.room_id_field.text()
                    ) {
                        Ok(peer) => {
                            self.peer = Some(peer);
                            self.status = Status::None;
                        },
                        Err(status) => self.status = status,
                    }
                };
            }

            if Button::with_text(&mut self.ui, canvas, input, button, "Host").clicked() {
                host_room!();
            }

            self.ui.space(8.0);
            if Button::with_text(&mut self.ui, canvas, input, button, "from File").clicked() {
                match FileDialog::new()
                    .set_filename("canvas.png")
                    .add_filter(
                        "Supported image files",
                        &[
                            "png",
                            "jpg", "jpeg", "jfif",
                            "gif",
                            "bmp",
                            "tif", "tiff",
                            "webp",
                            "avif",
                            "pnm",
                            "tga",
                        ])
                    .show_open_single_file()
                {
                    Ok(Some(path)) => {
                        self.image_file = Some(path);
                        host_room!();
                    },
                    Err(error) => self.status = Status::from(error),
                    _ => (),
                }
            }
            self.ui.pop_group();

            self.ui.fit();
            self.ui.pop_group();
        }
    }

    fn whd_process_right_bar(&mut self, canvas: &mut Canvas, input: &Input) {
        self.ui.space(6.0);

        if Button::with_icon_and_tooltip(
            &mut self.ui, canvas, input, ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            }, &self.assets.icons.whd.wallhackd,
            "WallhackD".to_owned(),
            WHDTooltipPos::Left
        ).clicked() {
            self.whd.whd_accent += 1;

            if self.whd.whd_accent > 5 {
                self.whd.whd_accent = 0
            }

            if self.whd.whd_accent > 0 {
                self.assets.colors = match self.whd.whd_accent {
                    1 => ColorScheme::whd_accent(Color::new(0xffF44336), Color::new(0xff1d1616)),
                    2 => ColorScheme::whd_accent(Color::new(0xffFF5722), Color::new(0xff1c1615)),
                    3 => ColorScheme::whd_accent(Color::new(0xff8BC34A), Color::new(0xff181a16)),
                    4 => ColorScheme::whd_accent(Color::new(0xff2196F3), Color::new(0xff15181b)),
                    5 => ColorScheme::whd_accent(Color::new(0xffFFEB3B), Color::new(0xff1e1d16)),
                    _ => ColorScheme::whd_accent(Color::new(0xff3F51B5), Color::new(0xff121517))
                }
            } else {
                self.assets.colors = ColorScheme::dark();
            }
        }
    }
}

impl State {
    pub fn new(assets: Assets, error: Option<&str>) -> Self {
        let username = assets.whd_commandline.username.clone().unwrap_or("Anon".to_owned());
        let mm_addr = assets.whd_commandline.matchmaker_addr.clone().unwrap_or("localhost:62137".to_owned());
        let roomid = assets.whd_commandline.roomid.clone().unwrap_or("".to_owned());

        Self {
            assets,
            ui: Ui::new(),
            nickname_field: TextField::new(Some(username.as_str())),
            matchmaker_field: TextField::new(Some(mm_addr.as_str())),
            room_id_field: TextField::new(Some(roomid.as_str())),
            join_expand: Expand::new(true),
            host_expand: Expand::new(false),
            status: match error {
                Some(err) => Status::Error(err.into()),
                None => Status::None,
            },
            peer: None,
            connected: false,

            image_file: None,

            whd: WHDState {
                host_custom_room_id_expand: Expand::new(false),
                room_id_field: TextField::new(Some(roomid.as_str())),

                last_status_text: String::new(),

                headless_trying_to_host: false,
                headless_trying_to_join: false,

                whd_accent: 0
            }
        }
    }

    fn process_header(&mut self, canvas: &mut Canvas) {
        self.ui.push_group((self.ui.width(), 92.0), Layout::Vertical);

        self.ui.push_group((self.ui.width(), 56.0), Layout::Freeform);
        self.ui.set_font_size(48.0);
        self.ui.text(canvas, "NetCanv [WHD]", self.assets.colors.text, (AlignH::Left, AlignV::Middle));

        self.ui.pop_group();

        self.ui
            .push_group((self.ui.width(), self.ui.remaining_height()), Layout::Freeform);
        self.ui.text(
            canvas,
            "[WHD] by Firstbober. Welcome! Host a room or join an existing one to start painting.",
            self.assets.colors.text,
            (AlignH::Left, AlignV::Middle),
        );
        self.ui.pop_group();

        self.ui.pop_group();
    }

    fn process_menu(&mut self, canvas: &mut Canvas, input: &mut Input) -> Option<Box<dyn AppState>> {
        self.whd_process_menu_start(canvas, input);

        self.ui.push_group((self.ui.width(), self.ui.remaining_height()), Layout::Vertical);

        let button = ButtonArgs {
            height: 32.0,
            colors: &self.assets.colors.button,
        };
        let textfield = TextFieldArgs {
            width: 160.0,
            colors: &self.assets.colors.text_field,
            hint: None,
        };
        let expand = ExpandArgs {
            label: "",
            font_size: 22.0,
            icons: &self.assets.icons.expand,
            colors: &self.assets.colors.expand,
        };

        // nickname, matchmaker
        self.ui.push_group(
            (self.ui.width(), TextField::labelled_height(&self.ui)),
            Layout::Horizontal,
        );
        self.nickname_field
            .with_label(&mut self.ui, canvas, input, "Nickname", TextFieldArgs {
                hint: Some("Name shown to others"),
                ..textfield
            });
        self.ui.space(16.0);
        self.matchmaker_field
            .with_label(&mut self.ui, canvas, input, "Matchmaker", TextFieldArgs {
                hint: Some("IP address"),
                ..textfield
            });
        self.ui.pop_group();
        self.ui.space(32.0);

        // join room
        if self
            .join_expand
            .process(&mut self.ui, canvas, input, ExpandArgs {
                label: "Join an existing room",
                ..expand
            })
            .mutually_exclude(&mut self.host_expand)
            .mutually_exclude(&mut self.whd.host_custom_room_id_expand)
            .expanded()
        {
            self.ui.push_group(self.ui.remaining_size(), Layout::Vertical);
            self.ui.offset((32.0, 8.0));

            self.ui
                .paragraph(canvas, self.assets.colors.text, AlignH::Left, None, &[
                    "Ask your friend for the Room ID",
                    "and enter it into the text field below.",
                ]);
            self.ui.space(16.0);
            self.ui.push_group((0.0, TextField::labelled_height(&self.ui)), Layout::Horizontal);
            self.room_id_field.with_label(&mut self.ui, canvas, input, "Room ID", TextFieldArgs {
                hint: Some("1-9 digits"),
                .. textfield
            });
            self.ui.offset((16.0, 16.0));
            if Button::with_text(&mut self.ui, canvas, input, button, "Join").clicked() {
                match Self::join_room(
                    self.nickname_field.text(),
                    self.matchmaker_field.text(),
                    self.room_id_field.text(),
                ) {
                    Ok(peer) => {
                        self.peer = Some(peer);
                        self.status = Status::None;
                    },
                    Err(status) => self.status = status,
                }
            }
            self.ui.pop_group();

            self.ui.fit();
            self.ui.pop_group();
        }
        self.ui.space(16.0);

        // host room
        if self
            .host_expand
            .process(&mut self.ui, canvas, input, ExpandArgs {
                label: "Host a new room",
                ..expand
            })
            .mutually_exclude(&mut self.join_expand)
            .mutually_exclude(&mut self.whd.host_custom_room_id_expand)
            .expanded()
        {
            self.ui.push_group(self.ui.remaining_size(), Layout::Vertical);
            self.ui.offset((32.0, 8.0));

            self.ui
                .paragraph(canvas, self.assets.colors.text, AlignH::Left, None, &[
                    "Create a blank canvas, or load an existing one from file,",
                    "and share the Room ID with your friends.",
                ]);
            self.ui.space(16.0);

            macro_rules! host_room {
                () => {
                    match Self::host_room(self.nickname_field.text(), self.matchmaker_field.text()) {
                        Ok(peer) => {
                            self.peer = Some(peer);
                            self.status = Status::None;
                        },
                        Err(status) => self.status = status,
                    }
                };
            }

            self.ui
                .push_group((self.ui.remaining_width(), 32.0), Layout::Horizontal);
            if Button::with_text(&mut self.ui, canvas, input, button, "Host").clicked() {
                host_room!();
            }
            self.ui.space(8.0);
            if Button::with_text(&mut self.ui, canvas, input, button, "from File").clicked() {
                match FileDialog::new()
                    .set_filename("canvas.png")
                    .add_filter("Supported image files", &[
                        "png", "jpg", "jpeg", "jfif", "gif", "bmp", "tif", "tiff", "webp", "avif", "pnm", "tga",
                    ])
                    .add_filter("NetCanv canvas", &["toml"])
                    .show_open_single_file()
                {
                    Ok(Some(path)) => {
                        self.image_file = Some(path);
                        host_room!();
                    },
                    Err(error) => self.status = Status::from(error),
                    _ => (),
                }
            }
            self.ui.pop_group();

            self.ui.fit();
            self.ui.pop_group();
        }

        self.ui.space(16.0);

        self.whd_process_menu_expands(canvas, input);

        self.ui.pop_group();

        chain_focus(input, &mut [
            &mut self.nickname_field,
            &mut self.matchmaker_field,
            &mut self.room_id_field,
        ]);

        None
    }

    fn process_status(&mut self, canvas: &mut Canvas) {
        match self.status.borrow() {
            Status::None => (),
            Status::Info(text) => {
                if self.whd.last_status_text != *text {
                    println!("[netcanv] (status) <info> {}", text);
                    self.whd.last_status_text = text.clone();
                }
            },
            Status::Error(text) => {
                if self.whd.last_status_text != *text {
                    println!("[netcanv] (status) <error> {}", text);
                    self.whd.last_status_text = text.clone();
                }
            }
        }

        if !matches!(self.status, Status::None) {
            self.ui.push_group((self.ui.width(), 84.0), Layout::Horizontal);
            let icon =
                match self.status {
                    Status::None => unreachable!(),
                    Status::Info(_) => &self.assets.icons.status.info,
                    Status::Error(_) => &self.assets.icons.status.error,
                };
            let color =
                match self.status {
                    Status::None => unreachable!(),
                    Status::Info(_) => self.assets.colors.text,
                    Status::Error(_) => self.assets.colors.error,
                };
            self.ui.icon(canvas, icon, color, Some((self.ui.height(), self.ui.height())));
            self.ui.space(8.0);
            self.ui
                .push_group((self.ui.remaining_width(), self.ui.height()), Layout::Freeform);
            let text = match &self.status {
                Status::None => unreachable!(),
                Status::Info(text) | Status::Error(text) => text,
            };
            self.ui.text(canvas, text, color, (AlignH::Left, AlignV::Middle));

            self.ui.pop_group();
            self.ui.pop_group();

            self.ui.push_group((self.ui.width(), 36.0), Layout::Vertical);
        } else {
            self.ui.push_group((self.ui.width(), 120.0), Layout::Vertical);
        }

        self.ui.text(canvas, format!("Netcanv {}", env!("CARGO_PKG_VERSION")).as_str(), self.assets.colors.text_field.text_hint, (AlignH::Left, AlignV::Bottom));
        self.ui.pop_group();

        self.ui.push_group((self.ui.width(), 20.0), Layout::Vertical);
        self.ui.text(canvas, format!("WallhackD {}", wallhackd::WALLHACKD_VERSION).as_str(), self.assets.colors.text_field.text_hint, (AlignH::Left, AlignV::Bottom));
        self.ui.pop_group();

    }

    fn validate_nickname(nickname: &str) -> Result<(), Status> {
        if nickname.is_empty() {
            return Err(Status::Error("Nickname must not be empty".into()))
        }
        if nickname.len() > 24 {
            return Err(Status::Error("The maximum length of a nickname is 24 characters".into()))
        }
        Ok(())
    }

    fn host_room(nickname: &str, matchmaker_addr_str: &str) -> Result<Peer, Status> {
        Self::validate_nickname(nickname)?;
        Ok(Peer::host(nickname, matchmaker_addr_str)?)
    }

    // [WHD] Must be here
    fn whd_host_room_with_custom_id(nickname: &str, matchmaker_addr_str: &str, room_id_str: &str) -> Result<Peer, Status> {
        if !matches!(room_id_str.len(), 1..=9) {
            return Err(Status::Error("Room ID must be a number with 1–9 digits".into()))
        }
        Self::validate_nickname(nickname)?;

        let room_id: u32 = room_id_str.parse()
            .map_err(|_| Status::Error("Room ID must be an integer".into()))?;

        Ok(Peer::whd_host_with_custom_id(nickname, matchmaker_addr_str, room_id)?)
    }

    fn join_room(nickname: &str, matchmaker_addr_str: &str, room_id_str: &str) -> Result<Peer, Status> {
        if !matches!(room_id_str.len(), 1..=9) {
            return Err(Status::Error("Room ID must be a number with 1–9 digits".into()))
        }
        Self::validate_nickname(nickname)?;
        let room_id: u32 = room_id_str
            .parse()
            .map_err(|_| Status::Error("Room ID must be an integer".into()))?;
        Ok(Peer::join(nickname, matchmaker_addr_str, room_id)?)
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
        canvas.clear(self.assets.colors.panel);

        if let Some(peer) = &mut self.peer {
            match peer.tick() {
                Ok(messages) =>
                    for message in messages {
                        match message {
                            Message::Error(error) => self.status = Status::Error(error.into()),
                            Message::Connected => self.connected = true,
                            _ => (),
                        }
                    },
                Err(error) => {
                    self.status = error.into();
                },
            }
        }

        self.ui.begin(get_window_size(&coordinate_system_helper), Layout::Horizontal);
        self.ui.set_font(self.assets.sans.clone());
        self.ui.set_font_size(14.0);

        self.ui.pad((64.0, 64.0));

        self.ui.push_group((self.ui.width(), 384.0), Layout::Vertical);
        self.ui.align((AlignH::Left, AlignV::Top));
        self.process_header(canvas);
        self.ui.space(24.0);
        self.process_menu(canvas, input);
        self.ui.space(24.0);
        self.process_status(canvas);
        self.ui.pop_group();

        self.ui.push_group((32.0, self.ui.height()), Layout::Vertical);
        self.ui.align((AlignH::Right, AlignV::Top));

        if Button::with_icon(
            &mut self.ui,
            canvas,
            input,
            ButtonArgs {
                height: 32.0,
                colors: &self.assets.colors.tool_button,
            },
            if self.assets.dark_mode {
                &self.assets.icons.color_switcher.light
            } else {
                &self.assets.icons.color_switcher.dark
            },
        )
        .clicked()
        {
            self.assets.dark_mode = !self.assets.dark_mode;

            if self.assets.dark_mode {
                self.assets.colors = ColorScheme::dark();
            } else {
                self.assets.colors = ColorScheme::light();
            }
        }

        self.whd_process_right_bar(canvas, input);

        self.ui.pop_group();
    }

    fn next_state(self: Box<Self>) -> Box<dyn AppState> {
        if self.connected {
            Box::new(paint::State::new(self.assets, self.peer.unwrap(), self.image_file))
        } else {
            self
        }
    }
}
