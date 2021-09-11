use skulpin::skia_safe::*;

use crate::ui::*;
use crate::util::*;

pub enum WHDTooltipPos {
    Top,
    Left,
    TopLeft
}

pub struct WHDButtonProps {
    tooltip: Option<String>,
    tooltip_position: Option<WHDTooltipPos>
}

pub struct Button;

#[derive(Clone)]
pub struct ButtonColors {
    pub outline: Color,
    pub text: Color,
    pub hover: Color,
    pub pressed: Color,

    pub whd_tooltip_bg: Color,
    pub whd_tooltip_text: Color
}

#[derive(Clone, Copy)]
pub struct ButtonArgs<'a> {
    pub height: f32,
    pub colors: &'a ButtonColors,
}

pub struct ButtonProcessResult {
    clicked: bool,
}

impl Button {
    pub fn process(
        ui: &mut Ui,
        canvas: &mut Canvas,
        input: &Input,
        ButtonArgs { height, colors }: ButtonArgs,
        width_hint: Option<f32>,
        extra: impl FnOnce(&mut Ui, &mut Canvas),
        whd_button_props: WHDButtonProps
    ) -> ButtonProcessResult {
        // horizontal because we need to fit() later
        ui.push_group((width_hint.unwrap_or(0.0), height), Layout::Horizontal);

        extra(ui, canvas);
        ui.fit();

        let paint = Paint::new(Color4f::from(colors.whd_tooltip_bg), None);
        let paint2 = Paint::new(Color4f::from(colors.whd_tooltip_text), None);

        let mut clicked = false;
        ui.outline(canvas, colors.outline, 1.0);
        if ui.has_mouse(input) {
            let fill_color = if input.mouse_button_is_down(MouseButton::Left) {
                colors.pressed
            } else {
                colors.hover
            };
            ui.fill(canvas, fill_color);
            clicked = input.mouse_button_just_released(MouseButton::Left);

            if whd_button_props.tooltip.is_some() {
                if !input.mouse_button_is_down(MouseButton::Left) {
                    ui.draw_on_canvas(canvas, |canvas| {
                        let text_size = ui.text_size(whd_button_props.tooltip.clone().unwrap().as_str());

                        let x_off = 20.0;
                        let y_off = 18.0;

                        let tlp_pos = whd_button_props.tooltip_position.unwrap();

                        let pos_rect: (i32, i32) = match tlp_pos {
                            WHDTooltipPos::Top => (
                                -(((text_size.0 + x_off) - ui.width()) / 2.0) as i32,
                                -(text_size.1 + y_off+8.0) as i32
                            ),
                            WHDTooltipPos::Left => (
                                -(text_size.0 + x_off + 8.0) as i32,
                                -(((text_size.1 + y_off) - ui.height()) / 2.0) as i32
                            ),
                            WHDTooltipPos::TopLeft => (
                                -((((text_size.0 + x_off) - ui.width()) / 2.0) + (text_size.0 / 2.0)) as i32,
                                -(text_size.1 + y_off+8.0) as i32
                            ),
                        };

                        let pos_text: (i32, i32) = match tlp_pos {
                            WHDTooltipPos::Top => (
                                -(((text_size.0) - ui.width()) / 2.0) as i32,
                                -(text_size.1 + (y_off / 2.0) - 1.0) as i32
                            ),
                            WHDTooltipPos::Left => (
                                -(text_size.0 + (x_off)) as i32,
                                -((text_size.1) - ui.height()) as i32
                            ),
                            WHDTooltipPos::TopLeft => (
                                -((((text_size.0) - ui.width()) / 2.0) + (text_size.0 / 2.0)) as i32,
                                -(text_size.1 + (y_off / 2.0) - 1.0) as i32
                            ),
                        };

                        let rect = Rect::from_point_and_size(pos_rect, (text_size.0 + x_off, text_size.1 + y_off));
                        canvas.draw_rect(rect, &paint);

                        let font = ui.borrow_font_mut();
                        canvas.draw_str(whd_button_props.tooltip.unwrap().as_str(), pos_text, &font, &paint2);
                    });
                }
            }
        }

        ui.pop_group();

        ButtonProcessResult { clicked }
    }

    pub fn with_text(
        ui: &mut Ui,
        canvas: &mut Canvas,
        input: &Input,
        args: ButtonArgs,
        text: &str,
    ) -> ButtonProcessResult {
        Self::process(ui, canvas, input, args, None, |ui, canvas| {
            let text_width = ui.text_size(text).0;
            let padding = args.height;
            ui.push_group((text_width + padding, ui.height()), Layout::Freeform);
            ui.text(canvas, text, args.colors.text, (AlignH::Center, AlignV::Middle));
            ui.pop_group();
        }, WHDButtonProps {
            tooltip: None,
            tooltip_position: None
        })
    }

    pub fn with_icon(
        ui: &mut Ui,
        canvas: &mut Canvas,
        input: &Input,
        args: ButtonArgs,
        icon: &Image,
    ) -> ButtonProcessResult {
        Self::process(ui, canvas, input, args, Some(args.height), |ui, canvas| {
            ui.icon(canvas, icon, args.colors.text, Some((args.height, args.height)));
        }, WHDButtonProps {
            tooltip: None,
            tooltip_position: None
        })
    }

    pub fn with_icon_and_tooltip(
        ui: &mut Ui,
        canvas: &mut Canvas,
        input: &Input,
        args: ButtonArgs,
        icon: &Image,
        tooltip_text: String,
        tooltip_pos: WHDTooltipPos
    ) -> ButtonProcessResult {
        Self::process(ui, canvas, input, args, Some(args.height), |ui, canvas| {
            ui.icon(canvas, icon, args.colors.text, Some((args.height, args.height)));
        }, WHDButtonProps {
            tooltip: Some(tooltip_text),
            tooltip_position: Some(tooltip_pos)
        })
    }
}

impl ButtonProcessResult {
    pub fn clicked(self) -> bool {
        self.clicked
    }
}
