mod data;
mod gui;
mod playback;
mod player;
mod thumbnail;
mod icons;

use anyhow::Context;
use rustypipe::{
	client::RustyPipe,
	model::{MusicItem, TrackItem},
};
use serde::Deserialize;
use tokio::process::Command;

use crate::gui::iced_main;

fn main() -> iced::Result {
	iced_main()
}

pub async fn rp_testing(search_text: &str) -> anyhow::Result<Vec<TrackItem>> {
	// Create a client
	let rp = RustyPipe::new();
	// Fetch the player
	let q = rp.query();
	let sres = q.music_search_main(search_text).await?;
	let item = sres
		.items
		.items
		.iter()
		.find_map(|i| match i {
			MusicItem::Track(track_item) => Some(track_item),
			_ => None,
		})
		.context("No such track found")?;
	let mut radio = q.music_radio_track(&item.id).await?.items;
	radio.insert(0, item.clone());

	Ok(radio)
}

pub async fn get_stream_url(youtube_url: &str) -> anyhow::Result<String> {
	#[derive(Debug, Deserialize)]
	struct YtDlpJson {
		url: String,
	}

	let output = Command::new("yt-dlp")
		.args(["-f", "ba[ext=m4a]", "--dump-single-json", youtube_url])
		.output()
		.await?;

	if !output.status.success() {
		anyhow::bail!("yt-dlp failed: {}", String::from_utf8_lossy(&output.stderr));
	}

	let json: YtDlpJson =
		serde_json::from_slice(&output.stdout).context("failed to parse yt-dlp json")?;

	Ok(json.url)
}
