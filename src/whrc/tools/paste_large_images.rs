use crate::app::paint::tools::Tool;
use crate::assets::Assets;
use crate::backend::{Backend, Image};
use crate::common::{Error, VectorMath};
use crate::config::config;
use crate::paint_canvas::Chunk;
use crate::ui::{Button, ButtonArgs, Tooltip, UiInput};
use image::{DynamicImage, GenericImageView};
use native_dialog::FileDialog;
use netcanv_protocol::relay::PeerId;
use netcanv_renderer::paws::{Color, Rect, Renderer};
use netcanv_renderer::RenderBackend;
use netcanv_renderer_opengl::winit::event::MouseButton;
use nysa::global as bus;

pub struct WHRCToolPasteLargeImages {
   pub icon: Image,
   pub icon_attach_file: Image,

   pub image_to_paste: Option<DynamicImage>,
}

impl WHRCToolPasteLargeImages {
   pub fn new(renderer: &mut Backend) -> Self {
      Self {
         icon: Assets::load_svg(
            renderer,
            include_bytes!("../assets/tools/paste-large-images.svg"),
         ),
         icon_attach_file: Assets::load_svg(
            renderer,
            include_bytes!("../assets/tools/attach-file.svg"),
         ),

         image_to_paste: None,
      }
   }
}

impl Tool for WHRCToolPasteLargeImages {
   fn name(&self) -> &'static str {
      "whrc-paste-large-images"
   }

   fn icon(&self) -> &Image {
      &self.icon
   }

   fn key_shortcut(&self) -> crate::keymap::KeyBinding {
      config().keymap.brush.decrease_thickness
   }

   fn process_bottom_bar(&mut self, args: crate::app::paint::tools::ToolArgs) {
      if Button::with_icon(
         args.ui,
         args.input,
         &ButtonArgs::new(args.ui, &args.assets.colors.toolbar_button)
            .tooltip(&args.assets.sans, Tooltip::top(&args.assets.tr.whrc.get("select-image-to-paste"))),
         &self.icon_attach_file,
      )
      .clicked()
      {
         match FileDialog::new()
            .add_filter("Supported image files", &["png", "jpg", "jpeg", "jfif"])
            .show_open_single_file()
         {
            Ok(Some(path)) => match image::open(path) {
               Ok(image) => {
                  self.image_to_paste = Some(image);
               }
               Err(err) => bus::push(Error(err.into())),
            },
            Err(err) => {
               bus::push(Error(err.into()));
            }
            _ => (),
         }
      }
   }

   fn process_paint_canvas_input(
      &mut self,
      args: crate::app::paint::tools::ToolArgs,
      paint_canvas: &mut crate::paint_canvas::PaintCanvas,
      viewport: &crate::viewport::Viewport,
   ) {
      if self.image_to_paste.is_some() {
         let image = self.image_to_paste.as_ref().unwrap();
         let mouse_position = args.ui.mouse_position(args.input);

         let rect = Rect::new(
            viewport.to_viewport_space(mouse_position, args.ui.size()).floor(),
            (image.width() as f32, image.height() as f32),
         );

         args.ui.draw(|ui| {
            let top_left = viewport.to_screen_space(rect.top_left(), ui.size()).floor();
            let bottom_right = viewport.to_screen_space(rect.bottom_right(), ui.size()).floor();

            let rect = Rect::new(top_left, bottom_right - top_left);

            let renderer = ui.render();
            renderer.outline(rect, Color::rgb(0x0397fb), 0.0, 2.0);
         });

         match args.input.action(MouseButton::Left) {
            (true, crate::ui::ButtonState::Pressed) => {
               let renderer = args.ui.render();

               let im = renderer.create_image_from_rgba(
                  image.width(),
                  image.height(),
                  &image.to_rgba8(),
               );
               paint_canvas.draw(renderer, rect, |renderer| {
                  renderer.image(rect, &im);
               });

               let mut chunks_to_send: Vec<((i32, i32), Vec<u8>)> = Vec::new();
               let (left, top, bottom, right) =
                  crate::paint_canvas::PaintCanvas::chunk_coverage(rect);

               assert!(left <= right);
               assert!(top <= bottom);
               for y in top..=bottom {
                  for x in left..=right {
                     let chunk = paint_canvas.chunks.get(&(x, y)).unwrap();

                     let image = DynamicImage::ImageRgba8(chunk.download_image());
                     let encoder = webp::Encoder::from_image(&image).unwrap();
                     let data = encoder.encode(Chunk::WEBP_QUALITY).to_owned();

                     chunks_to_send.push(((x, y), data));
                  }
               }

               args.net.peer.send_chunks(PeerId::BROADCAST, chunks_to_send).unwrap();
            }
            _ => {}
         }
      }
   }
}
