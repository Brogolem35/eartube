mod audio_cache;
mod data;
mod gui;
mod icons;
mod playback;
mod player;
mod thumbnail;
mod yt;

use crate::gui::iced_main;

fn main() -> iced::Result {
	iced_main()
}
