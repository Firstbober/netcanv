use std::error::Error;

use skulpin::*;

use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::WindowBuilder;

#[cfg(target_os = "linux")]
use winit::platform::unix::WindowBuilderExtUnix;

mod app;
mod assets;
mod net;
mod paint_canvas;
mod ui;
mod util;
mod viewport;

mod wallhackd;

use app::*;
use assets::*;
use ui::input::*;

const NETCANV_ICON: &[u8] = include_bytes!("../appimage/netcanv.png");

fn main() -> Result<(), Box<dyn Error>> {
    let clp_matches = clap::App::new("netcanv WallhackD")
        .version(wallhackd::WALLHACKD_VERSION)
        .author("lqdev <liquidekgaming@gmail.com>, Firstbober <firstbober@tutanota.com>")
        .about("Multiplayer Paint but with wallhack")
        .arg(
            clap::Arg::with_name("headless_host")
                .short("e")
                .long("headless_host")
                .takes_value(false)
                .requires("mm_address")
                .help("Launches netcanv whd in headless mode as host")
                .conflicts_with("headless_client"),
        )
        .arg(
            clap::Arg::with_name("headless_client")
                .short("c")
                .long("headless_client")
                .takes_value(false)
                .requires_all(&["mm_address", "username", "roomid"])
                .help("Launches netcanv whd in headless mode as client")
                .conflicts_with("headless_host"),
        )
        .arg(
            clap::Arg::with_name("mm_address")
                .short("m")
                .long("mm_address")
                .takes_value(true)
                .help("Address of matchmaker to use"),
        )
        .arg(
            clap::Arg::with_name("roomid")
                .short("r")
                .long("roomid")
                .takes_value(true)
                .help("Room ID to use, works only on WallhackD matchmakers"),
        )
        .arg(
            clap::Arg::with_name("save_canvas")
                .short("s")
                .long("save_canvas")
                .takes_value(true)
                .help("Save canvas, enter path where file should be saved")
                .conflicts_with("headless_host"),
        )
        .arg(
            clap::Arg::with_name("load_canvas")
                .short("l")
                .long("load_canvas")
                .takes_value(true)
                .help("Load canvas, enter path where file is located")
                .conflicts_with("headless_client"),
        )
        .arg(
            clap::Arg::with_name("username")
                .short("u")
                .long("username")
                .takes_value(true)
                .help("Username to use"),
        )
        .get_matches();

    let mut whd_cmd = wallhackd::WHDCommandLine {
        headless_client: false,
        headless_host: false,

        username: None,
        matchmaker_addr: None,
        roomid: None,

        save_canvas: None,
        load_canvas: None,
    };

    if clp_matches.is_present("headless_host") {
        whd_cmd.headless_host = true;
    }
    if clp_matches.is_present("headless_client") {
        whd_cmd.headless_client = true;
    }

    macro_rules! resolve_str {
        ($name:literal) => {
            match clp_matches.value_of($name) {
                Some(s) => Some(String::from(s)),
                None => None
            }
        };
    }

    whd_cmd.username = resolve_str!("username");
    whd_cmd.matchmaker_addr = resolve_str!("mm_address");
    whd_cmd.roomid = resolve_str!("roomid");

    whd_cmd.save_canvas = resolve_str!("save_canvas");
    whd_cmd.load_canvas = resolve_str!("load_canvas");

    if whd_cmd.headless_client || whd_cmd.headless_host {
        println!("Starting in headless mode");

        let mut headless_canvas = skia_safe::Canvas::new_null();

        let mut input = Input::new();
        let mut assets = Assets::new(ColorScheme::light());

        assets.whd_add_commandline(whd_cmd);

        let mut app: Option<Box<dyn AppState>> = Some(Box::new(lobby::State::new(assets, None)) as _);

        let coordinate_system_helper = CoordinateSystemHelper::new(
            skulpin::ash::vk::Extent2D {width: 1024, height: 600},
            skulpin::LogicalSize::new(1024, 600),
            PhysicalSize::new(1024, 600),
            1.0,
        );
        coordinate_system_helper.use_logical_coordinates(&mut headless_canvas);

        loop {
            app.as_mut().unwrap().process(StateArgs {
                canvas: &mut headless_canvas,
                coordinate_system_helper: &coordinate_system_helper,
                input: &mut input,
            });
            app = Some(app.take().unwrap().next_state());
        }
    } else {
        let event_loop = EventLoop::new();
        let winit_window = {
            let mut b = WindowBuilder::new()
                .with_inner_size(LogicalSize::new(1024, 600))
                .with_title("[WHD] NetCanv")
                .with_resizable(true);
            #[cfg(target_os = "linux")]
            {
                b = b.with_app_id("netcanv".into())
            }
            b
        }
        .build(&event_loop)?;

        winit_window.set_window_icon(
            Some(winit::window::Icon::from_rgba(::image::load_from_memory(NETCANV_ICON).unwrap().to_bytes(), 512, 512).unwrap())
        );

        let window = WinitWindow::new(&winit_window);
        let mut renderer = RendererBuilder::new()
            .use_vulkan_debug_layer(false)
            .build(&window)?;

        let mut assets = Assets::new(ColorScheme::whd_dark());
        assets.whd_add_commandline(whd_cmd);
        let mut app: Option<Box<dyn AppState>> =
            Some(Box::new(lobby::State::new(assets, None)) as _);
        let mut input = Input::new();

        event_loop.run(move |event, _, control_flow| {
            let window = WinitWindow::new(&winit_window);
            *control_flow = ControlFlow::Poll;

            match event {
                Event::WindowEvent { event, .. } => {
                    if let WindowEvent::CloseRequested = event {
                        *control_flow = ControlFlow::Exit;
                    } else {
                        input.process_event(&event);
                    }
                }

                Event::MainEventsCleared => {
                    renderer
                        .draw(&window, |canvas, csh| {
                            // unwrap always succeeds here as app is never None
                            // i don't really like this method chaining tho
                            app.as_mut().unwrap().process(StateArgs {
                                canvas,
                                coordinate_system_helper: &csh,
                                input: &mut input,
                            });
                            app = Some(app.take().unwrap().next_state());
                        })
                        .unwrap();
                    input.finish_frame();
                }

                _ => (),
            }
        });
    }
}
