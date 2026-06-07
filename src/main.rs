mod audio_cache;
mod data;
mod gui;
mod icons;
mod playback;
mod player;
mod thumbnail;

use anyhow::Context;
use rustypipe::{client::RustyPipe, model::TrackItem};
use serde::Deserialize;

use crate::gui::iced_main;

fn main() -> iced::Result {
	iced_main()
}

pub async fn search(search_text: &str) -> anyhow::Result<Vec<TrackItem>> {
	// Create a client
	let rp = RustyPipe::new();
	// Fetch the player
	let q = rp.query();
	let sres = q.music_search_tracks(search_text).await?;
	let res = sres.items.items;
	Ok(res)
}

pub async fn new_radio(track: TrackItem) -> anyhow::Result<Vec<TrackItem>> {
	// Create a client
	let rp = RustyPipe::new();
	// Fetch the player
	let q = rp.query();
	let mut radio = q.music_radio_track(&track.id).await?.items;
	radio.insert(0, track);

	Ok(radio)
}

pub async fn get_stream_url(youtube_url: &str) -> anyhow::Result<String> {
	#[derive(Debug, Deserialize)]
	struct YtDlpJson {
		url: String,
	}

	let output = tokio::process::Command::new("yt-dlp")
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
