use netcanv_renderer_opengl::winit::window::WindowBuilder;
use crate::backend::Image;

// -------------
// main.rs hooks

#[macro_export]
macro_rules! whrc_main_after_config {
   () => {
      log::info!(
         "WallhackRC {} - A rehydrated NetCanv expansion pack.",
         whrc_common::WALLHACKRC_VERSION
      );
   };
}

pub fn whrc_main_window_builder(b: WindowBuilder) -> WindowBuilder {
   b.with_title("[WHRC] Netcanv")
}

// main.rs hooks
// -------------

// ---------------
// assets.rs hooks



// assets.rs hooks
// ---------------

pub const WHRC_LOGO: &[u8] = include_bytes!("whrc/assets/wallhackrc.svg");

pub struct WallhackRCIcons {
	pub whrc_logo: Image
}

#[macro_export]
macro_rules! whrc_assets_icons_new {
	($renderer: expr) => {
		crate::whrc::WallhackRCIcons {
			whrc_logo: Self::load_svg($renderer, crate::whrc::WHRC_LOGO)
		}
	};
}

// ------------------
// app/lobby.rs hooks

// const = NetCanv default button count + custom button count
pub const WHRC_APP_LOBBY_ICON_PANEL_BUTTON_COUNT: f32 = 2.0 + 1.0;

#[macro_export]
macro_rules! whrc_app_lobby_process_icon_panel {
   ($ui: expr, $input: expr, $assets: expr) => {
      $ui.space(4.0);

      if Button::with_icon(
         $ui,
         $input,
         &ButtonArgs::new($ui, &$assets.colors.action_button).height(32.0).pill().tooltip(
            &$assets.sans,
            Tooltip::left(format!("WallhackRC {}", whrc_common::WALLHACKRC_VERSION)),
         ),
         &$assets.icons.whrc.whrc_logo,
      )
      .clicked()
      {}
   };
}

// app/lobby.rs hooks
// ------------------
