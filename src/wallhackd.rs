pub const WALLHACKD_VERSION: &str = "1.0.3";

pub struct WallhackDCommandline {
	pub headless_client: bool,
	pub headless_host: bool,

	pub username: Option<String>,
	pub matchmaker_addr: Option<String>,
	pub roomid: Option<String>,

	pub save_canvas: Option<String>,
	pub load_canvas: Option<String>
}