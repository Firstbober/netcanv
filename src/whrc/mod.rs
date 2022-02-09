use std::collections::HashMap;
use std::sync::Arc;

use netcanv_protocol::relay::RoomId;
use netcanv_renderer_opengl::winit::window::WindowBuilder;

use crate::backend::Image;
use crate::net::peer::Peer;

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

pub const WHRC_LOGO: &[u8] = include_bytes!("./assets/wallhackrc.svg");

pub struct WallhackRCIcons {
   pub whrc_logo: Image,
}

#[macro_export]
macro_rules! whrc_assets_new_icons {
   ($renderer: expr) => {
      crate::whrc::WallhackRCIcons {
         whrc_logo: Self::load_svg($renderer, crate::whrc::WHRC_LOGO),
      }
   };
}

// assets.rs hooks
// ---------------

// ------------------
// app/lobby.rs hooks

// const = custom button count
pub const WHRC_APP_LOBBY_ICON_PANEL_BUTTON_COUNT: f32 = 1.0;

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

pub struct WHRCAppLobbyHostRoomArgs {
   pub custom_room_id: Option<String>,
}

#[macro_export]
macro_rules! whrc_app_lobby_macro_host_room {
   ($self: expr) => {
      WHRCAppLobbyHostRoomArgs {
         custom_room_id: if $self.room_id_field.text().len() > 0 {
            Some($self.room_id_field.text().into())
         } else {
            None
         },
      }
   };
}

#[macro_export]
macro_rules! whrc_app_lobby_process_menu_host_expand_horizontal {
   ($self: expr, $ui: expr, $input: expr, $assets: expr) => {
      let textfield = TextFieldArgs {
         font: &$assets.sans,
         width: 160.0,
         colors: &$assets.colors.text_field,
         hint: None,
      };

      $self.room_id_field.with_label(
         $ui,
         $input,
         &$assets.sans,
         "[whrc] Custom Room ID",
         TextFieldArgs {
            hint: Some("6 characters"),
            font: &$assets.monospace,
            ..textfield
         },
      );

      $ui.offset(vector(8.0, 16.0));
   };
}

pub trait WHRCPeerFuncs {
   fn whrc_custom_id_host(
      socket_system: Arc<crate::net::socket::SocketSystem>,
      nickname: &str,
      relay_address: &str,
      args: WHRCAppLobbyHostRoomArgs,
   ) -> Self;
}

impl WHRCPeerFuncs for Peer {
   fn whrc_custom_id_host(
      socket_system: Arc<crate::net::socket::SocketSystem>,
      nickname: &str,
      relay_address: &str,
      args: WHRCAppLobbyHostRoomArgs,
   ) -> Self {
      let socket_receiver = socket_system.connect(relay_address.to_owned());
      Self {
         token: crate::net::peer::PeerToken(crate::net::peer::PEER_TOKEN.next()),
         state: crate::net::peer::State::WaitingForRelay(socket_receiver),
         relay_socket: None,
         is_host: true,
         nickname: nickname.into(),
         room_id: Some(RoomId::try_from(args.custom_room_id.unwrap().as_str()).unwrap()),
         peer_id: None,
         mates: HashMap::new(),
         host: None,
      }
   }
}

#[macro_export]
macro_rules! whrc_app_lobby_host_room {
   ($self: expr, $socket_system: expr, $nickname: expr, $relay_addr_str: expr, $whrc_host_args: expr) => {
      if $whrc_host_args.custom_room_id.is_some() {
         if $whrc_host_args.custom_room_id.as_ref().unwrap().len() != 6 {
            return Err(Status::Error(
               "Room ID must be a code with 6 characters".into(),
            ));
         }

         Ok(Peer::whrc_custom_id_host(
            $socket_system,
            $nickname,
            $relay_addr_str,
            $whrc_host_args,
         ))
      } else {
         Ok(Peer::host($socket_system, $nickname, $relay_addr_str))
      }
   };
}

// app/lobby.rs hooks
// ------------------

// ---------------------------
// net/peer.rs hooks

#[macro_export]
macro_rules! whrc_net_peer_connected_to_relay {
   ($self: expr) => {
      $self.send_to_relay(if $self.is_host && $self.room_id.is_some() {
         relay::Packet::WHRCHostCustomId($self.room_id.unwrap())
      } else if $self.is_host {
         relay::Packet::Host
      } else {
         relay::Packet::Join($self.room_id.unwrap())
      })?;
   };
}

// net/peer.rs  hooks
// ------------------

// ---------------------------
// app/paint/tool_bar.rs hooks

pub mod tools;

#[macro_export]
macro_rules! whrc_app_paint_tool_bar_register_tools {
   ($toolbar: expr, $renderer: expr) => {
      use crate::whrc::tools;

      $toolbar.add_tool(tools::paste_large_images::WHRCToolPasteLargeImages::new(
         $renderer,
      ))
   };
}

// app/paint/tool_bar.rs hooks
// ---------------------------
