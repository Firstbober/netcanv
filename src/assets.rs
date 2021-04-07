use std::borrow::Borrow;

use skulpin::skia_safe::*;

use crate::{wallhackd};

use crate::ui::{ButtonColors, ExpandColors, ExpandIcons, TextFieldColors};
use crate::util::{RcFont, new_rc_font};

const SANS_TTF: &[u8] = include_bytes!("assets/fonts/Barlow-Medium.ttf");
const SANS_BOLD_TTF: &[u8] = include_bytes!("assets/fonts/Barlow-Bold.ttf");

const CHEVRON_RIGHT_SVG: &[u8] = include_bytes!("assets/icons/chevron-right.svg");
const CHEVRON_DOWN_SVG: &[u8] = include_bytes!("assets/icons/chevron-down.svg");
const INFO_SVG: &[u8] = include_bytes!("assets/icons/info.svg");
const ERROR_SVG: &[u8] = include_bytes!("assets/icons/error.svg");
const SAVE_SVG: &[u8] = include_bytes!("assets/icons/save.svg");

// [WHD]

const ADD_PHOTO_ALTERNATE: &[u8] = include_bytes!("assets/icons/add-photo-alternate.svg");
const REPLAY: &[u8] = include_bytes!("assets/icons/replay.svg");

const DARK_MODE: &[u8] = include_bytes!("assets/icons/dark-mode.svg");
const LIGHT_MODE: &[u8] = include_bytes!("assets/icons/light-mode.svg");

const ARROW_BACK: &[u8] = include_bytes!("assets/icons/arrow-back.svg");
const ARROW_FORWARD: &[u8] = include_bytes!("assets/icons/arrow-forward.svg");

const WALLHACKD: &[u8] = include_bytes!("assets/icons/wallhackd.svg");

// [WHD]

pub enum ColorSchemeType {
    Light,
    Dark
}

pub struct ColorScheme {
    pub text: Color,
    pub panel: Color,
    pub panel2: Color,
    pub separator: Color,
    pub error: Color,

    pub button: ButtonColors,
    pub tool_button: ButtonColors,
    pub expand: ExpandColors,
    pub slider: Color,
    pub text_field: TextFieldColors,

    pub scheme_type: ColorSchemeType
}

pub struct StatusIcons {
    pub info: Image,
    pub error: Image,
}

pub struct FileIcons {
    pub save: Image,
}

pub struct WHDIcons {
    pub load_image: Image,
    pub draw_it_again: Image,

    pub dark_mode: Image,
    pub light_mode: Image,

    pub forward: Image,
    pub backwards: Image,

    pub wallhackd: Image
}

pub struct Icons {
    pub expand: ExpandIcons,
    pub status: StatusIcons,
    pub file: FileIcons,
    pub whd: WHDIcons
}

pub struct Assets {
    pub sans: RcFont,
    pub sans_bold: RcFont,

    pub colors: ColorScheme,
    pub icons: Icons,

    pub whd_commandline: wallhackd::WHDCommandLine
}

impl Assets {

    fn load_icon(data: &[u8]) -> Image {
        use usvg::{FitTo, NodeKind, Tree};

        let tree = Tree::from_data(data, &Default::default())
            .expect("error while loading the SVG file");
        let size = match *tree.root().borrow() {
            NodeKind::Svg(svg) => svg.size,
            _ => panic!("the root node of the SVG is not <svg/>"),
        };
        let mut pixmap = tiny_skia::Pixmap::new(size.width() as u32, size.height() as u32).unwrap();
        resvg::render(&tree, FitTo::Original, pixmap.as_mut());

        let image_info = ImageInfo::new(
            (size.width() as i32, size.height() as i32),
            ColorType::RGBA8888,
            AlphaType::Premul,
            ColorSpace::new_srgb(),
        );
        let stride = pixmap.width() as usize * 4;
        Image::from_raster_data(&image_info, Data::new_copy(pixmap.data()), stride).unwrap()
    }

    pub fn new(colors: ColorScheme) -> Self {
        Self {
            sans: new_rc_font(SANS_TTF, 14.0),
            sans_bold: new_rc_font(SANS_BOLD_TTF, 14.0),
            colors,
            icons: Icons {
                expand: ExpandIcons {
                    expand: Self::load_icon(CHEVRON_RIGHT_SVG),
                    shrink: Self::load_icon(CHEVRON_DOWN_SVG),
                },
                status: StatusIcons {
                    info: Self::load_icon(INFO_SVG),
                    error: Self::load_icon(ERROR_SVG),
                },
                file: FileIcons {
                    save: Self::load_icon(SAVE_SVG),
                },
                whd: WHDIcons {
                    load_image: Self::load_icon(ADD_PHOTO_ALTERNATE),
                    draw_it_again: Self::load_icon(REPLAY),

                    dark_mode: Self::load_icon(DARK_MODE),
                    light_mode: Self::load_icon(LIGHT_MODE),

                    forward: Self::load_icon(ARROW_FORWARD),
                    backwards: Self::load_icon(ARROW_BACK),

                    wallhackd: Self::load_icon(WALLHACKD)
                }
            },

            whd_commandline: wallhackd::WHDCommandLine {
                headless_client: false,
                headless_host: false,

                username: None,
                matchmaker_addr: None,
                roomid: None,

                save_canvas: None,
                load_canvas: None,
            }
        }
    }

    pub fn whd_add_commandline(&mut self, cmd: wallhackd::WHDCommandLine) {
        self.whd_commandline = cmd;
    }

}

impl ColorScheme {
    pub fn light() -> Self {
        let tooltip_bg = Color::new(0xff000000);
        let tooltip_text = Color::new(0xffeeeeee);

        Self {
            text: Color::new(0xff000000),
            panel: Color::new(0xffeeeeee),
            panel2: Color::new(0xffffffff),
            separator: Color::new(0xff202020),
            error: Color::new(0xff7f0000),

            button: ButtonColors {
                outline: Color::new(0x40000000),
                text: Color::new(0xff000000),
                hover: Color::new(0x20000000),
                pressed: Color::new(0x50000000),

                whd_tooltip_bg: tooltip_bg,
                whd_tooltip_text: tooltip_text
            },
            tool_button: ButtonColors {
                outline: Color::new(0x00000000),
                text: Color::new(0xff000000),
                hover: Color::new(0x20000000),
                pressed: Color::new(0x50000000),

                whd_tooltip_bg: tooltip_bg,
                whd_tooltip_text: tooltip_text
            },
            slider: Color::new(0xff000000),
            expand: ExpandColors {
                icon: Color::new(0xff000000),
                text: Color::new(0xff000000),
                hover: Color::new(0x30000000),
                pressed: Color::new(0x60000000),
            },
            text_field: TextFieldColors {
                outline: Color::new(0xff808080),
                outline_focus: Color::new(0xff303030),
                fill: Color::new(0xffffffff),
                text: Color::new(0xff000000),
                text_hint: Color::new(0x7f000000),
                label: Color::new(0xff000000),
            },

            scheme_type: ColorSchemeType::Light
        }
    }

    pub fn whd_dark() -> Self {
        let accent = 0xffFF9800;

        let tooltip_bg = Color::new(0xffeeeeee);
        let tooltip_text = Color::new(0xc5141414);

        Self {
            text: Color::new(0xffeeeeee),
            panel: Color::new(0xc5141414),
            panel2: Color::new(0xffffffff),
            separator: Color::new(0xffFF5722),
            error: Color::new(0xffF44336),

            button: ButtonColors {
                outline: Color::new(accent),
                text: Color::new(accent),
                hover: Color::new(0x30ffffff),
                pressed: Color::new(0x60000000),

                whd_tooltip_bg: tooltip_bg,
                whd_tooltip_text: tooltip_text
            },
            tool_button: ButtonColors {
                outline: Color::new(0x00000000),
                text: Color::new(0xffeeeeee),
                hover: Color::new(0x30ffffff),
                pressed: Color::new(0x60000000),

                whd_tooltip_bg: tooltip_bg,
                whd_tooltip_text: tooltip_text
            },
            slider: Color::new(accent),
            expand: ExpandColors {
                icon: Color::new(accent),
                text: Color::new(0xffeeeeee),
                hover: Color::new(accent),
                pressed: Color::new(0x60000000),
            },
            text_field: TextFieldColors {
                outline: Color::new(0xffeeeeee),
                outline_focus: Color::new(0xffd4d4d4),
                fill: Color::new(0xff171717),
                text: Color::new(0xffeeeeee),
                text_hint: Color::new(0xffbababa),
                label: Color::new(0xffeeeeee),
            },

            scheme_type: ColorSchemeType::Dark
        }
    }

}
